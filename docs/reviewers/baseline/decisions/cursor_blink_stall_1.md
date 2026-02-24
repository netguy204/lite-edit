---
decision: APPROVE
summary: All success criteria satisfied; root cause identified, fix implemented in two locations with defense-in-depth, and comprehensive regression tests added.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: The cursor blinks reliably in the active pane under all normal editing conditions (no unexplained stalls)

- **Status**: satisfied
- **Evidence**: The fix adds a guard in `dirty_lines_to_region_wrapped()` (viewport.rs:499-508) that returns `DirtyRegion::FullViewport` when `visible_lines() == 0`, ensuring a repaint is triggered even in the degenerate viewport state. Additionally, defense-in-depth is provided in `cursor_dirty_region()` (editor_state.rs:2629-2631). The same guard was also added to `dirty_lines_to_region()` (viewport.rs:614-619) for consistency. All editor tests pass.

### Criterion 2: Identify and fix the specific condition that causes `cursor_dirty_region()` to return `None` / a no-op dirty region when the cursor is actually visible

- **Status**: satisfied
- **Evidence**: The PLAN.md clearly identifies the root cause: when `visible_lines() == 0`, the check `line_start_screen_row >= visible_end_screen_row` becomes true for ALL positions because `visible_end_screen_row == first_visible_screen_row` (both 0). This incorrectly classifies visible cursor positions as "below viewport" and returns `None`. The fix at viewport.rs:506-508 guards against this by returning `FullViewport` when `visible_lines() == 0`. The code comment at viewport.rs:499-505 documents this exact reasoning.

### Criterion 3: Add a test that reproduces the stale-viewport condition (if feasible) and verifies the cursor dirty region is non-empty when the cursor is on-screen

- **Status**: satisfied
- **Evidence**: Three new tests were added:
  1. `test_dirty_lines_to_region_wrapped_zero_visible_lines` (viewport.rs:2473-2511) - Tests the wrapped variant, creating a viewport without calling `update_size()` and asserting `FullViewport` is returned.
  2. `test_dirty_lines_to_region_zero_visible_lines` (viewport.rs:2513-2527) - Tests the non-wrapped variant with the same setup.
  3. `test_toggle_cursor_blink_uninitialized_viewport_returns_dirty` (editor_state.rs:3476-3504) - Integration test verifying that `toggle_cursor_blink()` returns a dirty region even with an uninitialized viewport.

## Subsystem Alignment

The implementation correctly follows the viewport_scroll subsystem's invariant #4: "DirtyRegion::merge is associative and commutative with None as identity. **FullViewport absorbs everything.**" Using `FullViewport` as a fallback for the degenerate case is consistent with this pattern. The change is minimal and scoped appropriately.

## Code Quality

- Clear backreference comments link to this chunk
- Test assertions include descriptive messages
- Defense-in-depth pattern guards at multiple layers
- Changes are minimal and focused on the bug fix
