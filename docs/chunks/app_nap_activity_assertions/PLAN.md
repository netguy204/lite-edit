<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Use macOS's `NSProcessInfo` activity assertion API to explicitly communicate
terminal activity state to the operating system. The API works like this:

1. Call `beginActivityWithOptions:reason:` to acquire an activity token
2. Hold the token as long as the activity is ongoing
3. Call `endActivity:` to release the token when activity ceases

For terminal activity, we'll track "recent PTY output" using a timestamp.
When any terminal receives PTY data (via `poll_events()`), we update the
timestamp and ensure an activity assertion is held. A 2-second timeout
provides hysteresis - if no terminal has received output for 2 seconds,
we release the assertion.

**Strategy: Centralized Activity Manager**

Create an `ActivityAssertion` module that:
- Wraps `NSProcessInfo.beginActivityWithOptions:reason:` and `endActivity:`
- Holds the current assertion token (if any)
- Exposes `hold()` and `release()` methods

The `EditorState` will track `last_terminal_activity: Option<Instant>` and
manage assertion state:
- On `poll_agents()` returning activity: update timestamp, call `hold()`
- On `toggle_cursor_blink()` (every 0.5s): check if 2s has elapsed since
  last activity; if so and window is not key, call `release()`

This builds on the existing App Nap work from `app_nap_blink_timer` which
stops the blink timer when backgrounded. Together, these two changes should
allow macOS to fully nap the process when idle.

