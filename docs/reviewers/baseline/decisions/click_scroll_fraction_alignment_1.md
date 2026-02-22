---
decision: APPROVE
summary: All success criteria satisfied; scroll_fraction_px correctly threaded through both hit-test functions with comprehensive tests.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Clicking on a line when the viewport has a non-zero `scroll_fraction_px` positions the cursor on the visually-clicked line, not an offset line.

- **Status**: satisfied
- **Evidence**: Both `pixel_to_buffer_position` (line 588-589) and `pixel_to_buffer_position_wrapped` (line 660-661) now add `scroll_fraction_px` to `flipped_y` before dividing by `line_height`. This compensates for the renderer's vertical translation. The regression test `test_click_with_scroll_fraction_positions_correctly` validates this behavior by scrolling to line 5 + 8 pixels and verifying a click at flipped_y=10 correctly selects line 6.

### Criterion 2: The fix is applied to both `pixel_to_buffer_position_wrapped` and the legacy non-wrapped `pixel_to_buffer_position`.

- **Status**: satisfied
- **Evidence**: Both functions now have the `scroll_fraction_px: f32` parameter added (line 568 for `pixel_to_buffer_position`, line 642 for `pixel_to_buffer_position_wrapped`), and both apply the compensation formula `(flipped_y + scroll_fraction_px as f64) / line_height`.

### Criterion 3: `scroll_fraction_px` is threaded through the call site so the click handler has access to it.

- **Status**: satisfied
- **Evidence**: Both call sites in `handle_mouse` (lines 476 and 506) now pass `ctx.viewport.scroll_fraction_px()` to the hit-test functions. The viewport API already exposes this method, so no changes to `EditorContext` were needed.

### Criterion 4: A regression test is added in `buffer_target.rs`: simulate a scroll to a fractional position (`scroll_fraction_px > 0`), then verify that a click at the middle of a visually-rendered line maps to the correct buffer line, not an adjacent one.

- **Status**: satisfied
- **Evidence**: `test_click_with_scroll_fraction_positions_correctly` (lines 1376-1442) creates a 20-line buffer, scrolls to line 5 + 8 pixels fractional offset, then simulates a click and asserts the cursor lands on line 6 (the correct visual target). Additionally, `test_pixel_to_position_with_scroll_fraction` (lines 1445-1461) directly unit-tests the non-wrapped function with a non-zero scroll fraction.

### Criterion 5: Existing click-positioning tests continue to pass.

- **Status**: satisfied
- **Evidence**: All existing `pixel_to_` tests were updated to pass `0.0` for the new `scroll_fraction_px` parameter. Running `cargo test -p lite-edit pixel_to` shows all 10 tests pass. The full test suite shows 550 tests passing, with one unrelated failure in `editor_state::tests::test_new_tab_viewport_is_sized` (pre-existing, not modified by this chunk).

### Criterion 6: After the fix, clicking anywhere in the file (top, middle, bottom) reliably places the cursor on the intended line regardless of the current `scroll_fraction_px`.

- **Status**: satisfied
- **Evidence**: The fix is mathematical in nature - by adding `scroll_fraction_px` to the flipped Y coordinate before dividing by line height, the calculation correctly accounts for the renderer's visual offset at any scroll position. The test cases cover the scenario where the difference matters (clicking where the fractional offset would cause an off-by-one error). The existing tests with `scroll_fraction_px = 0.0` verify the fix doesn't regress normal clicking behavior.
