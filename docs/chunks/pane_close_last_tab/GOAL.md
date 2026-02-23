---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::close_tab
    implements: "Handle empty pane cleanup when closing last tab in multi-pane layout"
  - ref: crates/editor/src/workspace.rs#Workspace::find_fallback_focus
    implements: "Find adjacent pane to receive focus when current pane is removed"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- tiling_focus_keybindings
- tiling_multi_pane_render
- startup_workspace_dialog
---

# Chunk Goal

## Minor Goal

Fix crash when closing the last tab in a pane that is part of a multi-pane layout. Currently, `EditorState::close_tab` closes the tab via `pane.close_tab(index)` when `pane_count > 1`, but does not handle the resulting empty pane. Subsequent code at line ~1521 calls `active_tab().expect("no active tab")` on the now-empty pane, causing a panic.

The fix should call `cleanup_empty_panes` (which already exists in `pane_layout.rs`) after closing the last tab in a pane, and ensure the active pane focus moves to an adjacent pane.

## Success Criteria

- Closing the last tab in a pane with multiple panes removes the empty pane from the layout via `cleanup_empty_panes`
- Focus moves to an adjacent pane after the empty pane is removed
- No panic occurs when closing the last tab in any pane configuration
- Single-pane single-tab behavior is unchanged (still replaces with empty tab)
- Existing tests continue to pass

