---
status: ACTIVE
ticket: null
parent_chunk: viewport_scrolling
code_paths:
- crates/editor/src/viewport.rs
- crates/editor/src/buffer_target.rs
- crates/editor/src/renderer.rs
- crates/editor/src/glyph_buffer.rs
- crates/editor/tests/viewport_test.rs
code_references:
  - ref: crates/editor/src/viewport.rs#Viewport::scroll_offset_px
    implements: "Private field storing pixel-accurate scroll position"
  - ref: crates/editor/src/viewport.rs#Viewport::first_visible_line
    implements: "Derives integer line index from pixel offset via floor division"
  - ref: crates/editor/src/viewport.rs#Viewport::scroll_fraction_px
    implements: "Returns fractional pixel remainder for Y translation in renderer"
  - ref: crates/editor/src/viewport.rs#Viewport::set_scroll_offset_px
    implements: "Sets scroll position with pixel-space clamping to valid bounds"
  - ref: crates/editor/src/viewport.rs#Viewport::ensure_visible
    implements: "Snaps to whole-line boundary when bringing target line into view"
  - ref: crates/editor/src/viewport.rs#Viewport::scroll_to
    implements: "Scrolls to target line using pixel space internally"
  - ref: crates/editor/src/viewport.rs#Viewport::visible_range
    implements: "Uses first_visible_line with +1 for partial bottom line visibility"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::handle_scroll
    implements: "Accumulates raw pixel deltas without rounding for smooth scrolling"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphLayout::position_for_with_offset
    implements: "Position calculation with Y offset for sub-pixel rendering"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphLayout::quad_vertices_with_offset
    implements: "Quad vertex generation with Y offset for smooth scrolling"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Accepts y_offset parameter to shift all rendered content"
  - ref: crates/editor/src/renderer.rs#Renderer::update_glyph_buffer
    implements: "Passes viewport.scroll_fraction_px() as y_offset to glyph buffer"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- delete_backward_word
- fuzzy_file_matcher
- selector_rendering
- selector_widget
---

# Chunk Goal

## Minor Goal

Scrolling large documents feels disconnected and imprecise because the viewport only moves in whole-line increments. Trackpad gestures produce continuous sub-pixel delta values, but `BufferFocusTarget::handle_scroll` discards the fractional part by rounding `delta.dy / line_height` to the nearest integer before updating `scroll_offset: usize`.

This chunk makes scrolling continuous by tracking the scroll position as a floating-point pixel offset internally. The integer line index used for buffer-to-screen mapping is derived from the accumulated pixel offset, and the fractional remainder is applied as a vertical translation in the Metal renderer — so each rendered line lands at a sub-pixel-precise Y position. The result is that trackpad momentum and slow swipes produce visually smooth, proportional motion rather than discrete line-by-line jumps.

## Success Criteria

- `Viewport` gains a pixel-accurate scroll representation. The `scroll_offset` field is replaced (or augmented) by a `scroll_offset_px: f32` that accumulates raw pixel deltas without rounding.
- The integer line index (`first_visible_line`) is derived as `(scroll_offset_px / line_height).floor() as usize`, clamped to valid bounds, and available for buffer-to-screen mapping.
- The fractional pixel remainder (`scroll_offset_px % line_height`) is exposed so the renderer can apply it as a Y translation when drawing glyphs, causing the top line to be partially clipped and the bottom line to appear partially on-screen — identical to how any full-featured text editor renders mid-line scroll positions.
- Clamping works correctly in pixel space: `scroll_offset_px` is bounded between `0.0` and `(buffer_line_count - visible_lines) * line_height`, preventing scrolling past the start or end of the document.
- `ensure_visible` is updated to operate in pixel space and snap to the nearest whole-line boundary that brings the target line into view (i.e., it snaps to a pixel offset that is a multiple of `line_height`).
- All existing viewport tests continue to pass. New tests cover: (1) sub-line deltas accumulate correctly without triggering a line change, (2) deltas that cross a line boundary advance `first_visible_line` by exactly one, (3) the fractional remainder is correct after several accumulated deltas, (4) clamping at both ends works in pixel space.
- The renderer uses the fractional remainder to offset all drawn lines by `-remainder_px` in Y, so that content scrolls smoothly between line positions.

## Relationship to Parent

The `viewport_scrolling` chunk established the core scroll model: `scroll_offset: usize` tracks the first visible buffer line, `BufferFocusTarget::handle_scroll` converts `delta.dy` to a line count via integer rounding, and `ensure_visible` moves the viewport in whole-line steps.

That integer-only design was appropriate as a first pass but produces the stuttery feel described above. This chunk refines the scroll position representation from integer lines to floating-point pixels, keeping the same conceptual model (scroll is a viewport-only operation that never moves the cursor; editing snaps the viewport back) while making the position continuous.