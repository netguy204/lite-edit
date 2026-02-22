---
status: FUTURE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
code_references: []
narrative: file_picker_viewport
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after: ["renderer_styled_content", "terminal_emulator", "terminal_file_backed_scrollback", "workspace_model", "file_picker_mini_buffer", "mini_buffer_model"]
---

<!--
╔══════════════════════════════════════════════════════════════════════════════╗
║  DO NOT DELETE THIS COMMENT BLOCK until the chunk complete command is run.   ║
║                                                                              ║
║  AGENT INSTRUCTIONS: When editing this file, preserve this entire comment    ║
║  block. Only modify the frontmatter YAML and the content sections below      ║
║  (Minor Goal, Success Criteria, Relationship to Parent). Use targeted edits  ║
║  that replace specific sections rather than rewriting the entire file.       ║
╚══════════════════════════════════════════════════════════════════════════════╝
-->

# Chunk Goal

## Minor Goal

Fix the y-coordinate flip bug in `handle_mouse_selector` that causes clicks on
the file picker list to land on the wrong row — typically far above the intended
target.

macOS delivers mouse events with `y = 0` at the **bottom** of the screen. The
main buffer's hit-testing in `buffer_target.rs` handles this correctly with:

```rust
let flipped_y = (view_height as f64) - y;
```

`handle_mouse_selector` in `editor_state.rs` does not flip before forwarding to
`SelectorWidget::handle_mouse`. `list_origin_y` from `calculate_overlay_geometry`
is a top-relative offset, so the two coordinates are in opposite directions.
When a user clicks near the top of the list (small flipped_y, large raw y), the
selector computes a visible row far above row 0 and either selects the wrong item
or clamps to an out-of-bounds index.

The fix is to flip `event.position.1` using `view_height` before passing it to
`handle_mouse`, and to also flip `list_origin_y` accordingly so the selector
widget operates entirely in flipped (top = 0) coordinates — matching the
convention already established in `buffer_target.rs`.

## Success Criteria

- Clicking the first item in the file picker list selects the first item, not an
  item far above it or an out-of-bounds index.
- Clicking any visible item selects that item, not the item above it.
- The y coordinate passed to `SelectorWidget::handle_mouse` is computed as
  `view_height - raw_y`, consistent with `buffer_target.rs`.
- `list_origin_y` passed to `handle_mouse` is expressed in the same flipped
  coordinate space (i.e., `view_height - geometry.list_origin_y - list_height`,
  or the geometry is recalculated in flipped coordinates — whichever is simpler).
- All existing selector widget tests continue to pass.
- No changes to `SelectorWidget` itself; the fix lives entirely in
  `handle_mouse_selector`.
