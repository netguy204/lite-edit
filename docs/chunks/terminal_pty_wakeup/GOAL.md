---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/Cargo.toml
  - crates/terminal/src/lib.rs
  - crates/terminal/src/pty.rs
  - crates/terminal/src/pty_wakeup.rs
  - crates/terminal/src/terminal_buffer.rs
  - crates/editor/src/main.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/drain_loop.rs
  - crates/terminal/tests/wakeup_integration.rs
code_references:
  - ref: crates/terminal/src/pty_wakeup.rs#PtyWakeup
    implements: "Run-loop wakeup handle with debouncing for cross-thread PTY signaling"
  - ref: crates/terminal/src/pty_wakeup.rs#PtyWakeup::signal
    implements: "Dispatches callback to main queue via GCD when PTY data arrives"
  - ref: crates/terminal/src/pty_wakeup.rs#set_global_wakeup_callback
    implements: "Global callback registration for PTY wakeup (legacy mechanism, superseded by WakeupSignal)"
  - ref: crates/terminal/src/pty.rs#PtyHandle::spawn_with_wakeup
    implements: "PTY spawn variant that signals wakeup on data arrival"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::spawn_shell_with_wakeup
    implements: "Shell spawn with wakeup support"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::spawn_command_with_wakeup
    implements: "Command spawn with wakeup support"
  - ref: crates/editor/src/editor_state.rs#EditorState::set_event_sender
    implements: "EventSender registration for creating PtyWakeup handles (refactored from set_pty_wakeup_factory)"
  - ref: crates/editor/src/editor_state.rs#EditorState::create_pty_wakeup
    implements: "Creates PtyWakeup handle from registered EventSender"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_pty_wakeup
    implements: "Handler that polls agents when PTY data arrives (moved from main.rs EditorController)"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- terminal_input_render_bug
---

# Chunk Goal

## Minor Goal

Eliminate up to 500ms input-to-display latency in terminal tabs by waking the
main thread when PTY data arrives, instead of waiting for the cursor blink timer.

### Root Cause Analysis

The current input echo path for terminal tabs:

1. **Key press** → `EditorController::handle_key()` → `EditorState::handle_key_buffer()` writes encoded bytes to PTY via `terminal.write_input()`
2. **Immediate poll** → `poll_agents()` calls `try_recv()` on crossbeam channel — **channel is empty** because the shell hasn't had time to echo the character yet
3. **Render** — renders without the echoed character
4. **Wait up to 500ms** — the only periodic polling is the cursor blink timer (`CURSOR_BLINK_INTERVAL = 0.5s` in `main.rs`)
5. **Next timer tick** → `toggle_cursor_blink()` calls `poll_agents()` → crossbeam channel now has the echoed output → renders

The PTY reader thread (`pty.rs`) reads output in a background thread and sends
`TerminalEvent::PtyOutput` via a crossbeam channel, but nothing wakes the
NSRunLoop when data arrives. The main thread is asleep between timer ticks.

### Solution Direction

Add a run-loop wakeup mechanism so the main thread renders within ~1ms of PTY
data arriving. The recommended approach:

**Option A (preferred): Post a custom NSEvent from the reader thread.** When the
PTY reader thread sends data to the crossbeam channel, also post a
`NSApplication::postEvent` (or use `CFRunLoopSourceSignal` + `CFRunLoopWakeUp`)
to wake the NSRunLoop. A `kCFRunLoopBeforeWaiting` observer or the event handler
then calls `poll_agents()` and renders.

**Option B: `dispatch_source` on PTY file descriptor.** Create a
`dispatch_source_create(DISPATCH_SOURCE_TYPE_READ, pty_fd, ...)` that fires on
the main queue when the PTY master has data available. This avoids the
crossbeam channel entirely for wakeup (though the channel can remain for data
transfer).

**Option C: Short poll timer.** Replace the 500ms blink timer with a ~2ms poll
timer. Simple but wasteful — burns CPU even when idle.

Key files:
- `crates/editor/src/main.rs` — NSRunLoop, timer setup, EditorController
- `crates/terminal/src/pty.rs` — PTY reader thread, crossbeam channel
- `crates/editor/src/editor_state.rs` — `poll_agents()`

## Success Criteria

- Typing a character in a terminal tab and seeing it appear takes < 5ms end-to-end (measured from `write_input()` to render completion)
- No increase in idle CPU usage (must not busy-poll)
- Cursor blink continues to work at its current 500ms interval
- All existing terminal integration tests pass