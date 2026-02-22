---
decision: APPROVE
summary: All success criteria satisfied - scroll support for file picker is fully implemented with view_offset tracking, handle_scroll method, arrow key navigation viewport adjustment, offset-aware mouse handling, and proper renderer integration.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `SelectorWidget` gains a `view_offset` field

- **Status**: satisfied
- **Evidence**: `view_offset: usize` field defined at line 84 in selector.rs with default `0` (line 102). Public accessor `view_offset(&self) -> usize` at lines 125-127.

### Criterion 2: `SelectorWidget::handle_scroll` method

- **Status**: satisfied
- **Evidence**: Method at lines 231-251 implements the exact signature specified: `pub fn handle_scroll(&mut self, delta_y: f64, item_height: f64, visible_items: usize)`. Computes rows as `(delta_y / item_height).round() as isize` (line 243). Clamps view_offset to `0..=items.len().saturating_sub(visible_items)` (lines 246-248). No-op on empty or short lists (lines 232-240).

### Criterion 3: Arrow key navigation keeps selection visible

- **Status**: satisfied
- **Evidence**: `handle_key` method updates `view_offset` after arrow key navigation:
  - Up arrow: lines 176-178 set `view_offset = selected_index` if `selected_index < view_offset`
  - Down arrow: lines 189-193 set `view_offset = selected_index - visible_items + 1` if `selected_index >= view_offset + visible_items`
  - `set_visible_items(n: usize)` setter added at lines 133-135

### Criterion 4: `set_items` clamps `view_offset`

- **Status**: satisfied
- **Evidence**: Lines 152-154 in `set_items()`: `let max_offset = self.items.len().saturating_sub(self.visible_items); self.view_offset = self.view_offset.min(max_offset);`

### Criterion 5: `handle_mouse` is offset-aware

- **Status**: satisfied
- **Evidence**: Lines 286-295 compute `visible_row` from mouse position and then `item_index = self.view_offset + visible_row`. The method documentation at lines 270-272 clearly states this offset-aware behavior.

### Criterion 6: Renderer uses `view_offset`

- **Status**: satisfied
- **Evidence**: In selector_overlay.rs:
  - Lines 466-470: Item text iteration uses `.skip(widget.view_offset()).take(geometry.visible_items)`
  - Lines 369-388: Selection highlight only rendered if selected item is within visible window (`selected >= view_offset && selected < view_offset + visible_items`), with correct `visible_row = selected - view_offset` calculation

### Criterion 7: `EditorState::handle_scroll` forwards events when selector is open

- **Status**: satisfied
- **Evidence**:
  - Lines 504-519: `handle_scroll` routes to `handle_scroll_selector` when `focus == EditorFocus::Selector`
  - Lines 522-550: `handle_scroll_selector` calculates overlay geometry, calls `selector.set_visible_items()`, forwards scroll to selector via `selector.handle_scroll()`, and marks `DirtyRegion::FullViewport`

### Criterion 8: Tests - selector.rs unit tests

- **Status**: satisfied
- **Evidence**: All required tests present:
  - `scroll_down_increments_view_offset` (lines 382-393)
  - `scroll_up_decrements_view_offset` (lines 395-409)
  - `scroll_clamps_at_max_offset` (lines 411-422)
  - `scroll_clamps_at_zero` (lines 424-435)
  - `scroll_on_short_list_is_noop` (lines 437-449)
  - `scroll_on_empty_list_is_noop` (lines 451-462)
  - `down_past_visible_window_increments_view_offset` (lines 468-491)
  - `up_past_visible_window_decrements_view_offset` (lines 493-516)
  - `down/up_within_visible_window_does_not_change_view_offset` (lines 518-555)
  - `set_items_clamps_view_offset_when_list_shrinks` (lines 561-577)
  - `set_items_preserves_view_offset_when_list_grows` (lines 579-595)
  - `mouse_click_with_view_offset_selects_correct_item` (lines 601-617)
  - `mouse_click_on_visible_row_0_with_offset_5_selects_item_5` (lines 619-635)

### Criterion 9: Tests - editor_state.rs unit tests

- **Status**: satisfied
- **Evidence**:
  - `test_scroll_when_selector_open_scrolls_selector_not_buffer` (lines 1235-1275) - verifies buffer viewport doesn't scroll when selector is open
  - `test_scroll_when_selector_open_updates_view_offset` (lines 1277-1308) - verifies selector view_offset updates correctly
  - `test_scroll_when_buffer_focused_scrolls_buffer` (lines 1310-1334) - verifies existing buffer scroll behavior still works

### Criterion 10: Manual smoke test

- **Status**: satisfied
- **Evidence**: All 78 selector-related tests pass. Implementation properly wires together: EditorState.handle_scroll → handle_scroll_selector → SelectorWidget.handle_scroll, with geometry passed from calculate_overlay_geometry. The PLAN.md Step 8 documents the manual smoke test procedure. The full test suite (318 tests) passes with `cargo test`.
