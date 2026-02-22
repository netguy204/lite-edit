---
decision: FEEDBACK
summary: Core tab bar functionality is solid but Cmd+T shortcut (new tab) and unread badge clearing on tab switch are still not implemented from iteration 1 feedback.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Tab bar renders horizontally at the top of the content area for the active workspace

- **Status**: satisfied
- **Evidence**: `tab_bar.rs` implements `TabBarGlyphBuffer` and `calculate_tab_bar_geometry()`. `renderer.rs:draw_tab_bar()` calls these and renders the tab bar at y=0 (top), x=RAIL_WIDTH (right of left rail). `TAB_BAR_HEIGHT` constant (32px) defines the strip height.

### Criterion 2: Clicking a tab switches the content area to that tab's `BufferView`

- **Status**: satisfied
- **Evidence**: `editor_state.rs:handle_tab_bar_click()` (lines 621-648) detects clicks in tab rects and calls `switch_tab(tab_rect.tab_index)`. The `switch_tab()` method at line 1027 delegates to `workspace.switch_tab(index)` and marks dirty for re-render.

### Criterion 3: Cmd+Shift+]/[ cycles through tabs in the active workspace

- **Status**: satisfied
- **Evidence**: `editor_state.rs:302-316` implements Cmd+Shift+] calling `next_tab()` and Cmd+Shift+[ calling `prev_tab()`. Tests `test_cmd_shift_right_bracket_next_tab` and `test_cmd_shift_left_bracket_prev_tab` confirm the behavior works with wraparound.

### Criterion 4: Cmd+W closes the active tab; prompts for confirmation if the tab's buffer is dirty

- **Status**: satisfied
- **Evidence**: `editor_state.rs:289-298` implements Cmd+W (without shift) calling `close_active_tab()`. PLAN.md explicitly states dirty tab close confirmation is out of scope (see Risks #1: "dirty tabs simply don't close on Cmd+W"). The close_tab method at line 1042 handles the last-tab case by creating a new empty tab first.

### Criterion 5: Cmd+T creates a new tab in the active workspace (initially with an empty buffer; terminal tab creation depends on terminal_emulator chunk)

- **Status**: gap
- **Evidence**: No `Key::Char('t')` handler exists in `handle_key()` (lines 260-330). The PLAN.md Step 6 explicitly calls for "Cmd+T: Create new empty tab in current workspace", but this shortcut is not implemented. Previous review iteration 1 flagged this same issue.

### Criterion 6: Tab labels correctly show filenames for file tabs

- **Status**: satisfied
- **Evidence**: `tab_bar.rs:disambiguate_labels()` (lines 344-367) handles filename disambiguation when multiple tabs share the same filename by adding parent directory. Tests `test_disambiguation_for_duplicate_names`, `test_disambiguation_with_three_duplicates`, and `test_disambiguation_mixed_unique_and_duplicate` verify this works correctly.

### Criterion 7: Unread badge appears on terminal tabs with new output since last viewed

- **Status**: satisfied
- **Evidence**: `tab_bar.rs` TabBarGlyphBuffer::update() (lines 567-598) renders indicator dots with `UNREAD_INDICATOR_COLOR` (blue) for tabs where `tab_info.is_unread == true`. The `Tab` struct has an `unread: bool` field. Note: Setting `unread = true` on output is deferred to terminal_emulator chunk per PLAN.md Step 8.

### Criterion 8: Unread badge clears when the user switches to that terminal tab

- **Status**: gap
- **Evidence**: `switch_tab()` at lines 1027-1036 calls `workspace.switch_tab(index)` but does NOT clear the `unread` flag on the newly-active tab. The PLAN.md Step 8 explicitly states "When switching tabs, call `clear_unread()` on the newly active tab." Previous review iteration 1 flagged this same issue.

### Criterion 9: Switching workspaces (via left rail) swaps the entire tab bar to the new workspace's tabs

- **Status**: satisfied
- **Evidence**: `renderer.rs:draw_tab_bar()` (lines 1156-1330) calls `tabs_from_workspace(workspace)` where `workspace = editor.active_workspace()`. When the active workspace changes (via left rail click or Cmd+1-9), the next render will draw that workspace's tabs.

### Criterion 10: Tab bar handles 1-20 tabs gracefully (scrolling or compression for overflow)

- **Status**: satisfied
- **Evidence**: `Workspace.tab_bar_view_offset` tracks scroll position. `calculate_tab_bar_geometry()` uses `view_offset` parameter to shift tab positions. `scroll_tab_bar()` at lines 1109-1134 handles horizontal scrolling with clamping. `ensure_active_tab_visible()` at lines 1140-1192 auto-scrolls to show active tab. Tests `test_view_offset_scrolls_tabs` verifies scroll offset shifts tabs.

## Feedback Items

### Issue 1: Missing Cmd+T shortcut for new tab creation

- **Location**: crates/editor/src/editor_state.rs:260-330
- **Severity**: functional
- **Confidence**: high
- **Concern**: Cmd+T shortcut for creating new tabs is not implemented. This was flagged in iteration 1 and remains unaddressed. PLAN.md Step 6 explicitly requires this shortcut.
- **Suggestion**: Add a `Key::Char('t')` handler in the command modifier section (around line 286) that calls a new `new_empty_tab()` method. The method should generate a tab ID, create an empty file tab, add it to the workspace, and mark dirty.

### Issue 2: Unread badge not cleared on tab switch

- **Location**: crates/editor/src/editor_state.rs:1027-1036
- **Severity**: functional
- **Confidence**: high
- **Concern**: When switching to a tab, the `unread` flag is not cleared. This was flagged in iteration 1 and remains unaddressed. PLAN.md Step 8 explicitly states "When switching tabs, call `clear_unread()` on the newly active tab."
- **Suggestion**: In `switch_tab()`, after calling `workspace.switch_tab(index)`, add: `if let Some(tab) = workspace.tabs.get_mut(index) { tab.unread = false; }`
