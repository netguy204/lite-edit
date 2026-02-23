<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a targeted bug fix that adds missing `run_loop_waker` calls to non-PTY event senders in `EventSender`. The fix follows the existing pattern established by `send_pty_wakeup()`: after enqueueing an event to the mpsc channel, call `(self.inner.run_loop_waker)()` to signal the CFRunLoopSource so the drain loop processes the event.

**Key insight**: The parent chunk `pty_wakeup_reentrant` correctly identified that PTY wakeup needs the waker (since it arrives from a background thread), but the main-thread events (key, mouse, scroll, cursor blink, resize) were left without waker calls under the assumption that "the run loop is already awake." This assumption is flawed — while the run loop *might* be processing a timer or other source, the CFRunLoopSource that drains our event queue only fires when it is explicitly signaled. Without the signal, events accumulate but are never processed until the next incidental wakeup (e.g., next PTY output).

**Fix strategy**: Add `(self.inner.run_loop_waker)()` calls to all non-PTY send methods. This is safe to call redundantly — multiple signals collapse into a single callback invocation when the run loop drains its sources.

**Testing strategy**: Following `docs/trunk/TESTING_PHILOSOPHY.md`, we'll add unit tests that verify the waker callback is invoked for each event type. The existing `test_send_key_event` test provides a pattern but doesn't actually check whether the waker was called — it only verifies the event was received. We'll add explicit waker invocation assertions.

## Sequence

### Step 1: Write failing tests for waker calls

Add unit tests to `crates/editor/src/event_channel.rs` that verify the `run_loop_waker` callback is invoked when sending each event type:
- `test_send_key_calls_waker`
- `test_send_mouse_calls_waker`
- `test_send_scroll_calls_waker`
- `test_send_cursor_blink_calls_waker`
- `test_send_resize_calls_waker`

Each test creates a channel with a waker callback that increments an `AtomicUsize`, sends one event of the relevant type, and asserts `waker_called.load() == 1`.

Location: `crates/editor/src/event_channel.rs` (in the existing `#[cfg(test)] mod tests` block)

### Step 2: Add waker calls to send methods

Modify each send method in `EventSender` to call the waker after sending:

```rust
pub fn send_key(&self, event: KeyEvent) -> Result<(), SendError<EditorEvent>> {
    let result = self.inner.sender.send(EditorEvent::Key(event));
    (self.inner.run_loop_waker)();
    result
}
```

Repeat for:
- `send_key`
- `send_mouse`
- `send_scroll`
- `send_cursor_blink`
- `send_resize`

Note: `send_pty_wakeup` already calls the waker, so no change is needed there.

Location: `crates/editor/src/event_channel.rs`

### Step 3: Verify tests pass

Run the test suite to confirm:
1. The new waker tests pass
2. Existing tests still pass
3. No regressions in the broader editor test suite

Commands:
```bash
cargo test -p lite-edit --lib event_channel
cargo test -p lite-edit
```

### Step 4: Manual verification (optional but recommended)

Build and run the editor to verify:
- Hotkeys (Cmd+P, Cmd+S, etc.) respond immediately
- Mouse clicks register
- Scroll events work
- Cursor blink visibly toggles
- Window resize triggers relayout

This manual step confirms end-to-end behavior that unit tests cannot fully capture.

## Risks and Open Questions

1. **Redundant waker calls from main thread**: Events like key/mouse/scroll originate from NSView callbacks, which fire when the run loop is already awake. Calling the waker is harmless (CFRunLoopSourceSignal + CFRunLoopWakeUp are cheap and collapse), but it adds minor overhead. This is the conservative choice — correctness over micro-optimization. If profiling shows this is a bottleneck, a future chunk could add a flag to skip the waker when already on the main thread.

2. **Ordering with debounced PTY wakeup**: `send_pty_wakeup` has debouncing logic (the `wakeup_pending` flag). The other send methods do not need this because they're not called from a tight background loop. Each keypress or mouse event is discrete and should be processed.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->