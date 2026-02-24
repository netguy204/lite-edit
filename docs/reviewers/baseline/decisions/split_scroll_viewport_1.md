---
decision: APPROVE
summary: All success criteria satisfied with thorough tests; implementation follows viewport_scroll subsystem invariants and integrates cleanly with existing pane layout code.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: After a horizontal split, each resulting pane's tab reports `visible_lines` consistent with its actual content height (pane height minus tab-bar height, divided by line height).

- **Status**: satisfied
- **Evidence**:
  - `sync_pane_viewports()` in `editor_state.rs:501-556` calculates pane rects via `calculate_pane_rects()` and computes `pane_content_height = pane_rect.height - TAB_BAR_HEIGHT` for each pane
  - Each tab's viewport is updated via `tab.viewport.update_size(pane_content_height, line_count)` which correctly derives `visible_lines = floor(height / line_height)`
  - Test `test_vsplit_reduces_visible_lines` validates this behavior: creates 50% vertical split, asserts top and bottom panes both have `visible_lines = floor((300-32)/line_height)` = 13 lines

### Criterion 2: A tab whose line count exceeds the post-split visible-line count is scrollable — scroll input moves the viewport and all content is reachable.

- **Status**: satisfied
- **Evidence**:
  - Test `test_tab_becomes_scrollable_after_split` creates a tab with 50 lines in a split pane with ~13 visible lines
  - Verifies `line_count > visible_lines` (50 > 13)
  - Verifies scrolling works: `viewport.scroll_to(10, line_count)` succeeds, scrolling to max position succeeds
  - The scroll clamping logic in `RowScroller::set_scroll_offset_px` ensures valid bounds

### Criterion 3: A tab that was already scrolled before a split clamps its scroll offset to the new maximum without jumping or leaving blank space.

- **Status**: satisfied
- **Evidence**:
  - `sync_pane_viewports()` calls `update_size()` which internally calls `set_scroll_offset_px(self.scroll_offset_px, row_count)` to re-clamp (per viewport_scroll subsystem Invariant #7)
  - Test `test_scroll_clamped_on_shrink` scrolls to line 30, then shrinks window significantly - scroll stays at 30 (within valid bounds)
  - Test `test_scroll_clamping_on_extreme_resize` in viewport.rs tests extreme cases where scroll must be clamped down
  - Test `test_viewport_at_bottom_becomes_scrollable_after_resize` verifies "at bottom" tabs become scrollable after resize

### Criterion 4: No regression in single-pane visible-line calculation or scroll behavior.

- **Status**: satisfied
- **Evidence**:
  - `sync_active_tab_viewport()` remains intact and is still called throughout the codebase for single-tab operations
  - `sync_pane_viewports()` is a complementary method that handles multi-pane scenarios
  - All existing viewport tests pass (21 tests in viewport::tests module)
  - All split-related tests pass (19 tests mentioning "split")
  - Pre-existing performance test failures in buffer crate are unrelated to this change (verified by running tests with and without changes)

## Subsystem Compliance

The implementation correctly follows **viewport_scroll** subsystem invariants:
- **Invariant #1**: `scroll_offset_px` remains the single source of truth
- **Invariant #7**: `update_size()` re-clamps scroll offset as documented in `RowScroller::update_size()` (line 128-136)
- Uses `update_size()` API from the subsystem rather than manually manipulating scroll state
