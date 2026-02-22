---
status: ACTIVE
ticket: null
parent_chunk: file_picker
code_paths:
- crates/editor/src/selector.rs
- crates/editor/src/selector_overlay.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/selector.rs#SelectorWidget::first_visible_item
    implements: "Accessor for first visible item index (via RowScroller)"
  - ref: crates/editor/src/selector.rs#SelectorWidget::update_visible_size
    implements: "Setter for visible area height in pixels (computes visible row count)"
  - ref: crates/editor/src/selector.rs#SelectorWidget::set_items
    implements: "Clamps scroll offset when item list shrinks"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_scroll
    implements: "Translates pixel deltas into scroll offset adjustments via RowScroller"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_key
    implements: "Keeps selection visible when navigating with arrow keys"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_mouse
    implements: "Maps visible row to actual item index via scroll offset"
  - ref: crates/editor/src/selector_overlay.rs#SelectorGlyphBuffer::update_from_widget
    implements: "Renders visible window using first_visible_item skip/take"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_scroll_selector
    implements: "Forwards scroll events to selector when focused"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- file_picker
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# File Picker Scroll

## Minor Goal

The file picker overlay currently ignores scroll events (`handle_scroll` in
`EditorState` is a no-op when the selector is open). When a project has more
files than the overlay's visible rows, there is no way to reach items below the
fold — the user is stuck with whatever fits on screen.

This chunk adds scroll support to the file picker overlay so that trackpad and
mouse wheel scroll events pan the item list, making every matched file reachable
without having to narrow the query further.

## Success Criteria

### `SelectorWidget` gains a `view_offset` field

- `SelectorWidget` gains a `view_offset: usize` field (default `0`), representing
  the index of the first item visible in the list.
- A public `view_offset(&self) -> usize` accessor is added.

### `SelectorWidget::handle_scroll` method

- A new method is added:
  ```rust
  pub fn handle_scroll(&mut self, delta_y: f64, item_height: f64, visible_items: usize)
  ```
- `delta_y` is the raw pixel delta (positive = scroll down / content moves up,
  matching the existing `ScrollDelta` sign convention used by the buffer viewport).
- The number of rows to shift is `(delta_y / item_height).round() as isize`.
- `view_offset` is clamped so the last visible row never exceeds the last item:
  `view_offset` stays in `0..=(items.len().saturating_sub(visible_items))`.
- Scrolling on an empty list or a list that fits entirely within `visible_items`
  is a no-op.

### Arrow key navigation keeps selection visible

- `handle_key` (Up / Down arrow) updates `view_offset` after moving
  `selected_index` so the newly selected item is always within the visible window:
  - If `selected_index < view_offset`, set `view_offset = selected_index`.
  - If `selected_index >= view_offset + visible_items`, set
    `view_offset = selected_index - visible_items + 1`.
- This requires passing `visible_items: usize` to `handle_key`, or storing the
  most-recently-known `visible_items` on the widget.  
  **Preferred approach:** add a `visible_items: usize` field (default `0`)
  updated by a new `set_visible_items(n: usize)` setter, so `handle_key` can
  reference it without parameter changes.

### `set_items` clamps `view_offset`

- When `set_items` is called (e.g., after a query change narrows the list),
  `view_offset` is clamped to
  `0..=(items.len().saturating_sub(self.visible_items))` so it cannot point
  past the new end of the list.

### `handle_mouse` is offset-aware

- `handle_mouse` currently maps a clicked row index directly to `items[row]`.
  With scroll, the actual item index is `view_offset + row`.
- Update `handle_mouse` to compute the true item index as `view_offset + row`
  when setting/confirming `selected_index`.

### Renderer uses `view_offset`

- `selector_overlay.rs` `SelectorGlyphBuffer::update_from_widget` currently
  iterates `widget.items().iter().take(geometry.visible_items)`.
- Change this to:
  ```rust
  widget.items()
      .iter()
      .skip(widget.view_offset())
      .take(geometry.visible_items)
  ```
  so only the visible window of items is rendered.
- The selection highlight quad must be rendered at the row position of the
  selected item within the visible window:
  `visible_row = selected_index.wrapping_sub(view_offset)`.
  If `selected_index` is outside the visible window, omit the selection
  highlight (emit an empty quad range for `selection_range`).

### `EditorState::handle_scroll` forwards events when selector is open

- Remove the early-return no-op that ignores scroll events when the selector is
  open.
- When `focus == Selector`, forward the scroll event to the selector:
  ```rust
  let item_height = /* geometry.item_height as f64 */;
  let visible = /* geometry.visible_items */;
  self.active_selector.as_mut().unwrap()
      .handle_scroll(delta.dy as f64, item_height, visible);
  ```
- The geometry values needed (`item_height`, `visible_items`) must be derived
  from the same `OverlayGeometry` calculation already used by the renderer.
  Store the most-recently computed `OverlayGeometry` on `EditorState` (as
  `Option<OverlayGeometry>`) and update it each time the overlay is rendered, or
  recompute it on demand using the current viewport dimensions and line height.
- After forwarding, mark `DirtyRegion::FullViewport` so the updated list is
  redrawn.

### Tests

- `selector.rs` unit tests cover:
  - Scrolling down moves `view_offset` forward; clamped at max valid offset.
  - Scrolling up moves `view_offset` backward; clamped at 0.
  - Scrolling on a list that fits within `visible_items` is a no-op.
  - Arrow-key down past the bottom of the visible window increments `view_offset`.
  - Arrow-key up past the top of the visible window decrements `view_offset`.
  - `set_items` with a shorter list clamps `view_offset`.
  - `handle_mouse` click on visible row selects the correct item index
    (`view_offset + row`).
- `editor_state.rs` unit tests cover:
  - Scroll event while selector is open updates `view_offset` (not ignored).
  - Scroll event while selector is closed still scrolls the buffer viewport.

### Manual smoke test

- Open the file picker (Cmd+P) in a directory with more files than visible rows.
- Scroll down: the item list pans to reveal files not initially shown.
- Scroll up: the list pans back.
- Use arrow keys past the visible boundary: the list scrolls to keep the
  selection on screen.
- Click an item that was previously off-screen: it is selected and confirmed
  correctly.

## Relationship to Parent

The `file_picker` chunk established the Cmd+P overlay, `SelectorWidget`
interaction model, and renderer. It explicitly documented that scroll events are
**ignored** while the selector is open (`handle_scroll` early-return in
`EditorState`). This chunk lifts that restriction and wires scroll events through
to the selector, completing the interaction model for long file lists.