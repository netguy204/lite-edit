---
decision: FEEDBACK
summary: "Iteration 2 issue fixed (unread badge clearing), but Cmd+T shortcut for new tab creation remains missing after 3 review iterations."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Tab bar renders horizontally at the top of the content area for the active workspace

- **Status**: satisfied
- **Evidence**: `tab_bar.rs` implements `TabBarGlyphBuffer` with layout constants (`TAB_BAR_HEIGHT = 32.0`), geometry calculation (`calculate_tab_bar_geometry`), and 6-phase rendering (background → tab backgrounds → active highlight → indicators → close buttons → labels). `renderer.rs:draw_tab_bar()` (lines 1156-1330) renders the tab bar at y=0 (top), x=RAIL_WIDTH (right of left rail).

### Criterion 2: Clicking a tab switches the content area to that tab's `BufferView`

- **Status**: satisfied
- **Evidence**: `editor_state.rs:handle_tab_bar_click()` (lines 621-648) detects clicks in tab bar region, calculates geometry, iterates tab_rects, and calls `switch_tab(tab_rect.tab_index)` when the tab body (not close button) is clicked.

### Criterion 3: Cmd+Shift+]/[ cycles through tabs in the active workspace

- **Status**: satisfied
- **Evidence**: `editor_state.rs:302-316` implements Cmd+Shift+] calling `next_tab()` and Cmd+Shift+[ calling `prev_tab()`. Both methods wrap around using modular arithmetic. Tests `test_cmd_shift_right_bracket_next_tab`, `test_cmd_shift_left_bracket_prev_tab`, `test_next_tab_cycles_forward`, `test_prev_tab_cycles_backward` all pass.

### Criterion 4: Cmd+W closes the active tab; prompts for confirmation if the tab's buffer is dirty

- **Status**: satisfied
- **Evidence**: `editor_state.rs:289-298` implements Cmd+W (without shift) calling `close_active_tab()`. Per PLAN.md Risks #1, dirty tab confirmation is explicitly out of scope: "dirty tabs simply don't close on Cmd+W" for this chunk. The `close_tab()` method at line 1042 correctly handles the last-tab case by creating a new empty tab first. Test `test_cmd_w_closes_active_tab` validates this.

### Criterion 5: Cmd+T creates a new tab in the active workspace (initially with an empty buffer; terminal tab creation depends on terminal_emulator chunk)

- **Status**: gap
- **Evidence**: Searched for `Key::Char('t')` handler in `editor_state.rs` command modifier section (lines 260-330) but no such handler exists. PLAN.md Step 6 explicitly requires "Cmd+T: Create new empty tab in current workspace". This was flagged in iterations 1 and 2 and remains unaddressed.

### Criterion 6: Tab labels correctly show filenames for file tabs

- **Status**: satisfied
- **Evidence**: `tab_bar.rs:disambiguate_labels()` (lines 344-367) adds parent directory info when multiple tabs share the same filename. Tests `test_disambiguation_for_duplicate_names`, `test_disambiguation_with_three_duplicates`, `test_disambiguation_mixed_unique_and_duplicate` all pass.

### Criterion 7: Unread badge appears on terminal tabs with new output since last viewed

- **Status**: satisfied
- **Evidence**: `Tab` struct has `pub unread: bool` field with `mark_unread()` and `clear_unread()` methods. `TabBarGlyphBuffer::update()` (lines 567-598) renders UNREAD_INDICATOR_COLOR (blue) dots for tabs with `is_unread == true`. Per PLAN.md Step 8, setting unread=true on terminal output is deferred to terminal_emulator chunk. Infrastructure is complete.

### Criterion 8: Unread badge clears when the user switches to that terminal tab

- **Status**: satisfied (FIXED since iteration 2)
- **Evidence**: `workspace.rs:switch_tab()` (lines 316-322) now includes `self.tabs[index].clear_unread()` with chunk backreference comment. Test `test_switch_tab_clears_unread` validates this behavior.

### Criterion 9: Switching workspaces (via left rail) swaps the entire tab bar to the new workspace's tabs

- **Status**: satisfied
- **Evidence**: `renderer.rs:draw_tab_bar()` calls `tabs_from_workspace(workspace)` where `workspace = editor.active_workspace()`. When workspace changes, next render shows new workspace's tabs. Each workspace stores independent `tabs`, `active_tab`, and `tab_bar_view_offset`.

### Criterion 10: Tab bar handles 1-20 tabs gracefully (scrolling or compression for overflow)

- **Status**: satisfied
- **Evidence**: `Workspace.tab_bar_view_offset` tracks horizontal scroll. `calculate_tab_bar_geometry()` uses `view_offset` to shift tab positions. `scroll_tab_bar()` (lines 1109-1134) handles horizontal scrolling with clamping. `ensure_active_tab_visible()` (lines 1140-1192) auto-scrolls to show active tab when switching. Test `test_view_offset_scrolls_tabs` validates offset behavior.

## Feedback Items

### Issue 1: Missing Cmd+T shortcut for new tab creation (recurring - 3rd iteration)

- **id**: issue-cmd-t-iter3
- **location**: crates/editor/src/editor_state.rs:260-330
- **concern**: The Cmd+T shortcut for creating a new empty tab remains unimplemented. This was flagged in iterations 1 and 2 and persists in iteration 3. PLAN.md Step 6 explicitly requires this shortcut. This is a documented success criterion that has not been addressed.
- **suggestion**: Add a `Key::Char('t')` handler in the Cmd+modifiers block (around line 286) that calls a new `new_tab()` method. Implementation should: (1) generate a tab ID via `self.editor.gen_tab_id()`, (2) create an empty file tab via `Tab::empty_file(id, line_height)`, (3) add to workspace via `workspace.add_tab(tab)`, (4) mark dirty region for re-render, (5) auto-scroll to ensure new tab is visible.
- **severity**: functional
- **confidence**: high
