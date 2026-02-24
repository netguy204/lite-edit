---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/editor_event.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/event_channel.rs
  - crates/editor/src/workspace.rs
  - crates/terminal/src/terminal_buffer.rs
code_references:
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::process_pending_events
    implements: "Input-first event partitioning - processes priority events (user input, resize) before PTY wakeup events"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::process_single_event
    implements: "Single event processing extracted to enable partitioning"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_pty_wakeup
    implements: "Follow-up wakeup scheduling when terminals hit byte budget"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::is_priority_event
    implements: "Priority event classification for input-first partitioning"
  - ref: crates/terminal/src/terminal_buffer.rs#PollResult
    implements: "Poll result enum communicating whether more data is pending"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::DEFAULT_BYTES_PER_POLL
    implements: "Tunable byte budget constant (4KB default)"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::poll_events
    implements: "Byte-budgeted VTE processing - limits bytes per poll cycle"
  - ref: crates/editor/src/workspace.rs#Workspace::poll_standalone_terminals
    implements: "Propagates needs_rewakeup flag from terminal polling"
  - ref: crates/editor/src/editor_state.rs#EditorState::poll_agents
    implements: "Propagates needs_rewakeup flag to drain loop"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_pty_wakeup_followup
    implements: "Manual wakeup method that bypasses debouncing for budget overflow"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- dialog_pointer_cursor
- file_open_picker
- pane_cursor_click_offset
- pane_tabs_interaction
---

# Chunk Goal

## Minor Goal

Prevent rapid terminal output from starving user input processing in multi-pane layouts.

When the screen is split into an editor pane and multiple terminal panes (e.g., 4 vertical terminal splits running build processes), rapid terminal output makes the application unresponsive: clicks don't shift focus, typed characters appear with significant delay, and Cmd+Q doesn't immediately quit. The cursor continues to blink normally and CPU stays moderate (~20%), indicating this is not a compute saturation problem but an event processing priority/scheduling problem.

### Root Cause

The drain loop (`drain_loop.rs:126`) collects all pending events into a Vec and processes them sequentially. When terminals produce rapid output, `handle_pty_wakeup` calls `poll_agents()` (`editor_state.rs:2508`), which calls `poll_standalone_terminals()` on every workspace. Each terminal's `poll_events()` (`terminal_buffer.rs:274`) drains **all** accumulated PTY output and feeds it to `processor.advance()` synchronously — processing every buffered byte through the VTE state machine in a single call.

With 4 active terminals, a single `PtyWakeup` event triggers VTE processing across all 4 terminal buffers. The PTY reader thread uses a 4KB read buffer and can queue multiple chunks between drain cycles. Meanwhile, `poll_agents()` returns `DirtyRegion::FullViewport` for *any* terminal activity, triggering a complete screen redraw regardless of which panes actually changed.

The result: one `PtyWakeup` event → VTE processing of potentially tens of KB across 4 terminals → full viewport render → by the time this completes, more PTY data has accumulated and another wakeup fires. User input events (Key, Mouse) sit in the queue behind this cycle and are not reached in a timely manner. The cursor blink timer keeps firing independently because it's an NSTimer on the run loop, but the blink events themselves are also delayed in processing.

### Suggested Approach

Two complementary fixes that address the two layers of the problem (event ordering and unbounded work per event):

**A. Input-first event partitioning** — In `process_pending_events()` (`drain_loop.rs:126`), after draining the channel into a Vec, partition the events and process all user-input events (Key, Mouse, Scroll, Resize, FileDrop) *before* any PtyWakeup. This ensures input is never queued behind terminal output processing regardless of arrival order. CursorBlink can go in either partition since it's cosmetic.

**B. Time-budgeted VTE processing** — In `poll_events()` (`terminal_buffer.rs:274`), replace the unbounded `while let Some(event) = pty.try_recv()` drain loop with a byte-budgeted loop (e.g., 4KB per terminal per drain cycle). When the budget is exhausted, stop processing and leave remaining data in the channel. Return a flag indicating whether unprocessed data remains, and if so, ensure a follow-up PtyWakeup is scheduled so the data is eventually consumed on subsequent cycles.

Together: A eliminates the queuing delay (input events are never behind PTY events), and B bounds the wall-clock cost of a single drain cycle (VTE processing can't run away). Neither requires rearchitecting the drain loop — the drain-all-then-render pattern is preserved.

### What this chunk accomplishes

This chunk ensures user input events are processed with bounded latency even when terminals are flooding output, keeping the editor responsive under all realistic terminal workloads. This directly serves the project's core goal of being a responsive, native text editor.

## Success Criteria

- With 4 terminal panes running `yes` (or equivalent rapid-output command), clicking a different pane shifts focus within a perceptible timeframe (< 100ms)
- Typed characters appear within a perceptible timeframe (< 100ms) while terminals are flooding
- Cmd+Q quits promptly while terminals are flooding
- Normal terminal output (compilation, test runs) renders without visible delay or dropped content
- No terminal output bytes are lost — all PTY data is eventually processed and rendered correctly across subsequent drain cycles
- The byte budget per terminal is tunable (constant, not hard-coded in a loop condition) so it can be adjusted if needed