**NSActivityOptions**: Use `NSActivityOptions::UserInitiated` which:
- Prevents App Nap
- Allows display and system idle sleep (we're not a video player)
- Indicates work responding to user interaction

The `objc2-foundation` crate already provides `NSProcessInfo` and
`NSActivityOptions` bindings (the crate is already a dependency).

## Subsystem Considerations

No relevant subsystems. The existing subsystems (`renderer`, `viewport_scroll`)
are not related to process activity management.

## Sequence

### Step 1: Enable the NSProcessInfo feature in objc2-foundation

Update `crates/editor/Cargo.toml` to enable the `NSProcessInfo` feature on the
`objc2-foundation` dependency. This gives us access to `NSProcessInfo`,
`NSActivityOptions`, and the `beginActivityWithOptions_reason` method.

Location: `crates/editor/Cargo.toml`

### Step 2: Create the activity assertion module

Create a new module `crates/editor/src/activity_assertion.rs` that wraps the
NSProcessInfo activity assertion API:

```rust
// Chunk: docs/chunks/app_nap_activity_assertions - Activity assertion wrapper
//! Activity assertion management for App Nap prevention.
//!
//! This module wraps macOS's NSProcessInfo activity assertion API to
//! communicate terminal activity state to the operating system.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSActivityOptions, NSObjectProtocol, NSProcessInfo,
};

/// Manages a single NSProcessInfo activity assertion.
///
/// When held, the assertion prevents App Nap for latency-sensitive work.
/// Call `hold()` when terminal activity begins and `release()` when idle.
pub struct ActivityAssertion {
    /// The current activity token, if held.
    token: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}
```

Implement:
- `new()` - Creates without holding an assertion
- `hold(&mut self, mtm: MainThreadMarker)` - Begins an assertion if not held
- `release(&mut self)` - Ends the assertion if held
- `is_held(&self) -> bool` - Returns whether assertion is currently held

Use `NSActivityOptions::UserInitiated` and reason string "Terminal activity".

Location: `crates/editor/src/activity_assertion.rs`

### Step 3: Add the module to main.rs

Add `mod activity_assertion;` to `crates/editor/src/main.rs` to include the
new module in the build.

Location: `crates/editor/src/main.rs`

### Step 4: Add activity tracking fields to EditorState

Add to `EditorState`:
- `last_terminal_activity: Option<Instant>` - Timestamp of last PTY output
- `activity_assertion: ActivityAssertion` - The assertion manager

Initialize `last_terminal_activity = None` and create the assertion in the
constructor.

Location: `crates/editor/src/editor_state.rs`

### Step 5: Update poll_agents to track terminal activity

Modify `EditorState::poll_agents()` to update `last_terminal_activity` when
any terminal has activity. When `any_activity` is true:
1. Set `last_terminal_activity = Some(Instant::now())`
2. Call `activity_assertion.hold()` to ensure assertion is active

This ensures the assertion is held whenever terminals are actively receiving
PTY output.

Location: `crates/editor/src/editor_state.rs` (poll_agents method, ~line 3059)

### Step 6: Update toggle_cursor_blink to check activity timeout

Modify `EditorState::toggle_cursor_blink()` to check if terminals have been
quiescent and release the assertion when appropriate.

Add logic at the start of the method:
```rust
// Check for terminal quiescence (no activity for 2 seconds)
const ACTIVITY_TIMEOUT_MS: u64 = 2000;
if let Some(last_activity) = self.last_terminal_activity {
    let elapsed = Instant::now().duration_since(last_activity);
    if elapsed.as_millis() >= ACTIVITY_TIMEOUT_MS as u128 {
        // Terminals have been idle for 2 seconds
        // Release the activity assertion to allow App Nap
        self.activity_assertion.release();
        self.last_terminal_activity = None;
    }
}
```

The blink timer fires every 0.5 seconds, so this provides timely release.
Combined with the blink timer stopping when backgrounded (from
`app_nap_blink_timer`), this allows App Nap when:
- No recent terminal activity AND
- Window is not key (blink timer stopped)

Location: `crates/editor/src/editor_state.rs` (toggle_cursor_blink method)

### Step 7: Release assertion on window resign key

Add a method `EditorState::release_activity_assertion()` that can be called
from the drain loop when the window loses key status. This ensures the
assertion is released immediately when backgrounding, rather than waiting
for the 2-second timeout.

Also add `EditorEvent::WindowResignKey` to the event enum and handle it in
the drain loop to call `release_activity_assertion()`.

Location:
- `crates/editor/src/editor_event.rs` (add WindowResignKey variant)
- `crates/editor/src/editor_state.rs` (add release_activity_assertion method)
- `crates/editor/src/drain_loop.rs` (handle WindowResignKey event)
- `crates/editor/src/main.rs` (send event from windowDidResignKey delegate)
- `crates/editor/src/event_channel.rs` (add send_window_resign_key method)

### Step 8: Add tests for activity assertion module

Create tests for the `ActivityAssertion` module:
- Test `new()` creates with no assertion held
- Test `hold()` acquires assertion (is_held returns true)
- Test `release()` releases assertion (is_held returns false)
- Test double `hold()` is idempotent (doesn't create multiple assertions)
- Test double `release()` is safe (no panic or error)

Note: These tests verify API behavior, not App Nap itself. Verifying App Nap
requires Activity Monitor inspection which is documented in manual testing
notes.

Location: `crates/editor/src/activity_assertion.rs` (tests module)

### Step 9: Verify no regressions in existing tests

Run the full test suite to ensure no regressions:
```
cargo test --workspace
```

Location: N/A (test execution)

### Step 10: Manual verification with Activity Monitor

Manual testing procedure (for verification, not automated):
1. Launch lite-edit
2. Open Activity Monitor, enable "App Nap" column
3. Create a terminal tab, run `cat` (waits for input, produces no output)
4. Switch to another app (background lite-edit)
5. Verify "App Nap: Yes" appears in Activity Monitor after ~2-3 seconds
6. Switch back to lite-edit, run `ls -la /` in terminal
7. Verify "App Nap: No" while output is active
8. Wait 3 seconds, background again
9. Verify "App Nap: Yes" returns

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments to trace back to this chunk:

```rust
// Chunk: docs/chunks/app_nap_activity_assertions - Activity assertion for App Nap
```

Place at module level in `activity_assertion.rs` and on modified methods in
`editor_state.rs`.

## Dependencies

- **Chunk**: `app_nap_blink_timer` (ACTIVE) - This chunk builds on the blink
  timer work that stops the timer when backgrounded. Without that, the timer
  would keep waking the process even with an activity assertion released.

- **Library**: `objc2-foundation` with `NSProcessInfo` feature - Already a
  dependency, just need to enable the feature flag.

## Risks and Open Questions

- **objc2-foundation feature availability**: The `NSProcessInfo` feature flag
  may have additional required features (e.g., `NSString`). If compilation
  fails, check what additional features are needed.

- **Activity assertion thread safety**: The `NSProcessInfo` API must be called
  from the main thread. The drain loop runs on the main thread, so this should
  be fine, but we should verify `MainThreadMarker` requirements.

- **Multiple assertions**: macOS allows multiple concurrent activity assertions.
  Our design holds at most one, which is sufficient for terminal activity.
  If future features need different assertion types, we may need to expand.

- **Activity assertion cost**: Creating/releasing assertions has some overhead.
  If terminals produce very bursty output (acquire/release rapidly), there could
  be performance impact. The 2-second timeout provides hysteresis to mitigate
  this, but monitor for issues.

- **Testing limitations**: App Nap behavior cannot be unit tested - it requires
  manual verification with Activity Monitor. Document the manual test procedure
  clearly in Step 10.

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

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->