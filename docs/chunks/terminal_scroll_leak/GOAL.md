---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/viewport.rs
- crates/editor/src/row_scroller.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_active_tab_viewport
    implements: "Pane-aware viewport sync that uses actual pane content height instead of full window height in multi-pane layouts"
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_pane_viewports
    implements: "Dimension-change guard that skips redundant viewport update_size calls for non-terminal tabs whose visible_rows wouldn't change"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- tsx_goto_functions
---
# Chunk Goal

## Minor Goal

Opening a new terminal pane in a vertical split causes adjacent buffer panes to scroll unexpectedly. Specifically: with a vertically split layout (terminal on the left, buffer scrolled to bottom on the right), opening a new terminal tab on the left side causes the right-side buffer to scroll up by roughly one page. Opening a new buffer on the left does not trigger this behavior — only terminal creation does.

This suggests that terminal initialization is triggering a layout recalculation or resize event that incorrectly propagates to sibling panes, causing their scroll positions to be recalculated or reset. The buffer pane's scroll offset should be preserved across layout changes in adjacent panes.

## Success Criteria

- Opening a new terminal in one side of a vertical split does not affect the scroll position of a buffer in the other side
- Buffer scroll positions are preserved across all sibling pane operations (open, close, resize) that don't directly affect the buffer's own dimensions
- The fix correctly distinguishes between resize events that affect a pane's own size (which may legitimately need scroll adjustment) vs. events from sibling panes (which should not)