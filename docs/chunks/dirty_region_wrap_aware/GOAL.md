---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/viewport.rs
  - crates/editor/src/context.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/viewport.rs#Viewport::buffer_line_to_abs_screen_row
    implements: "Helper to convert buffer line index to cumulative absolute screen row"
  - ref: crates/editor/src/viewport.rs#Viewport::dirty_lines_to_region_wrapped
    implements: "Wrap-aware conversion from buffer-space DirtyLines to screen-space DirtyRegion"
  - ref: crates/editor/src/context.rs#EditorContext::mark_dirty
    implements: "Uses wrap-aware conversion when marking lines dirty"
  - ref: crates/editor/src/editor_state.rs#EditorState::cursor_dirty_region
    implements: "Uses wrap-aware conversion for cursor dirty region computation"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- tiling_tree_model
---

# Chunk Goal

## Minor Goal

Make `dirty_lines_to_region()` in `viewport.rs` wrap-aware so that cursor repositioning via mouse click triggers a repaint at all scroll positions, not just near the top of the document.

### Root Cause

`Viewport::dirty_lines_to_region()` converts buffer-space `DirtyLines` to screen-space `DirtyRegion` using `first_visible_line()`, which returns a **screen row** index in wrapped mode (via `RowScroller::first_visible_row()`). However, `DirtyLines::Single(line)` and the other variants carry **buffer line** indices. The comparison `buffer_line >= first_visible_screen_row` is a type mismatch: buffer lines vs screen rows.

When a file has many long lines that wrap (e.g., `docs/investigations/tiling_pane_layout/OVERVIEW.md` with 55 lines over 200 chars, max 843 chars), screen rows accumulate much faster than buffer lines. At a halfway scroll position, `first_visible_line()` might return screen row ~400 while the clicked buffer line is ~250. The visibility check `250 >= 400` fails, producing `DirtyRegion::None`, and the cursor is never repainted.

The same bug exists in `cursor_dirty_region()` in `editor_state.rs` which also calls `dirty_lines_to_region` with a buffer line index.

### Why it's intermittent

- **Works near top of file**: Cumulative wrapping is small, so screen rows ≈ buffer lines.
- **Works after scrolling**: Scroll events mark `DirtyRegion::FullViewport`, which forces a full repaint that reveals the correctly-positioned cursor.
- **Not reproducible in `.rs` files**: Rust files have short lines (avg 37 chars for `editor_state.rs`), so screen rows and buffer lines stay nearly 1:1 at all scroll positions.
- **Not reproducible in most `.md` files**: Only files with many very long lines (>200 chars) create enough screen-row divergence to trigger the bug at reachable scroll positions.

## Success Criteria

- `dirty_lines_to_region()` converts buffer line indices to screen row indices (using `WrapLayout` or `buffer_line_for_screen_row`) before comparing against the viewport's screen-row-based scroll position.
- Clicking at any scroll position in a file with heavy line wrapping (e.g., OVERVIEW.md with 843-char lines) immediately repaints the cursor at the clicked position without requiring a subsequent scroll.
- The `cursor_dirty_region()` method in `editor_state.rs` also produces correct dirty regions under wrapping.
- Existing `dirty_lines_to_region` unit tests continue to pass (they test the no-wrap case where screen rows = buffer lines).
- New unit tests verify correct dirty region computation when buffer lines wrap to multiple screen rows, specifically testing that a buffer line visible on screen but with a buffer index < `first_visible_screen_row` still produces a non-None dirty region.

## Key Code Locations

- **`crates/editor/src/viewport.rs:449`** — `dirty_lines_to_region()`: the buggy function that compares buffer lines against screen rows
- **`crates/editor/src/context.rs:118`** — `mark_cursor_dirty()`: calls `mark_dirty(DirtyLines::Single(cursor_line))` with a buffer line index
- **`crates/editor/src/editor_state.rs:1778`** — `cursor_dirty_region()`: same bug, also uses `dirty_lines_to_region` with buffer line
- **`crates/editor/src/buffer_target.rs:508`** — mouse click handler that calls `ctx.mark_cursor_dirty()` after positioning cursor