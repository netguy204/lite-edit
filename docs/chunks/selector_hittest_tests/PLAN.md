<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds comprehensive tests to verify that the selector's hit-testing
(click → item selection) is correct across the full space of scroll positions,
item heights, and list geometries. The prior chunks (`selector_coord_flip`,
`selector_row_scroller`, `selector_smooth_render`) implemented the fixes; this
chunk proves they work and guards against regression.

**Testing strategy:**

1. **Parameterised property tests** — Verify the core invariant: clicking the
   pixel centre of any rendered row selects exactly that row. We parameterise
   over:
   - Multiple `scroll_offset_px` values (including non-zero fractional parts)
   - A range of `item_height` values
   - Clicking first, middle, and last visible items

2. **Regression tests for original bugs** — Explicit tests that would have
   failed before the fixes:
   - **Coordinate-flip bug**: A click near the top of the overlay (large raw y,
     small flipped y) selects the topmost visible item, not an out-of-bounds
     index.
   - **Scroll-rounding bug**: Accumulating small scroll deltas (each < item_height)
     preserves the fractional parts without rounding loss.

3. **Boundary condition tests** — Edge cases where off-by-one errors are most
   likely:
   - Clicking exactly on a row boundary selects that row, not the previous one.
   - Clicking when `scroll_fraction_px == 0` (whole-row alignment) works.
   - Clicking below the last rendered item is a no-op.
   - Clicking above `list_origin_y` is a no-op.

Following `docs/trunk/TESTING_PHILOSOPHY.md`, all tests are pure unit tests
against `SelectorWidget` — no Metal, no macOS, no mocking. The tests exercise
the actual `handle_mouse` method with varying geometry parameters, asserting on
`selected_index()` after each event.

Location: `crates/editor/src/selector.rs`, inside the existing `#[cfg(test)]`
module.

## Sequence

### Step 1: Add parameterised centre-click property test

Create a test function `click_row_centre_selects_that_row` that verifies:
for each combination of:
- `scroll_offset_px` ∈ {0.0, 8.5, 17.2} (three fractional values)
- `item_height` ∈ {16.0, 20.0} (two common heights)
- `clicked_visible_row` ∈ {0, visible_rows/2, visible_rows-1}

compute the pixel centre of the rendered row:
```rust
let y = list_origin_y - scroll_fraction_px + clicked_visible_row * item_height + item_height / 2.0;
```

Call `handle_mouse((x, y), MouseEventKind::Down, item_height, list_origin_y)` and
assert `selected_index() == first_visible_item() + clicked_visible_row`.

This test parameterises using nested loops (not a separate test framework), keeping
dependencies minimal.

Location: `crates/editor/src/selector.rs` → `mod tests`

### Step 2: Add coordinate-flip regression test

Create a test function `coordinate_flip_regression_raw_y_near_top_selects_topmost`
that simulates a click on the selector list where the raw macOS y-coordinate is
near `view_height` (i.e., the top of the screen):

1. Set up a widget with 20 items, visible_size=80px, item_height=16px.
2. Scroll to offset 0 so first_visible_item=0.
3. The renderer places item 0 at y=`list_origin_y` (e.g., 100.0).
4. To click on item 0's centre, the flipped y is `list_origin_y + item_height/2`.
   If we were receiving raw macOS y, the correct raw y would be:
   `raw_y = view_height - flipped_y`
5. But `handle_mouse` expects already-flipped coordinates (done by
   `handle_mouse_selector`). So we pass `(x, list_origin_y + item_height/2.0)`
   and assert selected_index=0.

This test documents the coordinate system convention and guards against
re-introducing the flip bug.

Location: `crates/editor/src/selector.rs` → `mod tests`

### Step 3: Add scroll-rounding regression test

Create a test function `scroll_rounding_regression_sub_row_deltas_accumulate`
that verifies fractional scroll deltas accumulate without rounding:

1. Set up a widget with 20 items, item_height=16px (from default FontMetrics).
2. Apply 10 scroll deltas of `0.4 * item_height = 6.4` pixels each.
3. Assert total `scroll_offset_px == 64.0` (exactly 4 rows).
4. Assert `first_visible_item() == 4`.
5. Assert `scroll_fraction_px() == 0.0` (64.0 mod 16.0 = 0).

This would have failed before `selector_row_scroller` because each delta would
have been rounded to the nearest integer row (0), producing zero net scroll.

Location: `crates/editor/src/selector.rs` → `mod tests`

### Step 4: Add boundary condition tests

Create three boundary tests:

1. **`click_on_row_boundary_top_pixel_selects_that_row`** —
   Click at `y = list_origin_y - scroll_fraction_px + row * item_height` (the
   exact top pixel of a row). Assert it selects `first_visible_item() + row`.

2. **`click_when_scroll_fraction_is_zero`** —
   Set scroll_offset_px to an exact multiple of item_height (e.g., 32.0).
   Assert `scroll_fraction_px() == 0.0`. Click the centre of row 0 and assert
   selection is correct.

3. **`click_below_last_rendered_item_is_noop`** —
   With 10 items and 5 visible, click at y beyond the last item's bottom edge.
   Assert `selected_index()` is unchanged.

4. **`click_above_list_origin_is_noop`** —
   Click at y < list_origin_y. Assert `selected_index()` is unchanged.

Location: `crates/editor/src/selector.rs` → `mod tests`

### Step 5: Verify all tests pass and no new suppressions

Run `cargo test -p editor` and confirm:
- All new tests pass.
- All existing selector tests continue to pass.
- No new `#[allow(...)]` suppressions were added.

## Dependencies

- **selector_row_scroller** (ACTIVE) — Provides `RowScroller` integration in
  `SelectorWidget`, including `scroll_fraction_px()` and `first_visible_item()`.
- **selector_smooth_render** (ACTIVE) — Ensures the renderer's placement formula
  matches the hit-testing formula (both use `list_y - scroll_fraction_px`).
- **selector_coord_flip** (HISTORICAL) — The Y-coordinate flip fix in
  `handle_mouse_selector` that this chunk's regression test guards.

## Risks and Open Questions

- **Test coverage vs. execution time** — The parameterised test uses nested loops
  rather than a property-testing framework like `proptest`. This keeps
  dependencies minimal but limits exploration. The chosen parameter values
  (3 scroll offsets × 2 item heights × 3 row positions = 18 cases) cover the
  interesting boundaries without excessive runtime.

- **Floating-point precision** — The tests use `f32` arithmetic for scroll
  offsets and positions. Assertions should use approximate equality where
  exact match isn't expected (e.g., `(actual - expected).abs() < 0.001`).

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->