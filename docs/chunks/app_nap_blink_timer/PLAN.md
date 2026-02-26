<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current cursor blink timer is a perpetual 0.5-second `NSTimer` created once
at startup (`setup_cursor_blink_timer` in `main.rs`). It fires unconditionally,
preventing macOS from napping the process when backgrounded.

We will:

1. **Add `windowDidResignKey:` and `windowDidBecomeKey:` delegate methods** to
   the existing `NSWindowDelegate` implementation on `AppDelegate`. These fire
   when the window loses/gains key status (i.e., becomes inactive/active).

2. **Invalidate the timer on resign key** and set the `blink_timer` ivar to
   `None`, removing all 0.5-second wakeups. This is the single most impactful
   change for App Nap eligibility.

3. **Recreate the timer on become key**, storing it back in the ivar, and reset
   the cursor to visible so the user sees a solid cursor when returning to the
   app.

4. **Add `setTolerance:` to the timer** with a 0.1s tolerance. This allows macOS
   to coalesce the timer with other system timers, reducing wakeups even when
   the app is in the foreground.

The implementation builds on:
- `AppDelegate` (`main.rs`) which already implements `NSWindowDelegate` for
  `windowDidResize:` and `windowDidChangeBackingProperties:`.
- `AppDelegateIvars` which already stores `blink_timer: RefCell<Option<Retained<NSTimer>>>`.
- `setup_cursor_blink_timer()` which creates the timer and schedules it.

**Testing strategy** (per TESTING_PHILOSOPHY.md):
- The timer setup and teardown logic is platform code (humble view), so we
  verify the high-level behavior manually via Activity Monitor's "App Nap"
  column.
- Existing cursor blink tests (`test_toggle_cursor_blink`, etc.) run in
  `EditorState` without timers and remain unchanged.
- We add a unit test verifying that `cursor_visible` is reset to `true` when
  focus is regained (if we add state-level hooks).

## Sequence

### Step 1: Add windowDidResignKey: to AppDelegate

Add a new method to the `unsafe impl NSWindowDelegate for AppDelegate` block:

```rust
#[unsafe(method(windowDidResignKey:))]
fn window_did_resign_key(&self, _notification: &NSNotification) {
    // Invalidate and clear the blink timer
    let mut timer_slot = self.ivars().blink_timer.borrow_mut();
    if let Some(timer) = timer_slot.take() {
        timer.invalidate();
    }
}
```

This method:
- Borrows the `blink_timer` ivar mutably
- Takes the timer out (replacing with `None`)
- Calls `invalidate()` to remove it from the run loop

**Location**: `crates/editor/src/main.rs` in the `unsafe impl NSWindowDelegate` block.

### Step 2: Add windowDidBecomeKey: to AppDelegate

Add another method to the `NSWindowDelegate` impl:

```rust
#[unsafe(method(windowDidBecomeKey:))]
fn window_did_become_key(&self, _notification: &NSNotification) {
    let mtm = MainThreadMarker::from(self);

    // Get the event sender to recreate the timer
    let sender = self.ivars().event_sender.borrow();
    if let Some(sender) = sender.as_ref() {
        // Recreate and store the blink timer
        let new_timer = self.setup_cursor_blink_timer(mtm, sender.clone());
        *self.ivars().blink_timer.borrow_mut() = Some(new_timer);
    }

    // Reset cursor to visible by sending a synthetic cursor blink event
    // The drain loop will see this and ensure cursor_visible = true
    if let Some(sender) = sender.as_ref() {
        let _ = sender.send_cursor_blink();
    }
}
```

This method:
- Gets a `MainThreadMarker` (required for `setup_cursor_blink_timer`)
- Clones the `EventSender` from ivars
- Creates a new timer and stores it
- Sends a cursor blink event so the cursor shows immediately

**Location**: `crates/editor/src/main.rs` in the `unsafe impl NSWindowDelegate` block.

### Step 3: Add timer tolerance in setup_cursor_blink_timer

After creating the timer with `scheduledTimerWithTimeInterval_repeats_block`,
call `setTolerance:` to allow timer coalescing:

```rust
// Allow 0.1s tolerance for timer coalescing (reduces wakeups even when active)
timer.setTolerance(0.1);
```

**Location**: `crates/editor/src/main.rs` in `setup_cursor_blink_timer()`, after
the `unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(...) }` block.

### Step 4: Reset cursor_visible on focus regain

The current `toggle_cursor_blink` already handles the "recent keystroke" case
which keeps the cursor solid. However, when the window regains focus, we want
to immediately show a solid cursor regardless of keystroke timing.

Option A: Send a synthetic event that resets `cursor_visible = true` in
`EditorState`. This could be a new event type `WindowFocusGained` or we can
rely on the cursor blink event handler checking a flag.

Option B: Add a `reset_cursor_visible()` method to `EditorState` that the drain
loop calls when it receives a focus-gained signal.

**Chosen approach**: The simplest approach is to not require state changes. When
`windowDidBecomeKey:` fires, the timer is recreated and starts sending blink
events again. The existing `toggle_cursor_blink` logic will toggle the cursor.
To ensure the cursor starts visible, we can have `windowDidBecomeKey:` send a
cursor blink event immediately. On the first toggle after regaining focus, if
the cursor was invisible, it becomes visible. If it was visible, it becomes
invisible but will toggle back on the next tick.

For a cleaner UX (always start with visible cursor), we could:
- Send a "focus gained" event that sets `cursor_visible = true` in `EditorState`

Let's do the simpler version first: the immediate cursor blink event in Step 2
will trigger a toggle. If UX testing shows glitches, we can add a dedicated
reset mechanism.

### Step 5: Verify existing cursor blink tests pass

Run `cargo test` in the `crates/editor` directory to verify no regressions:

```bash
cargo test --package editor -- cursor_blink
```

The existing tests (`test_toggle_cursor_blink`, `test_buffer_focus_blink_toggles_cursor_visible`,
etc.) test the state-level logic which is unaffected by timer lifecycle changes.

### Step 6: Manual App Nap verification

1. Build and run lite-edit: `cargo run --release -p editor`
2. Open Activity Monitor, enable the "App Nap" column
3. Focus another application (lite-edit loses key window status)
4. Wait ~30 seconds for App Nap to engage
5. Verify "App Nap" shows "Yes" for lite-edit
6. Click lite-edit to regain focus
7. Verify cursor blink resumes normally
8. Verify no visual glitches (cursor should be visible when window regains focus)

## Risks and Open Questions

- **Timer recreation overhead**: Creating a new `NSTimer` on each focus change
  is negligible compared to the ~0.5s interval. The timer runs on the main
  run loop which is already active for event processing.

- **Race condition on shutdown**: If the app is terminating while
  `windowDidBecomeKey:` runs, the `event_sender` might be in an inconsistent
  state. The existing code already uses `Option` and ignores send errors, so
  this should be safe.

- **Cursor visible state on regain**: The current approach sends a blink event
  which toggles the cursor. If the cursor was invisible when focus was lost and
  the first toggle makes it visible, that's correct. If it was visible, the
  first toggle makes it invisible, then the next tick makes it visible again.
  This might cause a single invisible frame. Acceptable for now; can add a
  dedicated reset if needed.

- **Multiple windows**: lite-edit currently has a single window. If multiple
  windows are added in the future, the timer management would need per-window
  tracking. This is out of scope for this chunk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->