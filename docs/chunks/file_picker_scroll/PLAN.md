<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds scroll support to the file picker overlay, building on the
existing `SelectorWidget` and `selector_overlay` infrastructure from the
`file_picker` chunk. The strategy is to:

1. Add scroll-related state (`view_offset`, `visible_items`) to `SelectorWidget`
2. Add a `handle_scroll` method to translate pixel deltas into row shifts
3. Update existing methods (`handle_key`, `handle_mouse`, `set_items`) to be
   offset-aware
4. Update the renderer to use `view_offset` when iterating items
5. Wire scroll events through `EditorState` to the selector when focused

The implementation follows the Humble View Architecture: all scroll logic lives
in pure Rust state manipulation (`SelectorWidget`), while the Metal renderer
simply projects that state to pixels. This keeps the logic testable without GPU
dependencies.

Tests follow TDD as per `docs/trunk/TESTING_PHILOSOPHY.md`: write failing tests
for each behavior first, then implement the minimum code to make them pass.

## Sequence

### Step 1: Add `view_offset` and `visible_items` fields to `SelectorWidget`

Location: `crates/editor/src/selector.rs`

Add two new fields to `SelectorWidget`:
- `view_offset: usize` (default `0`) — index of the first visible item
- `visible_items: usize` (default `0`) — number of visible rows (updated externally)

Add accessors:
- `pub fn view_offset(&self) -> usize`
- `pub fn set_visible_items(&mut self, n: usize)`

**Tests (write first):**
- `new_widget_has_view_offset_zero`
- `new_widget_has_visible_items_zero`
- `set_visible_items_stores_value`

### Step 2: Implement `SelectorWidget::handle_scroll`

Location: `crates/editor/src/selector.rs`

Add method:
```rust
pub fn handle_scroll(&mut self, delta_y: f64, item_height: f64, visible_items: usize)
```

Behavior:
- Compute rows to shift: `(delta_y / item_height).round() as isize`
- Update `view_offset` by adding the row delta
- Clamp `view_offset` to `0..=items.len().saturating_sub(visible_items)`
- No-op if items fit entirely within `visible_items`

**Tests (write first):**
- `scroll_down_increments_view_offset`
- `scroll_up_decrements_view_offset`
- `scroll_clamps_at_max_offset`
- `scroll_clamps_at_zero`
- `scroll_on_short_list_is_noop` (items.len() <= visible_items)
- `scroll_on_empty_list_is_noop`

### Step 3: Update `handle_key` to keep selection visible

Location: `crates/editor/src/selector.rs`

Modify the Up/Down arrow handling in `handle_key`:
- After moving `selected_index`, check if it's outside the visible window
- If `selected_index < view_offset`, set `view_offset = selected_index`
- If `selected_index >= view_offset + visible_items`, set
  `view_offset = selected_index - visible_items + 1`

Note: This requires `visible_items` to be stored on the widget (from Step 1).

**Tests (write first):**
- `down_past_visible_window_increments_view_offset`
- `up_past_visible_window_decrements_view_offset`
- `down_within_visible_window_does_not_change_view_offset`
- `up_within_visible_window_does_not_change_view_offset`

### Step 4: Update `set_items` to clamp `view_offset`

Location: `crates/editor/src/selector.rs`

Modify `set_items` to clamp `view_offset` after replacing items:
```rust
let max_offset = self.items.len().saturating_sub(self.visible_items);
self.view_offset = self.view_offset.min(max_offset);
```

This ensures `view_offset` doesn't point past the end when the list shrinks
(e.g., after query narrows results).

**Tests (write first):**
- `set_items_clamps_view_offset_when_list_shrinks`
- `set_items_preserves_view_offset_when_list_grows`

### Step 5: Update `handle_mouse` to be offset-aware

Location: `crates/editor/src/selector.rs`

Modify `handle_mouse` to account for `view_offset` when computing the item index:
- The clicked row is computed as before: `(relative_y / item_height) as usize`
- But the actual item index is `view_offset + row`
- Clamp to ensure `view_offset + row < items.len()` before setting `selected_index`

**Tests (write first):**
- `mouse_click_with_view_offset_selects_correct_item`
- `mouse_click_on_visible_row_0_with_offset_5_selects_item_5`

### Step 6: Update renderer to use `view_offset`

Location: `crates/editor/src/selector_overlay.rs`

In `SelectorGlyphBuffer::update_from_widget`, modify Phase 6 (Item Text):
```rust
for (i, item) in items.iter()
    .skip(widget.view_offset())
    .take(geometry.visible_items)
    .enumerate()
```

For Phase 2 (Selection Highlight):
- Compute `visible_row = selected_index.wrapping_sub(view_offset)`
- Only render highlight if `selected_index >= view_offset` AND
  `selected_index < view_offset + visible_items`
- Otherwise emit empty quad (zero-length range)

**No unit tests needed**: This is humble view code (GPU buffer construction).
Verify visually with manual smoke test.

### Step 7: Wire scroll events through `EditorState` to selector

Location: `crates/editor/src/editor_state.rs`

Modify `EditorState::handle_scroll`:
- Remove the early-return that ignores scroll when `focus == Selector`
- When `focus == Selector`:
  - Calculate `OverlayGeometry` using `calculate_overlay_geometry`
  - Call `active_selector.handle_scroll(delta.dy, geometry.item_height, geometry.visible_items)`
  - Update `active_selector.set_visible_items(geometry.visible_items)` for arrow key navigation
  - Mark `DirtyRegion::FullViewport`

Also update `handle_key_selector` and `handle_mouse_selector` to call
`set_visible_items` with the computed geometry value, ensuring the widget always
has the current `visible_items` for its scroll calculations.

**Tests (write first):**
- `scroll_when_selector_open_updates_view_offset`
- `scroll_when_buffer_focused_scrolls_buffer` (existing test, verify still works)

### Step 8: Final integration and smoke test

Verify the full interaction:
1. Open file picker (Cmd+P) in a directory with many files
2. Scroll down with trackpad/mouse wheel — items scroll
3. Scroll up — items scroll back
4. Arrow key past visible boundary — list scrolls to keep selection visible
5. Click an off-screen item after scrolling — correct item selected
6. Type to narrow query — view_offset clamps if necessary
7. Press Enter — correct file opens

## Dependencies

- `file_picker` chunk must be complete (provides base `SelectorWidget` and
  `EditorState` integration)
- `selector_widget` and `selector_rendering` chunks must be complete (provides
  the widget model and overlay geometry)

## Risks and Open Questions

- **Scroll delta sign convention**: The existing `ScrollDelta` uses positive `dy`
  for scroll down (content moves up). Verify this matches macOS trackpad behavior.
  The GOAL.md specifies this convention explicitly.

- **Item height precision**: The selector uses `line_height` as `item_height`.
  Ensure this value is passed consistently from `OverlayGeometry` through to
  `handle_scroll`. Mismatch could cause drift.

- **Large scroll deltas**: Very fast scrolling could produce large `delta_y`
  values. The clamping in `handle_scroll` should handle this, but verify with
  manual testing.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->