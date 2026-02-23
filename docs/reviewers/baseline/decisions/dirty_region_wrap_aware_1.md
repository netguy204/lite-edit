---
decision: APPROVE
summary: All success criteria satisfied with wrap-aware dirty region conversion properly implemented in viewport.rs, context.rs, and editor_state.rs with comprehensive tests.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `dirty_lines_to_region()` converts buffer line indices to screen row indices (using `WrapLayout` or `buffer_line_for_screen_row`) before comparing against the viewport's screen-row-based scroll position.

- **Status**: satisfied
- **Evidence**: The new `dirty_lines_to_region_wrapped()` method at viewport.rs:478 converts buffer line indices to absolute screen row indices using the helper `buffer_line_to_abs_screen_row()` (viewport.rs:453). This helper computes cumulative screen rows by iterating over preceding buffer lines and summing `screen_rows_for_line()` from WrapLayout. The method then compares these screen row indices against `first_visible_screen_row()` and `visible_end_screen_row` correctly.

### Criterion 2: Clicking at any scroll position in a file with heavy line wrapping (e.g., OVERVIEW.md with 843-char lines) immediately repaints the cursor at the clicked position without requiring a subsequent scroll.

- **Status**: satisfied
- **Evidence**: `EditorContext::mark_dirty()` (context.rs:86-102) now calls `dirty_lines_to_region_wrapped()` with the wrap layout and line length function. The `mark_cursor_dirty()` method (context.rs:136-139) delegates to `mark_dirty()`. This ensures that cursor repositioning via mouse click will correctly compute dirty regions even when buffer line indices are much smaller than screen row indices at deep scroll positions.

### Criterion 3: The `cursor_dirty_region()` method in `editor_state.rs` also produces correct dirty regions under wrapping.

- **Status**: satisfied
- **Evidence**: The `cursor_dirty_region()` method at editor_state.rs:1782-1805 creates a WrapLayout from the current viewport width and font metrics, captures line lengths, and calls `dirty_lines_to_region_wrapped()` on the viewport. The chunk backreference is present (line 1776).

### Criterion 4: Existing `dirty_lines_to_region` unit tests continue to pass (they test the no-wrap case where screen rows = buffer lines).

- **Status**: satisfied
- **Evidence**: The original `dirty_lines_to_region()` method is preserved at viewport.rs:587-643 and remains unchanged. Tests verified by running `cargo test --all -- dirty` which showed all 36 dirty-related tests passing, including the original viewport tests (test_dirty_single_visible, test_dirty_range_fully_visible, test_dirty_from_line_to_end_*, etc.).

### Criterion 5: New unit tests verify correct dirty region computation when buffer lines wrap to multiple screen rows, specifically testing that a buffer line visible on screen but with a buffer index < `first_visible_screen_row` still produces a non-None dirty region.

- **Status**: satisfied
- **Evidence**: Comprehensive wrap-aware tests were added at viewport.rs:1922-2251:
  - `test_dirty_single_visible_wrapped`: Buffer line 5 at screen row 10 is visible when scrolled to row 10
  - `test_dirty_single_not_visible_wrapped`: Buffer lines above/below viewport correctly return None
  - `test_dirty_range_wrapped`: Range of buffer lines correctly maps to screen rows
  - `test_dirty_from_line_to_end_wrapped`: FromLineToEnd produces correct FullViewport/None
  - `test_dirty_wrapped_heavy_divergence`: Reproduces the original bug scenario (buffer line 40 << screen row 200)
  - `test_dirty_wrapped_partial_line_visible`: Buffer line partially overlapping viewport produces non-None region
