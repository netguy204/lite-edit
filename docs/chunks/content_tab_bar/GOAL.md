---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/tab_bar.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/tab_bar.rs#TabInfo
    implements: "Tab metadata (label, kind, dirty, unread) used for rendering"
  - ref: crates/editor/src/tab_bar.rs#TabBarGeometry
    implements: "Layout geometry for all tabs including scroll offset"
  - ref: crates/editor/src/tab_bar.rs#calculate_tab_bar_geometry
    implements: "Tab width/position computation with horizontal scroll support"
  - ref: crates/editor/src/tab_bar.rs#TabBarGlyphBuffer
    implements: "Glyph-level rendering of the tab bar strip"
  - ref: crates/editor/src/workspace.rs#Tab
    implements: "Per-tab model: kind (file/terminal), buffer ref, dirty flag, unread badge"
  - ref: crates/editor/src/workspace.rs#Workspace
    implements: "Owns the tab list and tab_bar_view_offset for horizontal scroll"
  - ref: crates/editor/src/editor_state.rs#EditorState::switch_tab
    implements: "Switch active tab; clears unread badge on target tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::close_tab
    implements: "Close tab with dirty-buffer guard (Cmd+W)"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_tab
    implements: "Create new empty file tab (Cmd+T)"
  - ref: crates/editor/src/editor_state.rs#EditorState::scroll_tab_bar
    implements: "Horizontal tab bar scroll and auto-scroll to active tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_tab_bar_click
    implements: "Click-to-switch and close-button hit testing"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- workspace_model
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Implement the content tab bar — the second level of the tab hierarchy. This is the horizontal tab bar at the top of the content area showing the active workspace's open tabs. Each tab represents a `BufferView` — files, terminals, diffs — unified through the same trait.

**Tab bar rendering:**
- Horizontal strip at the top of the content area (to the right of the left rail)
- Each tab shows: label (filename, "Terminal", etc.), kind indicator (icon or color), dirty/unread badge
- Active tab is visually highlighted
- Tabs are heterogeneous — file tabs, terminal tabs, and future diff tabs all appear in the same bar
- Tab bar scrolls horizontally if there are too many tabs to fit

**Tab interactions:**
- Click tab to switch to it
- Cmd+Shift+] / Cmd+Shift+[ to cycle forward/backward through tabs
- Cmd+W to close the active tab (with confirmation for unsaved files)
- Cmd+T to open a new terminal tab in the current workspace
- Cmd+O to open a file tab (file picker)
- Middle-click or close button on tab to close it

**Unread badges for terminal tabs:**
- When a terminal tab has new output since the user last viewed it, show a badge (dot or count)
- Badge clears when the user switches to that tab
- This is the within-workspace equivalent of the left rail's workspace-level notification

**Tab label derivation:**
- File tabs: filename (e.g., "main.rs"), with path disambiguation if multiple files share a name
- Terminal tabs: "Terminal" or shell name, optionally with running command
- The active tab's full path or title shown in a breadcrumb or title bar area

## Success Criteria

- Tab bar renders horizontally at the top of the content area for the active workspace
- Clicking a tab switches the content area to that tab's `BufferView`
- Cmd+Shift+]/[ cycles through tabs in the active workspace
- Cmd+W closes the active tab; prompts for confirmation if the tab's buffer is dirty
- Cmd+T creates a new tab in the active workspace (initially with an empty buffer; terminal tab creation depends on terminal_emulator chunk)
- Tab labels correctly show filenames for file tabs
- Unread badge appears on terminal tabs with new output since last viewed
- Unread badge clears when the user switches to that terminal tab
- Switching workspaces (via left rail) swaps the entire tab bar to the new workspace's tabs
- Tab bar handles 1-20 tabs gracefully (scrolling or compression for overflow)