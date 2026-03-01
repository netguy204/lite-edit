---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::goto_definition
    implements: "Same-file goto-definition scroll reveal"
  - ref: crates/editor/src/editor_state.rs#EditorState::goto_cross_file_definition
    implements: "Cross-file goto-definition scroll reveal"
  - ref: crates/editor/src/editor_state.rs#EditorState::go_back
    implements: "Go-back navigation scroll reveal"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: implements
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- terminal_spawn_reliability
- treesitter_gotodef_type_resolution
---

# Chunk Goal

## Minor Goal

When goto-definition (F12 or Cmd+click) jumps the cursor to a position outside the currently visible viewport, the view does not scroll to reveal the new cursor position. The cursor moves correctly, but the user cannot see where they landed until they manually scroll or press a key that triggers `ensure_visible`.

This chunk fixes the goto-definition flow in `EditorState::goto_definition()` to ensure the viewport scrolls to reveal the cursor after the jump, for both same-file and cross-file navigation.

## Success Criteria

- After goto-definition jumps the cursor to a line outside the visible viewport, the viewport scrolls to reveal the cursor position
- Works for both same-file jumps (Stage 1 locals resolution) and cross-file jumps (Stage 2 symbol index)
- Works in both wrapped and unwrapped modes
- The bug is verified fixed: Cmd+click a symbol whose definition is off-screen, and the view updates to show the definition


