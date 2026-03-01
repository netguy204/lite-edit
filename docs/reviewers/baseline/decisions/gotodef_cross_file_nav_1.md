---
decision: FEEDBACK
summary: "Implementation satisfies most criteria but is missing the unit tests specified in PLAN.md (Step 6)"
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Cmd+click on a symbol defined in another file opens that file in a new tab

- **Status**: satisfied
- **Evidence**: `goto_cross_file_definition()` at editor_state.rs:1510 checks for existing tab via `workspace.find_tab_by_path()`, and if not found, calls `open_file_in_new_tab()` to create a new tab. The cursor is then positioned at `target_line, target_col` and `ensure_cursor_visible_in_active_tab()` is called.

### Criterion 2: If the target file is already open in a tab, that tab is activated instead of creating a duplicate

- **Status**: satisfied
- **Evidence**: `goto_cross_file_definition()` at line 1537 calls `workspace.find_tab_by_path(&target_file)` and at line 1541 calls `workspace.switch_to_tab_by_id(target_tab_id)` if found. Tests `test_switch_to_tab_by_id_same_pane`, `test_switch_to_tab_by_id_cross_pane`, and `test_switch_to_tab_by_id_already_active` verify this behavior.

### Criterion 3: The original file remains open and unmodified in its tab

- **Status**: satisfied
- **Evidence**: The implementation either switches to an existing tab or opens a new tab - it no longer calls `associate_file()` which was replacing content. The original tab is untouched. The `switch_to_tab_by_id` simply changes the active pane and tab index without modifying other tabs.

### Criterion 4: The jump stack records the original position so Ctrl+O navigates back

- **Status**: satisfied
- **Evidence**: `goto_cross_file_definition()` pushes to `workspace.jump_stack` at line 1529-1534 before navigation. The enhanced `go_back()` at line 1618 now supports cross-tab navigation via `workspace.switch_to_tab_by_id(pos.tab_id)` at line 1649, with graceful handling of closed tabs (continues to next entry).

### Criterion 5: The viewport scrolls to reveal the cursor at the definition site

- **Status**: satisfied
- **Evidence**: `goto_cross_file_definition()` calls `ensure_cursor_visible_in_active_tab()` at line 1557. This helper (line 4222) uses the viewport_scroll subsystem's `ensure_visible_wrapped()` which is documented as compliant in the subsystem's OVERVIEW.md.

### Criterion 6: The bug is verified fixed: Cmd+click `DirtyLines` in `buffer_view.rs` opens `types.rs` in a new tab with cursor on the `DirtyLines` definition

- **Status**: unclear
- **Evidence**: The code structure correctly implements the fix, but there's no automated test verifying this specific scenario. Manual verification would be needed, or an integration test.

## Feedback Items

### Issue 1: Missing unit tests from PLAN.md Step 6

- **Location**: crates/editor/src/editor_state.rs (tests module)
- **Severity**: functional
- **Confidence**: high
- **Concern**: PLAN.md Step 6 specifies 5 unit tests to be added:
  1. "Cross-file goto opens new tab (target not already open)"
  2. "Cross-file goto switches to existing tab"
  3. "Cross-file goto preserves original file"
  4. "Go-back navigates to different tab"
  5. "Go-back + goto round-trip"

  These tests for `goto_cross_file_definition()` and `go_back()` are not present. While tests exist for `switch_to_tab_by_id()` (5 tests in workspace.rs) and `StatusMessage` (5 tests in editor_state.rs), the core cross-file navigation behavior at the EditorState level is untested.

- **Suggestion**: Add the 5 unit tests as specified in PLAN.md Step 6. These tests should verify the integration of the various components (`find_tab_by_path`, `switch_to_tab_by_id`, `open_file_in_new_tab`, `jump_stack`, `ensure_cursor_visible`) when invoked through `goto_cross_file_definition()` and `go_back()`.
