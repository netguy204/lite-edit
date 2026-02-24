<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk addresses terminal output flooding starving user input with two complementary fixes:

**A. Input-first event partitioning** — Modify `process_pending_events()` in `drain_loop.rs` to process user-input events (Key, Mouse, Scroll, Resize, FileDrop) *before* any PtyWakeup events. This is a simple reordering that ensures input latency is bounded by the cost of processing input events, not by accumulated PTY work.

**B. Byte-budgeted VTE processing** — Modify `poll_events()` in `terminal_buffer.rs` to stop after processing a configurable byte budget (e.g., 4KB) per terminal per drain cycle. When the budget is exhausted, the method returns a flag indicating unprocessed data remains, triggering a follow-up wakeup. This bounds the wall-clock cost of a single drain cycle while ensuring all data is eventually processed.

Both fixes preserve the existing drain-all-then-render pattern. Neither requires changes to the event channel architecture or the single-threaded ownership model.

The approach follows the Humble View architecture from `docs/trunk/TESTING_PHILOSOPHY.md`: the prioritization and budgeting logic is pure, testable state manipulation. The tests can verify event ordering and budget behavior without platform dependencies.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem indirectly via `poll_standalone_terminals()` which calls `scroll_to_bottom()` and `is_at_bottom()`. No changes to viewport behavior are needed—the subsystem's existing methods work correctly. We just need to ensure that even with budgeted processing, the auto-follow behavior triggers correctly.

No subsystem deviations discovered.

## Sequence

### Step 1: Add `is_user_input_event()` helper to EditorEvent

The `EditorEvent` enum already has an `is_user_input()` method. Verify it includes all the event types we want to prioritize (Key, Mouse, Scroll, Resize, FileDrop). Add Resize if it's not already included, since window resize should be responsive.

Location: `crates/editor/src/editor_event.rs`

### Step 2: Implement input-first event partitioning in drain loop

Modify `process_pending_events()` to partition events after draining from the channel:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Input-first event partitioning
pub fn process_pending_events(&mut self) {
    let mut had_pty_wakeup = false;

    // Drain all events into a Vec
    let events: Vec<EditorEvent> = self.receiver.drain().collect();

    // Partition: process user-input events first, then PTY/timer events
    // This ensures input latency is never gated by accumulated terminal output
    let (input_events, other_events): (Vec<_>, Vec<_>) = events
        .into_iter()
        .partition(|e| e.is_user_input() || matches!(e, EditorEvent::Resize));

    // Process input events first
    for event in input_events {
        // ... existing match arms
    }

    // Then process other events (PtyWakeup, CursorBlink)
    for event in other_events {
        // ... existing match arms
    }

    // ... rest unchanged
}
```

This is a behavioral change but preserves all existing functionality. The drain-all-then-render-once pattern is preserved.

Location: `crates/editor/src/drain_loop.rs`

### Step 3: Add byte budget constant to TerminalBuffer

Add a tunable constant for the maximum bytes to process per poll cycle:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Byte-budgeted VTE processing
impl TerminalBuffer {
    /// Maximum bytes to process per `poll_events()` call.
    /// When this budget is exhausted, remaining data stays in the channel
    /// and will be processed on the next wakeup cycle.
    pub const DEFAULT_BYTES_PER_POLL: usize = 4 * 1024; // 4KB
}
```

Also add an instance field to allow per-terminal configuration if needed in the future.

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 4: Modify poll_events() to use byte budget

Change the unbounded `while let Some(event) = pty.try_recv()` loop to track bytes processed and stop when the budget is exhausted:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Byte-budgeted VTE processing
pub fn poll_events(&mut self) -> PollResult {
    let Some(ref pty) = self.pty else {
        return PollResult::Idle;
    };

    let mut bytes_processed: usize = 0;
    let mut processed_any = false;

    // Drain events up to the byte budget
    while let Some(event) = pty.try_recv() {
        match event {
            TerminalEvent::PtyOutput(data) => {
                bytes_processed += data.len();
                self.processor.advance(&mut self.term, &data);
                processed_any = true;

                // Check budget after processing (we always process at least one chunk)
                if bytes_processed >= Self::DEFAULT_BYTES_PER_POLL {
                    break;
                }
            }
            TerminalEvent::PtyExited(_code) => {
                processed_any = true;
            }
            TerminalEvent::PtyError(_) => {
                processed_any = true;
            }
        }
    }

    // ... existing event_rx handling (DSR responses)

    if processed_any {
        self.clear_selection();
        self.update_damage();
        self.check_scrollback_overflow();
    }

    // Return whether more data may be pending
    if bytes_processed >= Self::DEFAULT_BYTES_PER_POLL {
        PollResult::MorePending
    } else if processed_any {
        PollResult::Processed
    } else {
        PollResult::Idle
    }
}
```

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 5: Define PollResult enum

Create a return type that communicates both whether work was done and whether more work remains:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Byte-budgeted VTE processing
/// Result of a `poll_events()` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollResult {
    /// No events were available.
    Idle,
    /// Events were processed and the channel is now empty.
    Processed,
    /// Events were processed but more data may remain (budget exhausted).
    /// Caller should schedule a follow-up wakeup.
    MorePending,
}
```

