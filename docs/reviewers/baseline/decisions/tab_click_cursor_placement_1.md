---
decision: APPROVE
summary: All success criteria satisfied with a clean implementation that syncs viewport dimensions on tab activation, and comprehensive regression tests verify the fix.
operator_review: null
---

## Criteria Assessment

### Criterion 1: Clicking anywhere in the buffer of a non-first tab immediately moves the cursor to the clicked position with no scrolling required first.

- **Status**: satisfied
- **Evidence**: The `sync_active_tab_viewport()` helper method (editor_state.rs lines 270-289) is called in `new_tab()`, `switch_tab()`, and `associate_file()`. This ensures `visible_lines` is set correctly before any mouse click handling, so `dirty_lines_to_region` will compute a non-empty dirty region and the cursor repaint will occur.

### Criterion 2: The fix applies to tabs created by Cmd+T and to tabs opened via the file picker (Cmd+O).

- **Status**: satisfied
- **Evidence**:
  - Cmd+T flow: `new_tab()` calls `sync_active_tab_viewport()` at line 1499
  - File picker (Cmd+O) flow: `associate_file()` calls `sync_active_tab_viewport()` at line 1298

### Criterion 3: Switching away from a tab and back, then clicking, also places the cursor correctly (i.e., the fix is not limited to newly-created tabs).

- **Status**: satisfied
- **Evidence**: `switch_tab()` calls `sync_active_tab_viewport()` at line 1410 after `workspace.switch_tab(index)` completes. The `test_switch_tab_viewport_is_sized` test (lines 3968-4000) explicitly verifies this by switching tab 0 → tab 1 → tab 0 → tab 1 and asserting correct viewport sizing and non-None dirty regions throughout.

### Criterion 4: Clicking on the first tab continues to work correctly (no regression).

- **Status**: satisfied
- **Evidence**: The helper `sync_active_tab_viewport()` is additive—it syncs the viewport for any active tab, including the first tab. All 537 existing tests pass, including existing click/cursor positioning tests that exercise the first tab.

### Criterion 5: A regression test is added: create a second tab (without resizing), click at a specific position, and assert that the dirty region is non-empty and the cursor landed at the expected buffer position.

- **Status**: satisfied
- **Evidence**: Four regression tests added (lines 3903-4075):
  - `test_new_tab_viewport_is_sized`: Creates second tab, verifies `visible_lines = 10`, tests dirty region computation for line 2
  - `test_switch_tab_viewport_is_sized`: Switches between tabs, verifies viewport sizing and dirty regions
  - `test_associate_file_viewport_is_sized`: Tests file picker confirmation flow with temp file
  - `test_sync_viewport_skips_when_no_view_height`: Edge case for initial state before first resize

### Criterion 6: All existing viewport and click-positioning tests continue to pass.

- **Status**: satisfied
- **Evidence**: `cargo test -p lite-edit` shows all 537 tests pass with 0 failures. This includes all viewport_test.rs, wrap_test.rs, and editor_state tests.
