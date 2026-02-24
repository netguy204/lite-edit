<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `alacritty_terminal` library already handles DSR (Device Status Report) escape sequences internally. When a hosted program sends `ESC[6n` (DSR with argument 6), alacritty's `device_status(6)` method:

1. Reads the cursor position from `self.grid.cursor.point`
2. Formats a CPR response: `format!("\x1b[{};{}R", pos.line + 1, pos.column + 1)`
3. Emits `Event::PtyWrite(text)` via the event listener

The problem is that the current `EventProxy` implementation in `terminal_buffer.rs` ignores all events:

```rust
impl EventListener for EventProxy {
    fn send_event(&self, _event: Event) {
        // We could capture title changes, bell events, etc. here
        // For now, we ignore them
    }
}
```

**Solution:** Modify `EventProxy` to capture `Event::PtyWrite` events using a thread-safe channel, then drain that channel during `poll_events()` and write the responses back to the PTY via `PtyHandle::write()`.

This follows the existing event-handling pattern: just as PTY output flows from the reader thread via `crossbeam_channel` to the main thread for processing, terminal-generated responses will flow from alacritty's event listener to the main thread for PTY write-back.

### Why Channel-Based Approach

The `EventListener::send_event` method takes `&self`, not `&mut self`, so we cannot directly write to the PTY. Using a channel:
- Maintains the immutable borrow requirement of `EventListener`
- Keeps PTY writes on the main thread (same thread as `poll_events()`)
- Follows the existing `crossbeam_channel` pattern already used for PTY output

### Testing Strategy

Per TESTING_PHILOSOPHY.md, we'll use TDD for the meaningful behavioral aspect:
1. Write a test that sends DSR escape sequence (`\x1b[6n`) to a terminal
2. Verify the terminal responds with correct CPR format (`\x1b[row;colR`)
3. Verify cursor position in response matches `cursor_info()` position

Since the PTY round-trip involves real process I/O, this will be an integration test that spawns a shell, sends the DSR query, and verifies the response appears in the echoed output.

## Sequence

### Step 1: Write failing integration test for DSR/CPR round-trip

Create a new test in `crates/terminal/tests/integration.rs` that:
1. Spawns a shell in a `TerminalBuffer`
2. Uses `stty` or similar to query cursor position via DSR
3. Verifies the response is written back and appears correctly

The test should fail initially because `EventProxy` ignores `PtyWrite` events.

```rust
/// Test that DSR (Device Status Report) escape sequences receive CPR responses.
#[test]
fn test_dsr_cursor_position_report() {
    // 1. Create terminal and spawn shell
    // 2. Send DSR query via printf "\033[6n"
    // 3. Poll events and verify CPR response was written back to PTY
    // 4. Check that cursor position matches cursor_info()
}
```

Location: `crates/terminal/tests/integration.rs`

### Step 2: Add EventSender struct with crossbeam channel

Create an `EventSender` struct that wraps a `crossbeam_channel::Sender<Event>` and implements `EventListener`. This replaces the current `EventProxy`.

```rust
// Chunk: docs/chunks/tty_cursor_reporting - DSR/CPR event forwarding
use crossbeam_channel::{unbounded, Receiver, Sender};

struct EventSender {
    tx: Sender<Event>,
}

impl EventListener for EventSender {
    fn send_event(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 3: Update TerminalBuffer to store event receiver

Add an `event_rx: Receiver<Event>` field to `TerminalBuffer` and update the constructor to create the channel and pass the sender to `Term::new()`.

```rust
pub struct TerminalBuffer {
    term: Term<EventSender>,  // Changed from Term<EventProxy>
    // ... existing fields ...
    event_rx: Receiver<Event>,  // New field
}

impl TerminalBuffer {
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self {
        let (event_tx, event_rx) = unbounded();
        let event_sender = EventSender { tx: event_tx };
        let term = Term::new(config, &size, event_sender);
        // ...
        Self {
            term,
            // ...
            event_rx,
        }
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 4: Process terminal events in poll_events()

Modify `poll_events()` to drain the terminal event channel and handle `Event::PtyWrite` by writing to the PTY.

```rust
pub fn poll_events(&mut self) -> bool {
    // ... existing PTY event polling ...

    // Chunk: docs/chunks/tty_cursor_reporting - Process terminal-generated events
    // Handle events from alacritty_terminal (DSR responses, etc.)
    while let Ok(event) = self.event_rx.try_recv() {
        match event {
            Event::PtyWrite(text) => {
                if let Some(ref mut pty) = self.pty {
                    let _ = pty.write(text.as_bytes());
                }
            }
            // Other events (Title, Bell, etc.) could be handled here in the future
            _ => {}
        }
        processed_any = true;
    }

    // ... existing damage tracking ...
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 5: Add unit test for EventSender

Add a unit test verifying that `EventSender` correctly forwards events through the channel.

```rust
#[test]
fn test_event_sender_forwards_pty_write() {
    use alacritty_terminal::event::{Event, EventListener};
    use crossbeam_channel::unbounded;

    let (tx, rx) = unbounded();
    let sender = EventSender { tx };

    sender.send_event(Event::PtyWrite("test".to_string()));

    let received = rx.try_recv();
    assert!(matches!(received, Ok(Event::PtyWrite(s)) if s == "test"));
}
```

Location: `crates/terminal/src/terminal_buffer.rs` (in `#[cfg(test)]` module)

### Step 6: Run tests and verify

Run the test suite to verify:
1. The new DSR/CPR integration test passes
2. All existing terminal tests continue to pass
3. No regressions in terminal behavior

```bash
cargo test -p lite-edit-terminal
```

### Step 7: Manual verification with Claude Code (optional)

If possible, manually test by running Claude Code inside lite-edit's terminal and verifying the cursor is positioned correctly on the prompt line, not one row below.

## Risks and Open Questions

1. **Thread safety of channel send**: The `send_event` call happens on the main thread during `processor.advance()`, so there's no actual threading concern. The channel is an implementation detail to work around the `&self` constraint.

2. **Event ordering**: Terminal events from `alacritty_terminal` and PTY output from the reader thread go through separate channels. This should be fine since:
   - PTY output triggers `processor.advance()` which generates terminal events
   - Terminal events are processed immediately after in the same `poll_events()` call
   - DSR responses are synchronous reactions to DSR requests in the output stream

3. **Other Event types**: We're only handling `Event::PtyWrite` for now. Other events like `Event::Title`, `Event::Bell`, `Event::ClipboardStore` could be useful in the future but are out of scope for this chunk.

4. **Scrollback offset in CPR**: The DSR response from alacritty uses `grid.cursor.point` which is in viewport coordinates, not document coordinates. Per ANSI standard, CPR row/column values are 1-indexed and relative to the viewport origin, so this is correct. Programs querying cursor position expect viewport-relative coordinates.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.
-->