Location: `crates/terminal/src/terminal_buffer.rs` (or a new `poll_result.rs` if preferred)

### Step 6: Update poll_standalone_terminals() to handle PollResult

Modify `Workspace::poll_standalone_terminals()` to track whether any terminal has more pending data:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Schedule follow-up wakeup when budget exhausted
pub fn poll_standalone_terminals(&mut self) -> (bool, bool) {
    // Returns (had_events, needs_rewakeup)
    let mut had_events = false;
    let mut needs_rewakeup = false;

    for pane in self.pane_root.all_panes_mut() {
        for tab in &mut pane.tabs {
            if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
                let was_at_bottom = viewport.is_at_bottom(terminal.line_count());
                let was_alt_screen = terminal.is_alt_screen();

                let result = terminal.poll_events();

                match result {
                    PollResult::Processed | PollResult::MorePending => {
                        had_events = true;
                        if matches!(result, PollResult::MorePending) {
                            needs_rewakeup = true;
                        }
                        // ... existing auto-follow logic
                    }
                    PollResult::Idle => {}
                }
            }
        }
    }

    (had_events, needs_rewakeup)
}
```

Location: `crates/editor/src/workspace.rs`

### Step 7: Update poll_agents() to propagate needs_rewakeup

Modify `EditorState::poll_agents()` to return whether a follow-up wakeup is needed:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Propagate needs_rewakeup
pub fn poll_agents(&mut self) -> (DirtyRegion, bool) {
    // Returns (dirty_region, needs_rewakeup)
    let mut any_activity = false;
    let mut any_needs_rewakeup = false;

    for workspace in &mut self.editor.workspaces {
        if workspace.poll_agent() {
            any_activity = true;
        }
        let (had_events, needs_rewakeup) = workspace.poll_standalone_terminals();
        if had_events {
            any_activity = true;
        }
        if needs_rewakeup {
            any_needs_rewakeup = true;
        }
    }

    let dirty = if any_activity {
        DirtyRegion::FullViewport
    } else {
        DirtyRegion::None
    };

    (dirty, any_needs_rewakeup)
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 8: Schedule follow-up wakeup in drain loop

When `poll_agents()` indicates more data is pending, send a `PtyWakeup` event to ensure the next drain cycle processes more data:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Follow-up wakeup scheduling
fn handle_pty_wakeup(&mut self) {
    let (terminal_dirty, needs_rewakeup) = self.state.poll_agents();
    if terminal_dirty.is_dirty() {
        self.state.dirty_region.merge(terminal_dirty);
    }

    // If any terminal hit its byte budget, schedule a follow-up wakeup
    // so remaining data gets processed on the next cycle
    if needs_rewakeup {
        self.sender.send_pty_wakeup();
    }
}
```

This uses the existing `EventSender` to queue another `PtyWakeup`. Since the wakeup coalescing logic (`wakeup_pending` flag) already exists, multiple rapid wakeups won't flood the queue.

Location: `crates/editor/src/drain_loop.rs`

### Step 9: Add send_pty_wakeup method to EventSender

Add a method to manually send a `PtyWakeup` event for follow-up scheduling:

```rust
// Chunk: docs/chunks/terminal_flood_starvation - Manual wakeup for budget overflow
impl EventSender {
    /// Sends a PtyWakeup event to process remaining terminal data.
    /// Used when a terminal hits its byte budget and has more data pending.
    pub fn send_pty_wakeup(&self) {
        let _ = self.tx.send(EditorEvent::PtyWakeup);
    }
}
```

Location: `crates/editor/src/event_channel.rs`

### Step 10: Write unit tests for event partitioning

Test that input events are processed before PTY events regardless of arrival order:

```rust
#[test]
fn test_input_events_processed_before_pty_wakeup() {
    // Create events in order: PtyWakeup, Key, PtyWakeup, Mouse
    // Verify processing order: Key, Mouse, PtyWakeup, PtyWakeup
}

#[test]
fn test_resize_processed_before_pty_wakeup() {
    // Resize is not "user input" but should still be prioritized
}
```

