---
decision: APPROVE
summary: All success criteria are satisfied. The coordinate space mismatch is fixed with new wrap-aware methods, rendering uses correct baseline, and comprehensive tests verify all scenarios.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: The cursor's vertical position is stable and correct at all scroll positions.

- **Status**: satisfied
- **Evidence**: The fix in `update_from_buffer_with_wrap()` (glyph_buffer.rs:1086-1096) uses `first_visible_screen_row()` to get the scroll position in screen-row units, then calls `buffer_line_for_screen_row()` to find the correct buffer line and row offset. The rendering tracks `cumulative_screen_row` relative to viewport top, starting at 0 and accounting for `screen_row_offset_in_line` on the first buffer line. Tests in wrap_test.rs verify cursor positioning at various scroll positions.

### Criterion 2: `Viewport::ensure_visible_wrapped` sets `scroll_offset_px` such that `first_visible_line()` returns the correct buffer line index when wrapping is active.

- **Status**: satisfied
- **Evidence**: `ensure_visible_wrapped()` in viewport.rs:248-309 correctly computes scroll positions in screen-row space. The new `first_visible_screen_row()` method (viewport.rs:85-90) returns the screen row index from `scroll_offset_px`. The rendering code now uses `buffer_line_for_screen_row()` (viewport.rs:102-129) to convert this to the correct buffer line and row offset, ensuring coordinate spaces align.

### Criterion 3: The cursor rendering path in `GlyphBuffer::update_from_buffer_with_wrap` accumulates `cumulative_screen_row` from the correct baseline.

- **Status**: satisfied
- **Evidence**: In glyph_buffer.rs:1155-1240 (Phase 1: Selection Quads), the rendering loop initializes `cumulative_screen_row = 0` and tracks `is_first_buffer_line`. For the first buffer line, it uses `start_row_offset = screen_row_offset_in_line` to skip rows above viewport. The same pattern is applied consistently in Phase 2 (Border Quads), Phase 3 (Glyph Quads), and Phase 4 (Cursor Quad) at lines 1246-1287, 1293-1373, and 1375-1443 respectively.

### Criterion 4: No regression to any existing behaviour in the non-wrapped rendering path (`update_from_buffer_with_cursor`), viewport arithmetic, or scroll clamping.

- **Status**: satisfied
- **Evidence**: The non-wrapped path `update_from_buffer_with_cursor()` (glyph_buffer.rs:514-830) remains unchanged. All 46 viewport unit tests pass, including the existing tests for `first_visible_line()`, `visible_range()`, `ensure_visible()`, and scroll clamping. The implementation adds new methods (`first_visible_screen_row()`, `buffer_line_for_screen_row()`) without modifying existing ones.

### Criterion 5: Tests covering the required scenarios.

- **Status**: satisfied
- **Evidence**: wrap_test.rs contains 21 tests including the specific scenarios. viewport.rs contains 7 new tests for the wrap-aware methods:
  - `test_first_visible_screen_row` (viewport.rs:886-901)
  - `test_buffer_line_for_screen_row_no_wrapping` (viewport.rs:903-933)
  - `test_buffer_line_for_screen_row_with_wrapping` (viewport.rs:936-1002)
  - `test_buffer_line_for_screen_row_past_end` (viewport.rs:1004-1023)
  - `test_cursor_on_unwrapped_line_with_wrapped_lines_above` (viewport.rs:1025-1049)
  - `test_cursor_on_continuation_row` (viewport.rs:1051-1082)

### Criterion 6: Cursor on a line that does not wrap, with wrapped lines above it.

- **Status**: satisfied
- **Evidence**: Test `test_cursor_on_unwrapped_line_with_wrapped_lines_above_scrolled_to_top` in wrap_test.rs:393-417 verifies that with line 0 having 200 chars (3 screen rows) and cursor on line 1 (unwrapped), the cursor appears at screen row 3. Additionally, viewport.rs:1025-1049 tests `buffer_line_for_screen_row()` for this scenario.

### Criterion 7: Cursor on the continuation row (second or later screen row) of a wrapped buffer line.

- **Status**: satisfied
- **Evidence**: Test `test_cursor_on_continuation_row_of_wrapped_line` in wrap_test.rs:419-439 verifies cursor at col 100 of a 200-char line (80 cols/row) appears at screen row 1. Test `test_cursor_on_continuation_row` in viewport.rs:1051-1082 verifies `buffer_line_for_screen_row()` correctly returns `row_offset=1` and `row_offset=2` for the continuation rows.

### Criterion 8: Cursor at the document start with no wrapped lines above.

- **Status**: satisfied
- **Evidence**: Test `test_cursor_at_document_start_no_wrapped_lines_above` in wrap_test.rs:442-458 verifies cursor at line 0, col 0 appears at screen row 0. Test `test_buffer_line_for_screen_row_no_wrapping` in viewport.rs:903-933 verifies the mapping returns `(0, 0, 0)` for screen row 0.

### Criterion 9: `ensure_visible_wrapped` called when the cursor is below the viewport with wrapped lines above.

- **Status**: satisfied
- **Evidence**: Test `test_ensure_visible_wrapped_cursor_below_viewport` in wrap_test.rs:461-489 verifies that with wrapped lines (200, 50, 150, 100 chars at 80 cols/row), cursor at line 3, col 0 is correctly calculated to be at screen row 6. Test `test_cursor_visible_after_partial_scroll` in wrap_test.rs:491-511 verifies cursor calculation after partial scroll with `screen_row_offset_in_first = 1`.
