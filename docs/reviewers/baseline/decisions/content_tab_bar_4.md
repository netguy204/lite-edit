---
decision: FEEDBACK
summary: "Implementation is nearly complete but close_tab() does not guard against closing dirty (unsaved) tabs as specified in PLAN.md"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Tab bar renders horizontally at the top of the content area for the active workspace

- **Status**: satisfied
- **Evidence**: `tab_bar.rs` implements `TabBarGlyphBuffer` which renders the tab bar using Metal. `renderer.rs` calls `draw_tab_bar()` at line 938 after drawing the left rail. The tab bar is positioned at `y=0` (top of content area) and starts at `x=RAIL_WIDTH` (to the right of the left rail).

### Criterion 2: Clicking a tab switches the content area to that tab's `BufferView`

- **Status**: satisfied
- **Evidence**: `editor_state.rs:handle_tab_bar_click()` (line 1478) calculates tab geometry and checks `tab_rect.contains(mouse_x, mouse_y)`. If matched (and not the close button), it calls `self.switch_tab(idx)` which delegates to `workspace.switch_tab()`.

### Criterion 3: Cmd+Shift+]/[ cycles through tabs in the active workspace

- **Status**: satisfied
- **Evidence**: `editor_state.rs:handle_key()` (lines 316-331) intercepts `Cmd+Shift+]` and `Cmd+Shift+[` and calls `next_tab()` / `prev_tab()`. These methods implement wraparound cycling. Tests verify this behavior: `test_cmd_shift_right_bracket_next_tab`, `test_next_tab_cycles_forward`.

### Criterion 4: Cmd+W closes the active tab; prompts for confirmation if the tab's buffer is dirty

- **Status**: gap
- **Evidence**: `editor_state.rs:handle_key()` (lines 305-313) intercepts `Cmd+W` and calls `close_active_tab()`. However, `close_tab()` (lines 1342-1358) does NOT check if the tab is dirty before closing. The PLAN.md Step 5 explicitly states "Check if tab is dirty, if so skip close (confirmation is future work)" but this check is missing in the implementation.

### Criterion 5: Cmd+T creates a new tab in the active workspace (initially with an empty buffer; terminal tab creation depends on terminal_emulator chunk)

- **Status**: satisfied
- **Evidence**: `editor_state.rs:handle_key()` (lines 333-341) intercepts `Cmd+T` and calls `new_tab()`. The `new_tab()` method (lines 1398-1414) creates an empty file tab via `Tab::empty_file()` and adds it to the workspace. Tests verify: `test_cmd_t_creates_new_tab`.

### Criterion 6: Tab labels correctly show filenames for file tabs

- **Status**: satisfied
- **Evidence**: `tab_bar.rs:tabs_from_workspace()` extracts labels from tabs. `disambiguate_labels()` (lines 344-367) adds parent directory when filenames collide. Tests verify: `test_no_disambiguation_for_unique_names`, `test_disambiguation_for_duplicate_names`, `test_disambiguation_with_three_duplicates`.

### Criterion 7: Unread badge appears on terminal tabs with new output since last viewed

- **Status**: satisfied
- **Evidence**: `workspace.rs:Tab` has an `unread: bool` field with `mark_unread()` and `clear_unread()` methods. `tab_bar.rs:TabBarGlyphBuffer::update()` (lines 566-598) renders unread indicators with `UNREAD_INDICATOR_COLOR` when `tab_info.is_unread` is true. Note: Actually setting `unread=true` when terminal output arrives is deferred to `terminal_emulator` chunk per PLAN.md.

### Criterion 8: Unread badge clears when the user switches to that terminal tab

- **Status**: satisfied
- **Evidence**: `workspace.rs:Workspace::switch_tab()` (lines 316-322) calls `self.tabs[index].clear_unread()` when switching tabs. Test `test_switch_tab_clears_unread` verifies this behavior.

### Criterion 9: Switching workspaces (via left rail) swaps the entire tab bar to the new workspace's tabs

- **Status**: satisfied
- **Evidence**: `renderer.rs:draw_tab_bar()` reads the active workspace via `editor.active_workspace()` and calls `tabs_from_workspace()` to get the current workspace's tabs. When workspace changes via left rail click or Cmd+1-9, the renderer draws the new workspace's tab bar on next frame.

### Criterion 10: Tab bar handles 1-20 tabs gracefully (scrolling or compression for overflow)

- **Status**: satisfied
- **Evidence**: `tab_bar.rs:TabBarGeometry` tracks `view_offset` for horizontal scrolling. `workspace.rs:Workspace` has `tab_bar_view_offset: f32`. `editor_state.rs:ensure_active_tab_visible()` (lines 1432-1469) auto-scrolls to keep active tab visible. `scroll_tab_bar()` allows manual scrolling. Tests: `test_view_offset_scrolls_tabs`.

## Feedback Items

### Issue 1: Dirty tab close protection missing

- **ID**: issue-dirty-guard
- **Location**: `crates/editor/src/editor_state.rs:1342-1358` (close_tab method)
- **Concern**: The `close_tab()` method closes tabs unconditionally without checking `tab.dirty`. PLAN.md Step 5 explicitly states: "Check if tab is dirty, if so skip close (confirmation is future work)". This guard is missing, allowing unsaved work to be lost without warning.
- **Suggestion**: Add a dirty check before closing:
  ```rust
  pub fn close_tab(&mut self, index: usize) {
      if let Some(workspace) = self.editor.active_workspace_mut() {
          // Guard: don't close dirty tabs
          if let Some(tab) = workspace.tabs.get(index) {
              if tab.dirty {
                  return; // Skip close for dirty tabs (confirmation UI is future work)
              }
          }
          // ... rest of close logic
      }
  }
  ```
- **Severity**: functional
- **Confidence**: high