Location: `crates/editor/src/drain_loop.rs` (test module)

### Step 11: Write unit tests for byte-budgeted polling

Test that `poll_events()` respects the byte budget:

```rust
#[test]
fn test_poll_events_respects_byte_budget() {
    // Queue 10KB of data
    // First poll_events() returns MorePending after ~4KB
    // Second poll_events() returns MorePending after ~4KB
    // Third poll_events() returns Processed after ~2KB
}

#[test]
fn test_poll_events_processes_at_least_one_chunk() {
    // Even a single 8KB chunk is processed in one call
    // (we always process at least one event)
}
```

Location: `crates/terminal/src/terminal_buffer.rs` (test module)

### Step 12: Write integration test for responsiveness under load

Test the full drain loop behavior with simulated flooding:

```rust
#[test]
fn test_input_responsive_during_pty_flood() {
    // Start terminal with rapid output (e.g., yes command)
    // Send key events during output
    // Verify key events are processed promptly (within bounded time)
}
```

Location: `crates/terminal/tests/flood_integration.rs` (new file)

### Step 13: Update code_paths in GOAL.md

Add the files touched by this implementation:
- `crates/editor/src/drain_loop.rs`
- `crates/editor/src/editor_event.rs`
- `crates/editor/src/editor_state.rs`
- `crates/editor/src/event_channel.rs`
- `crates/editor/src/workspace.rs`
- `crates/terminal/src/terminal_buffer.rs`

Location: `docs/chunks/terminal_flood_starvation/GOAL.md`

---

**BACKREFERENCE COMMENTS**

Add the following backreferences to modified code:

- `drain_loop.rs#process_pending_events`: `// Chunk: docs/chunks/terminal_flood_starvation - Input-first event partitioning`
- `terminal_buffer.rs#poll_events`: `// Chunk: docs/chunks/terminal_flood_starvation - Byte-budgeted VTE processing`
- `terminal_buffer.rs#PollResult`: `// Chunk: docs/chunks/terminal_flood_starvation - Byte-budgeted VTE processing`
- `workspace.rs#poll_standalone_terminals`: `// Chunk: docs/chunks/terminal_flood_starvation - Needs rewakeup propagation`
- `event_channel.rs#EventSender::send_pty_wakeup`: `// Chunk: docs/chunks/terminal_flood_starvation - Manual wakeup for budget overflow`

## Dependencies

No external dependencies. This chunk builds entirely on existing infrastructure:
- The unified event channel (`EditorEvent`, `EventSender`, `EventReceiver`)
- The drain loop pattern (`process_pending_events`)
- The terminal buffer polling (`poll_events`, `poll_standalone_terminals`)
- The PTY wakeup coalescing (`wakeup_pending` flag)

## Risks and Open Questions

1. **Byte budget tuning**: The 4KB default is a reasonable starting point (matches the PTY read buffer size), but may need adjustment based on real-world testing. Too small = excessive wakeups and render churn. Too large = input latency spikes. The success criteria specify "within a perceptible timeframe (< 100ms)" which should be achievable with 4KB at reasonable VTE processing speeds.

2. **Follow-up wakeup latency**: When we self-send a `PtyWakeup` event, it goes to the back of the event queue. With input-first partitioning, any pending input events will be processed before the follow-up. This is correct behavior (input takes priority) but means high-throughput terminal output may visually lag behind what's been received. This is acceptable—the success criteria state "normal terminal output renders without visible delay", not "instant."

3. **Agent terminal polling**: The `poll_agent()` method (for workspace agents) is separate from `poll_standalone_terminals()`. The byte budget only applies to standalone terminals. If agent terminals experience similar flooding, they would need the same treatment. However, agents typically don't produce unbounded output like `yes` or continuous builds, so this may not be needed.

4. **Wakeup coalescing interaction**: The existing `wakeup_pending` flag prevents redundant wakeups from the PTY reader thread. Our self-sent wakeup uses `EventSender::send_pty_wakeup()` which bypasses this flag. This is intentional—we want to guarantee a follow-up cycle when budget is exhausted, even if the reader thread has already signaled. But we should verify no infinite loop is possible (it shouldn't be, since we only send a follow-up when budget is exhausted AND more data exists).

5. **DirtyRegion granularity**: Currently `poll_agents()` returns `FullViewport` if *any* terminal had activity. A future optimization could track which panes actually changed and return more granular dirty regions. This is out of scope for this chunk but would compound well with the budgeting—smaller dirty regions mean faster renders per cycle.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->