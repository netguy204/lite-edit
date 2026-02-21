---
decision: APPROVE
summary: All success criteria satisfied - viewport now tracks pixel-accurate scroll position with fractional rendering support
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `Viewport` gains a pixel-accurate scroll representation. The `scroll_offset` field is replaced (or augmented) by a `scroll_offset_px: f32` that accumulates raw pixel deltas without rounding.

- **Status**: satisfied
- **Evidence**: `viewport.rs:31` - The `Viewport` struct now has `scroll_offset_px: f32` (private field) replacing the integer `scroll_offset: usize`. The field is documented as "Scroll position in pixels" and is manipulated via `set_scroll_offset_px()` which accepts raw pixel values without rounding.

### Criterion 2: The integer line index (`first_visible_line`) is derived as `(scroll_offset_px / line_height).floor() as usize`, clamped to valid bounds, and available for buffer-to-screen mapping.

- **Status**: satisfied
- **Evidence**: `viewport.rs:65-70` - The `first_visible_line()` method computes `(self.scroll_offset_px / self.line_height).floor() as usize`. This is used throughout for buffer-to-screen mapping in `visible_range()`, `buffer_line_to_screen_line()`, `screen_line_to_buffer_line()`, and `dirty_lines_to_region()`.

### Criterion 3: The fractional pixel remainder (`scroll_offset_px % line_height`) is exposed so the renderer can apply it as a Y translation when drawing glyphs, causing the top line to be partially clipped and the bottom line to appear partially on-screen — identical to how any full-featured text editor renders mid-line scroll positions.

- **Status**: satisfied
- **Evidence**: `viewport.rs:80-85` - The `scroll_fraction_px()` method computes `self.scroll_offset_px % self.line_height`. The renderer passes this to the glyph buffer at `renderer.rs:246-253` via `viewport.scroll_fraction_px()` as the `y_offset` parameter.

### Criterion 4: Clamping works correctly in pixel space: `scroll_offset_px` is bounded between `0.0` and `(buffer_line_count - visible_lines) * line_height`, preventing scrolling past the start or end of the document.

- **Status**: satisfied
- **Evidence**: `viewport.rs:101-105` - The `set_scroll_offset_px()` method computes `max_offset_px = max_lines as f32 * self.line_height` where `max_lines = buffer_line_count.saturating_sub(self.visible_lines)`, then clamps via `px.clamp(0.0, max_offset_px)`. Tests `test_clamping_at_start` and `test_clamping_at_end` verify this behavior.

### Criterion 5: `ensure_visible` is updated to operate in pixel space and snap to the nearest whole-line boundary that brings the target line into view (i.e., it snaps to a pixel offset that is a multiple of `line_height`).

- **Status**: satisfied
- **Evidence**: `viewport.rs:153-172` - The `ensure_visible()` method computes `target_px = line as f32 * self.line_height` when scrolling is needed, ensuring the offset is always a multiple of `line_height`. The test `test_ensure_visible_snaps_to_whole_line` at line 539-552 verifies that after calling `ensure_visible()`, `scroll_fraction_px()` returns 0.0.

### Criterion 6: All existing viewport tests continue to pass. New tests cover: (1) sub-line deltas accumulate correctly without triggering a line change, (2) deltas that cross a line boundary advance `first_visible_line` by exactly one, (3) the fractional remainder is correct after several accumulated deltas, (4) clamping at both ends works in pixel space.

- **Status**: satisfied
- **Evidence**: `cargo test viewport` shows 44 tests passing. Specific new tests:
  - `test_sub_line_delta_accumulates_without_line_change` (viewport.rs:326-339)
  - `test_crossing_line_boundary_advances_first_visible_line` (viewport.rs:342-360)
  - `test_fractional_remainder_correct_after_accumulated_deltas` (viewport.rs:363-375)
  - `test_clamping_at_start` and `test_clamping_at_end` (viewport.rs:378-397)
  - `test_small_scroll_delta_accumulates` and `test_accumulated_scroll_crosses_line_boundary` in buffer_target.rs

### Criterion 7: The renderer uses the fractional remainder to offset all drawn lines by `-remainder_px` in Y, so that content scrolls smoothly between line positions.

- **Status**: satisfied
- **Evidence**:
  - `renderer.rs:241-256` - `update_glyph_buffer()` passes `viewport.scroll_fraction_px()` as `y_offset`
  - `glyph_buffer.rs:102-106` - `position_for_with_offset()` computes `y = row as f32 * self.line_height - y_offset`
  - `glyph_buffer.rs:127-151` - `quad_vertices_with_offset()` uses this offset for all quad positioning
  - All quad creation methods (selection, cursor, glyphs) now use `_with_offset` variants

## Notes

- The performance test failures (`insert_100k_chars_under_100ms`, `insert_100k_chars_with_newlines_under_200ms`) are pre-existing and unrelated to this chunk - they are flaky timing tests in the buffer crate.
- A backwards compatibility alias `scroll_offset()` is provided at `viewport.rs:280-283` that returns `first_visible_line()` for any code that may still reference the old field.
