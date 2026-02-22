---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/selector.rs
- crates/editor/src/selector_overlay.rs
- crates/editor/src/row_scroller.rs
- crates/editor/src/editor_state.rs
code_references:
- ref: crates/editor/src/editor_state.rs
  implements: "set_item_height calls before update_visible_size to sync RowScroller row_height"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- file_search_path_matching
---

# Chunk Goal

## Minor Goal

The file picker (selector overlay) cannot scroll to the bottom of a long match
list. When searching for a common term (e.g. "GOAL" in a project with many
matching files), the user can arrow-key or scroll down but the viewport stops
well before the end of the list — there are still >3 selectable items below the
lowest visible item. Pressing Down continues to move the selection onto items
that are never rendered on screen.

This is a continuation of the work in `selector_scroll_bottom`, which fixed two
related bugs (panel showing only one item on open, and selection clipping at
the bottom edge). The current bug is that the scroll range itself is too short —
the maximum scroll offset does not allow the viewport to reach the true bottom
of the item list.

**Likely root causes to investigate:**

1. **`visible_items` vs total item count in geometry**: `calculate_overlay_geometry`
   computes `visible_items = item_count.min(max_visible_items)`. The height passed
   to `update_visible_size` is `visible_items * item_height`. If `visible_items`
   is being used somewhere as the total item count rather than just the viewport
   capacity, the `RowScroller` would compute a `max_offset_px` that's too small.

2. **Panel sizing shrinks the scroll range**: The overlay panel height is sized to
   `visible_items` rows (capped by `max_visible_items`). If `item_count` is used
   as the `visible_items` parameter somewhere during geometry calculation for
   scroll clamping, the scroller might think all items fit in the panel.

3. **`ensure_visible` or `set_scroll_offset_px` clamping mismatch**: The
   `RowScroller::set_scroll_offset_px` clamps to
   `(row_count - visible_rows) * row_height`. If `row_count` is being passed
   incorrectly (e.g., as `visible_items` instead of total items), the max scroll
   would be too small.

The implementer should add diagnostic logging or write a targeted test that
creates a selector with many more items than `max_visible_items`, scrolls to the
bottom, and verifies the last item is reachable and renderable within the
scissor-clipped list area.

## Success Criteria

- In a file picker with more items than fit in the panel, arrow-keying Down
  from the first item to the last item keeps the selection highlight visible
  on every frame, all the way to the final item.
- Mouse wheel scrolling can reach the bottom of the list — the last item is
  fully visible when scrolled to the end.
- The number of selectable-but-invisible items at the bottom is zero.
- New or updated unit tests in `selector.rs` verify:
  - A list with 2× the panel capacity can be scrolled so the last item is
    in `visible_item_range()`.
  - `ensure_visible(last_item_index, total_items)` places the scroll offset
    such that `last_item_index` is within the rendered visible range.
- No regressions in existing selector or overlay tests.

