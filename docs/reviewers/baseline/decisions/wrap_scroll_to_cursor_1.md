---
decision: APPROVE
summary: "All success criteria satisfied: ensure_visible_wrapped now computes cursor position from buffer line 0, eliminating the coordinate-space confusion, with four well-targeted tests covering all specified scenarios."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `ensure_visible_wrapped` receives an actual buffer line index (or computes one internally via `buffer_line_for_screen_row`), never a raw screen row number masquerading as a buffer line.

- **Status**: satisfied
- **Evidence**: The `first_visible_line` parameter was removed entirely from `ensure_visible_wrapped` (viewport.rs). The function now always iterates from buffer line 0 to compute absolute screen row position. Call sites in editor_state.rs:4341 and context.rs:156 no longer pass `first_visible_line()`. The function uses `first_visible_screen_row()` internally to determine viewport position, which correctly returns a screen row from `scroll_offset_px`.

### Criterion 2: After any cursor movement in a wrapped file, `ensure_cursor_visible_in_active_tab` scrolls the viewport so the cursor is visible and at the correct position.

- **Status**: satisfied
- **Evidence**: Both call sites (editor_state.rs and context.rs) now call `ensure_visible_wrapped` without the erroneous `first_visible_line` parameter. The function computes `cursor_abs_screen_row` from line 0, then compares against `current_top_screen_row` to decide scroll direction. The scroll-up and scroll-down branches both compute correct targets.

### Criterion 3: Scrolling to a cursor that is on a continuation row (second or later screen row of a wrapped buffer line) places the viewport correctly.

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_cursor_on_continuation_row` test places cursor at col 150 of a 250-char line (second screen row), scrolls viewport past it, and verifies the viewport scrolls to `1.0 * 16.0` (screen row 1). The `cursor_row_offset` from `buffer_col_to_screen_pos` is correctly added to `cursor_abs_screen_row`.

### Criterion 4: Scrolling to a cursor with many wrapped lines above it does not overshoot.

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_cursor_below_viewport_with_wrapped_lines_above` sets up lines 0-2 each wrapping to 3 screen rows. Cursor at buffer line 4 has absolute screen row 10. The test verifies correct behavior: no scroll when cursor is on the partial row (visible_range semantics), scroll triggered when cursor moves to screen row 11. The always-from-0 iteration eliminates the under-counting that caused overshoot.

### Criterion 5: No regression to non-wrapped scrolling behavior.

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_non_wrapped_document_regression` tests with 20 short lines (50 chars, all fit in 100 cols). Verifies: cursor at line 5 visible (no scroll), cursor at line 15 triggers scroll down, cursor at line 0 scrolls back to top. The existing `test_ensure_visible_wrapped_partial_row_should_not_scroll` test also passes unchanged (just removed the `first_visible_line` arg).

### Criterion 6: Tests covering all specified scenarios

- **Status**: satisfied
- **Evidence**: Four new tests plus the updated existing test all pass (verified via `cargo test`):

### Criterion 7: Cursor below viewport with wrapped lines above: viewport scrolls to correct position.

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_cursor_below_viewport_with_wrapped_lines_above` - passes.

### Criterion 8: Cursor above viewport with wrapped lines below: viewport scrolls to correct position.

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_cursor_above_viewport_with_wrapped_lines` - passes.

### Criterion 9: Cursor on a continuation row of a wrapped line.

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_cursor_on_continuation_row` - passes. Verifies exact scroll target (screen row 1 * line_height).

### Criterion 10: Cursor in a non-wrapped document (existing behavior preserved).

- **Status**: satisfied
- **Evidence**: `test_wrap_scroll_non_wrapped_document_regression` - passes. Tests scroll down, scroll up, and no-scroll cases.
