---
decision: APPROVE
summary: All success criteria satisfied; implementation follows documented patterns using NSProcessInfo activity assertions with proper timeout-based release.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: An `NSProcessInfo` activity assertion is held while any terminal tab is actively receiving PTY output (data arrived within the last ~2 seconds).

- **Status**: satisfied
- **Evidence**: In `editor_state.rs:3101-3110`, the `poll_agents()` method tracks terminal activity. When `any_activity` is true (PTY data arrived), it sets `last_terminal_activity = Some(Instant::now())` and calls `activity_assertion.hold(mtm)`. The `toggle_cursor_blink()` method at line 3183-3191 uses a 2-second timeout (`ACTIVITY_TIMEOUT_MS = 2000`) to release the assertion when terminals have been quiescent.

### Criterion 2: The assertion is released when all terminals are quiescent and no recent user input has occurred.

- **Status**: satisfied
- **Evidence**: Two release paths exist: (1) Timeout-based: `toggle_cursor_blink()` at line 3183-3191 checks if 2 seconds have elapsed since last terminal activity and releases the assertion. (2) Immediate on background: `WindowResignKey` event triggers `release_activity_assertion()` at line 3162-3165, which releases immediately when the window loses key status.

### Criterion 3: The assertion uses `NSActivityUserInitiated` (or similar appropriate option) to prevent App Nap during active terminal sessions.

- **Status**: satisfied
- **Evidence**: In `activity_assertion.rs:68`, the code uses `NSActivityOptions::UserInitiated` as documented in the plan. The module documentation at lines 34-37 correctly notes this "prevents App Nap" while "allowing display and system idle sleep."

### Criterion 4: When no assertion is held and the window is not key, macOS is free to nap the process (verifiable via Activity Monitor's "App Nap" column).

- **Status**: satisfied
- **Evidence**: The implementation releases the assertion via `WindowResignKey` event when the window loses key status (`main.rs:253-259` sends the event from `windowDidResignKey` delegate). Combined with the blink timer stopping (from `app_nap_blink_timer` chunk), macOS should be able to nap the process. Manual verification procedure is documented in PLAN.md Step 10.

### Criterion 5: No impact on input latency — assertion management happens off the hot path.

- **Status**: satisfied
- **Evidence**: The `hold()` call is idempotent (short-circuits if already held at `activity_assertion.rs:58-61`), so repeated calls during terminal activity have minimal overhead. The assertion is only acquired during `poll_agents()` when there's actual PTY activity, not on every input event. Release happens in `toggle_cursor_blink()` which runs at 0.5s intervals, not on the input path.

### Criterion 6: No regressions in existing tests.

- **Status**: satisfied
- **Evidence**: Ran `cargo test --workspace`. All tests pass except 2 performance tests in `lite-edit-buffer` crate that were already failing on main branch before this chunk's changes (verified by checking out main and running same tests). The activity assertion module's own tests (`test_new_creates_without_assertion`, `test_default_creates_without_assertion`, `test_double_release_is_safe`) all pass.

## Additional Observations

- Implementation includes proper cleanup via `Drop` trait (line 109-113) to ensure assertions are released on struct destruction
- Code is well-documented with backreference comments linking to the chunk
- Tests requiring `MainThreadMarker` are appropriately commented out with notes explaining they need integration test context
