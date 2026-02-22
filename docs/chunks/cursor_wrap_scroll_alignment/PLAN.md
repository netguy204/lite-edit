# Implementation Plan

## Approach

The root cause is a coordinate-space mismatch: `Viewport::ensure_visible_wrapped()` sets
`scroll_offset_px` in screen-row units, but `Viewport::first_visible_line()` interprets it
as buffer-line units. The fix introduces wrap-aware coordinate conversion methods and
updates the rendering pipeline to use them.

Key insight: When wrapping is active, `scroll_offset_px` represents a position in
**screen row** space. To render correctly, we must:
1. Convert that screen row to the corresponding buffer line
2. Track how many rows of the first visible buffer line are scrolled off-screen
3. Use this offset when accumulating screen rows during rendering

No changes are needed to `ensure_visible_wrapped()` itself - it correctly computes scroll
positions in screen-row space. The fix is in how rendering interprets that scroll position.

## Subsystem Considerations

No subsystems are directly affected by this chunk. The fix is localized to viewport
coordinate conversion and glyph buffer rendering.

## Sequence

### Step 1: Add wrap-aware viewport methods

Location: `crates/editor/src/viewport.rs`

Add two new methods to Viewport:

1. `first_visible_screen_row()` - Returns `floor(scroll_offset_px / line_height)` as a
   screen row index. Identical to `first_visible_line()` in value, but with semantics
   clarified: this is a screen row, not a buffer line.

2. `buffer_line_for_screen_row()` - Static method that, given a target screen row and
   wrap layout, returns:
   - The buffer line containing that screen row
   - The row offset within that buffer line (0 = first row)
   - Cumulative screen rows before this buffer line

This enables the renderer to correctly map scroll positions in screen-row space back to
buffer coordinates.

### Step 2: Update glyph buffer wrap rendering

Location: `crates/editor/src/glyph_buffer.rs`

Modify `update_from_buffer_with_wrap()` to:

1. Use `first_visible_screen_row()` to get the scroll position in screen-row units
2. Call `buffer_line_for_screen_row()` to find:
   - `first_visible_buffer_line`: Which buffer line is at the viewport top
   - `screen_row_offset_in_line`: How many rows of that line are scrolled off
3. Track `cumulative_screen_row` starting from 0 (viewport top)
4. For the first buffer line, skip `screen_row_offset_in_line` rows worth of content
5. Adjust cursor, selection, border, and glyph positioning accordingly

This ensures all rendered content is positioned relative to the correct baseline.

### Step 3: Add comprehensive tests

Location: `crates/editor/src/viewport.rs` (unit tests) and
`crates/editor/tests/wrap_test.rs` (integration tests)

Add tests covering the success criteria scenarios:
- Cursor on unwrapped line with wrapped lines above
- Cursor on continuation row of wrapped line
- Cursor at document start with no wrapped lines
- `ensure_visible_wrapped` with cursor below viewport
- Cursor position after partial scroll (row offset > 0)
- Cursor above viewport returns None

## Dependencies

- `line_wrap_rendering` chunk (completed) - Provides `WrapLayout` and
  `ensure_visible_wrapped`

## Risks and Open Questions

- **Performance**: The `buffer_line_for_screen_row()` method iterates through buffer
  lines from the start. For documents with many lines, this could be O(n). However,
  this is called once per render frame at most, and the iteration is simple arithmetic.
  If profiling shows issues, we could cache the mapping or use binary search.

## Deviations

- The original plan did not specify a static method `buffer_line_for_screen_row()`.
  During implementation, making it static (taking the wrap layout as parameter) proved
  cleaner since it doesn't need viewport state beyond what's passed explicitly.

- Added tracking of `is_first_buffer_line` flag in the rendering loop to correctly
  handle the `screen_row_offset_in_line` only for the first visible buffer line.
