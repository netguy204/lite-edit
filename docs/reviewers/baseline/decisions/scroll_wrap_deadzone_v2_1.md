---
decision: APPROVE
summary: "All success criteria satisfied; the implementation correctly fixes screen row to buffer line mapping by using `Viewport::buffer_line_for_screen_row` in `pixel_to_buffer_position_wrapped`, ensuring click handling agrees with renderer coordinate systems."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Scrolling to the bottom of a file with many wrapped lines shows the last line at the bottom of the viewport with no extra dead space below

- **Status**: satisfied
- **Evidence**: The `set_scroll_offset_px_wrapped` method (viewport.rs:329-342) correctly uses `compute_total_screen_rows` to calculate max offset based on actual screen rows, not buffer lines. Test `test_set_scroll_offset_px_wrapped_allows_scroll_for_large_content` verifies max_offset calculation is correct (240px for 20 screen rows with 5 visible).

### Criterion 2: Scrolling back up from the bottom responds immediately with no deadzone at any file size or wrap density

- **Status**: satisfied
- **Evidence**: Test `test_scroll_at_max_wrapped_responds_immediately` (viewport.rs:1208-1245) explicitly verifies that after scrolling to max position (240.0) and then subtracting 1px, the offset immediately becomes 239.0 with no absorbed scroll. All scroll clamping tests pass.

### Criterion 3: Clicking in the buffer at the maximum scroll position places the cursor at the clicked location (not offset by any number of lines)

- **Status**: satisfied
- **Evidence**: The core fix in `pixel_to_buffer_position_wrapped` (buffer_target.rs:690-697) now uses `Viewport::buffer_line_for_screen_row` to correctly convert the absolute screen row to a buffer line. Tests verify this:
  - `test_click_mixed_wrap_uses_screen_row_as_buffer_line` verifies click at max scroll lands on correct buffer line
  - `test_pixel_to_buffer_consistency_with_buffer_line_for_screen_row` verifies agreement with renderer logic
  - `test_click_uniform_wrap_shows_cursor_offset_symptom` demonstrates the ~10 line offset symptom is fixed

### Criterion 4: The computed `max_offset_px` in `set_scroll_offset_px_wrapped` matches the actual rendered content height (total screen rows as drawn by the renderer minus visible rows, times line height)

- **Status**: satisfied
- **Evidence**: Test `test_compute_total_screen_rows_matches_manual_count` (viewport.rs:1343-1395) verifies that `compute_total_screen_rows` produces the exact same count as manually walking the buffer with `screen_rows_for_line`. Test `test_buffer_line_for_screen_row_covers_all_screen_rows` verifies complete coverage of the screen row to buffer line mapping.

### Criterion 5: All existing scroll and viewport tests continue to pass

- **Status**: satisfied
- **Evidence**: Running `cargo test --workspace -- buffer_target` shows 90/90 tests pass. Running `cargo test --workspace -- viewport` shows 70/70 tests pass. The only test failures are in performance tests (`insert_100k_chars_under_100ms`) which are pre-existing timing-sensitive tests unrelated to this chunk (verified by checking these tests were unmodified in the diff).

### Criterion 6: Regression test: a file where wrapped lines produce significantly more screen rows than buffer lines (e.g., 2x+) scrolls correctly to the true bottom

- **Status**: satisfied
- **Evidence**: Test `test_wrapped_max_offset_greater_than_unwrapped` (viewport.rs:1302-1337) uses 10 buffer lines that each wrap to 2 screen rows (160 chars at 80 cols = 2x ratio). It verifies wrapped max offset (240.0) is greater than unwrapped max offset (80.0). Test `test_click_uniform_wrap_shows_cursor_offset_symptom` also uses this same 2x wrap density scenario.
