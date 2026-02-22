---
status: ACTIVE
ticket: null
parent_chunk: line_wrap_rendering
code_paths:
- crates/editor/src/viewport.rs
- crates/editor/src/glyph_buffer.rs
code_references:
  - ref: crates/editor/src/viewport.rs#Viewport::first_visible_screen_row
    implements: "Returns scroll position as screen row (not buffer line), enabling correct coordinate space interpretation in wrapped mode"
  - ref: crates/editor/src/viewport.rs#Viewport::buffer_line_for_screen_row
    implements: "Maps screen row to buffer line + row offset, enabling correct baseline calculation for wrapped rendering"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_wrap
    implements: "Uses wrap-aware coordinate conversion for correct cursor/selection/glyph positioning with wrapped lines"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- text_buffer
- buffer_view_trait
- file_picker_scroll
- line_wrap_rendering
- renderer_styled_content
---

# Chunk Goal

## Minor Goal

The cursor's vertical screen position is wrong—and variably so depending on how
far the viewport has been scrolled—whenever wrapped lines are present in the
document. The cursor may appear one or more rows above or below the text it
actually belongs to.

The root cause is a coordinate-space mismatch introduced in
`line_wrap_rendering`. `Viewport::first_visible_line()` derives the first
visible buffer line as `floor(scroll_offset_px / line_height)`, treating each
unit of scroll as exactly one buffer line. But `Viewport::ensure_visible_wrapped`
sets `scroll_offset_px` in *visual* (screen) row pixel space: it sums up the
screen rows occupied by each buffer line's wrapped output and sets
`scroll_offset_px = target_screen_row * line_height`. When wrapped buffer lines
are present, visual screen row N does not equal buffer line N. The two subsystems
speak different units, so `first_visible_line()` returns the wrong buffer line
index, and the render loop in `update_from_buffer_with_wrap` accumulates
`cumulative_screen_row` from the wrong starting point, placing the cursor quad
at the wrong Y position on screen.

This chunk fixes the mismatch so that the cursor always appears at the exact
pixel row of the character it occupies, regardless of scroll position or the
number of wrapped lines visible above it.

## Success Criteria

- The cursor's vertical position is stable and correct at all scroll positions.
  Scrolling a document that contains wrapped lines does not cause the cursor to
  drift up or down relative to the text it is on.
- `Viewport::ensure_visible_wrapped` sets `scroll_offset_px` such that
  `first_visible_line()` returns the correct buffer line index when wrapping is
  active. The two are always in the same coordinate space.
- The cursor rendering path in `GlyphBuffer::update_from_buffer_with_wrap`
  accumulates `cumulative_screen_row` from the correct baseline: the viewport's
  true first visible *buffer* line, not an incorrectly-derived approximation.
- No regression to any existing behaviour in the non-wrapped rendering path
  (`update_from_buffer_with_cursor`), viewport arithmetic, or scroll clamping.
- Tests covering at least the following scenarios:
  - Cursor on a line that does not wrap, with wrapped lines above it.
  - Cursor on the continuation row (second or later screen row) of a wrapped
    buffer line.
  - Cursor at the document start with no wrapped lines above.
  - `ensure_visible_wrapped` called when the cursor is below the viewport with
    wrapped lines above.

## Relationship to Parent

`line_wrap_rendering` introduced soft wrapping, `WrapLayout`, and
`ensure_visible_wrapped`. The scroll arithmetic in that chunk conflated visual
screen rows with buffer line indices when computing and applying `scroll_offset_px`,
which is what causes the cursor hot-position to be variable. The coordinate
mapping inside `WrapLayout` itself is correct; only the scroll-offset accounting
in `Viewport::ensure_visible_wrapped` and the baseline used by the cursor-quad
emit in `GlyphBuffer::update_from_buffer_with_wrap` need to be corrected.
