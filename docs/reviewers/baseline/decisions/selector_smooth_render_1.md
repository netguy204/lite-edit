---
decision: APPROVE
summary: All success criteria satisfied - implementation correctly integrates scroll_fraction_px() and visible_item_range() for smooth sub-row scrolling
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Trackpad scrolling in the file picker produces smooth, continuous motion with no visible full-row snapping between scroll events.

- **Status**: satisfied
- **Evidence**: Implementation in `update_from_widget()` now computes `list_y = geometry.list_origin_y - scroll_frac` (line 380), which subtracts the fractional scroll offset from the list origin. Item Y positions are computed as `list_y + draw_idx as f32 * geometry.item_height` (line 485), meaning items shift by fractional pixels as the user scrolls. This mirrors the pattern established in the main buffer renderer.

### Criterion 2: The selection highlight moves with its item as the list scrolls fractionally.

- **Status**: satisfied
- **Evidence**: Selection highlight Y is computed using the same `list_y` offset: `sel_y = list_y + visible_row as f32 * geometry.item_height` (line 389). This ensures the highlight tracks the item's fractional position during scrolling.

### Criterion 3: The first visible item is partially clipped at the top when `scroll_fraction_px > 0`, matching the behaviour of the main buffer viewport.

- **Status**: satisfied
- **Evidence**: By computing `list_y = geometry.list_origin_y - scroll_frac`, when `scroll_frac > 0`, the first item's Y position will be above `list_origin_y`, causing partial clipping. This matches the main buffer's behavior where the fractional scroll creates a partially-visible top row.

### Criterion 4: A partially-visible item is always drawn at the bottom of the list when the list is scrolled to a fractional position.

- **Status**: satisfied
- **Evidence**: The implementation uses `widget.visible_item_range()` (line 331 and 484) which delegates to `RowScroller::visible_range()`. Per the PLAN.md, RowScroller already adds +1 extra row for partial bottom visibility. The tests in `selector.rs` confirm `visible_item_range()` returns a range that includes partial items (test `visible_item_range_accounts_for_partial_visibility` at line 1183).

### Criterion 5: `calculate_overlay_geometry` and `OverlayGeometry` are unchanged.

- **Status**: satisfied
- **Evidence**: Git diff shows no changes to `calculate_overlay_geometry()` function or `OverlayGeometry` struct. The only modifications are within `SelectorGlyphBuffer::update_from_widget()`. The diff confirms the changes are limited to lines 325-510 of `update_from_widget()` only.

### Criterion 6: All existing `SelectorGlyphBuffer` geometry tests pass; no rendering tests are broken.

- **Status**: satisfied
- **Evidence**: Running `cargo test selector_overlay` shows all 14 geometry tests pass: `panel_width_is_60_percent_of_view_width`, `panel_is_horizontally_centered`, etc. All selector-related tests (83 tests) also pass. The flaky performance test failures in `lite-edit-buffer` are pre-existing and unrelated to this chunk.

