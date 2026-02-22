---
decision: FEEDBACK
summary: "Most tab bar functionality implemented well, but Cmd+T shortcut for creating new tabs is missing and unread badge clear on tab switch is not implemented."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Tab bar renders horizontally at the top of the content area for the active workspace

- **Status**: satisfied
- **Evidence**: `tab_bar.rs` implements `TabBarGlyphBuffer` following the left rail pattern, with layout constants (`TAB_BAR_HEIGHT = 32.0`), geometry calculation (`calculate_tab_bar_geometry`), and rendering. `renderer.rs:920` calls `draw_tab_bar()` which renders the tab bar between the left rail and content area.

### Criterion 2: Clicking a tab switches the content area to that tab's `BufferView`

- **Status**: satisfied
- **Evidence**: `editor_state.rs:597-603` detects tab bar clicks and calls `handle_tab_bar_click()`. This method at line 636-647 iterates through tab rects and calls `switch_tab(tab_rect.tab_index)` when a tab body is clicked.

### Criterion 3: Cmd+Shift+]/[ cycles through tabs in the active workspace

- **Status**: satisfied
- **Evidence**: `editor_state.rs:302-316` implements both Cmd+Shift+] (calls `next_tab()`) and Cmd+Shift+[ (calls `prev_tab()`). The `next_tab()` and `prev_tab()` methods at lines 1073-1094 correctly wrap around. Tests at lines 2491-2650 validate this behavior.

### Criterion 4: Cmd+W closes the active tab; prompts for confirmation if the tab's buffer is dirty

- **Status**: satisfied (partial - per PLAN.md)
- **Evidence**: `editor_state.rs:289-298` implements Cmd+W to close the active tab via `close_active_tab()`. Per PLAN.md Step 5 and Step 6, confirmation for dirty tabs is explicitly stated as "future work" and "out of scope" for this chunk. The `close_tab()` method correctly handles the last-tab case by creating a new empty tab first.

### Criterion 5: Cmd+T creates a new tab in the active workspace (initially with an empty buffer; terminal tab creation depends on terminal_emulator chunk)

- **Status**: gap
- **Evidence**: Searched for Cmd+T handler in `editor_state.rs` key handling (lines 260-330) but no `Key::Char('t')` case with command modifier exists. PLAN.md Step 6 explicitly states "Cmd+T: Create new empty tab in current workspace" but this shortcut was not implemented.

### Criterion 6: Tab labels correctly show filenames for file tabs

- **Status**: satisfied
- **Evidence**: `tab_bar.rs:337-367` implements `disambiguate_labels()` which adds parent directory info when multiple tabs share the same filename (e.g., "src/main.rs" vs "tests/main.rs"). Tests at lines 953-1045 validate disambiguation scenarios. Labels are rendered in the tab bar via `TabBarGlyphBuffer::update()` phase 6.

### Criterion 7: Unread badge appears on terminal tabs with new output since last viewed

- **Status**: satisfied (infrastructure only)
- **Evidence**: `workspace.rs:140` has `pub unread: bool` on `Tab`. `tab_bar.rs:574-576` renders the unread indicator with blue color when `tab_info.is_unread` is true. Per PLAN.md Step 8, "Setting `unread = true` when terminal output arrives is deferred to the `terminal_emulator` chunk." The infrastructure is in place.

### Criterion 8: Unread badge clears when the user switches to that terminal tab

- **Status**: gap
- **Evidence**: PLAN.md Step 8 states "When switching tabs, call `clear_unread()` on the newly active tab." However, `switch_tab()` at `editor_state.rs:1027-1036` does not clear the unread flag. There is no `clear_unread()` method on `Tab` and no call to set `unread = false` in any switch-tab code path.

### Criterion 9: Switching workspaces (via left rail) swaps the entire tab bar to the new workspace's tabs

- **Status**: satisfied
- **Evidence**: `renderer.rs:1161-1167` gets tabs from `editor.active_workspace()`. When `switch_workspace()` is called (`editor_state.rs:1011-1015`), the active workspace changes and the next render frame calls `draw_tab_bar()` which recalculates geometry from the new workspace's tabs. Each workspace stores its own `tabs` and `active_tab`.

### Criterion 10: Tab bar handles 1-20 tabs gracefully (scrolling or compression for overflow)

- **Status**: satisfied
- **Evidence**: `tab_bar.rs` implements `view_offset` for horizontal scrolling. `calculate_tab_bar_geometry()` at lines 269-319 uses view_offset to show only visible tabs. `editor_state.rs:1105-1134` implements `scroll_tab_bar()` and `ensure_active_tab_visible()` to auto-scroll when the active tab would be offscreen. Tests at lines 896-915 validate scroll behavior.

## Feedback Items

### Issue 1: Missing Cmd+T shortcut for creating new tabs

- **id**: issue-cmd-t
- **location**: crates/editor/src/editor_state.rs:260-330
- **concern**: The keyboard handler does not implement Cmd+T to create a new empty tab. PLAN.md Step 6 explicitly calls for this: "Cmd+T: Create new empty tab in current workspace". This is a documented feature that was not implemented.
- **suggestion**: Add a handler in the Cmd+modifiers block (around line 280) that checks for `Key::Char('t')` and creates a new empty tab, similar to how `new_workspace()` is called for Cmd+N. Create a `new_tab()` method that generates a tab ID, creates an empty file tab, and adds it to the active workspace.
- **severity**: functional
- **confidence**: high

### Issue 2: Unread badge not cleared when switching to tab

- **id**: issue-unread-clear
- **location**: crates/editor/src/editor_state.rs:1027-1036
- **concern**: PLAN.md Step 8 specifies "When switching tabs, call `clear_unread()` on the newly active tab." The `switch_tab()` method does not clear the unread flag. When a user switches to a terminal tab that has an unread badge, the badge should disappear.
- **suggestion**: In `switch_tab()`, after changing the active tab, set `workspace.tabs[index].unread = false`. Alternatively, implement `Tab::clear_unread()` and call it.
- **severity**: functional
- **confidence**: high
