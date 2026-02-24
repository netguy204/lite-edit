---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::get_pane_content_dimensions
    implements: "Helper to compute pane-local content dimensions for scroll clamping"
  - ref: crates/editor/src/editor_state.rs#EditorState::scroll_pane
    implements: "Uses pane-specific dimensions instead of full-window dimensions"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: uses
- subsystem_id: renderer
  relationship: uses
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- welcome_scroll
---

# Chunk Goal

## Minor Goal

Fix a bug where vertical split panes cannot scroll to the end of a long file.
After opening a vertical split, attempting to scroll past the visible region
fails—the view refuses to advance to the bottom of the document. Removing the
split (returning to a single pane) immediately restores correct scrolling
behavior. This points to a constraint or clamp in scroll position logic that
incorrectly accounts for pane geometry when multiple panes are active.

## Success Criteria

- After creating a vertical split, each pane can independently scroll to the
  last line of its document.
- The scroll position is not artificially clamped to a value that would only
  be correct for a full-window single-pane layout.
- Existing single-pane scrolling behavior is unaffected.
- The bug is reproducible by opening a file longer than the window, splitting
  vertically, and scrolling downward; after the fix this reaches the final line.