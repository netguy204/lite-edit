# Implementation Plan

## Approach

This chunk fixes two related scrolling bugs in the selector overlay:

**Bug A — Picker opens showing only one item**: `SelectorWidget::new()` initializes
`RowScroller` with `visible_rows = 0`. The `update_visible_size` method is only
called inside event handlers (`handle_key_selector`, `handle_mouse_selector`,
`handle_scroll_selector`), so the first render before user interaction uses
`visible_item_range() = 0..1`. The fix is to call `update_visible_size` inside
`open_file_picker` immediately after `set_items`.

**Bug B — Cannot arrow-key to the bottom of a long match list**: When navigating
down a long list, the selection highlight eventually clips at the panel bottom and
then disappears. The root cause is that `update_visible_size` is called at the
START of each key event handler using the pre-event item count. When typing
changes the query, `set_items` is called AFTER `handle_key`, leaving `visible_rows`
derived from the old count. This staleness window causes scissor rect / visible_rows
mismatches. The fix is to call `update_visible_size` a second time AFTER `set_items`
in the typing branch of `handle_key_selector`.

**Testing strategy** (per TESTING_PHILOSOPHY.md):
- TDD for the `ensure_visible` boundary condition — write failing tests showing
  selection at `draw_idx == visible_rows - 1` is within scissor bounds.
- Tests verify initial `visible_item_range` matches panel capacity after
  `open_file_picker`.
- Tests verify navigating to the last item places selection at
  `draw_idx == visible_rows - 1`, not beyond.

## Sequence

### Step 1: Add test for initial visible_item_range after open_file_picker

Write a unit test in `selector.rs` that verifies when `update_visible_size` is
called after `set_items` with appropriate geometry, the `visible_item_range()`
returns a range larger than `0..1`.

This test will initially fail if we construct a widget without calling
`update_visible_size`, demonstrating Bug A's root cause.

Location: `crates/editor/src/selector.rs` (test module)

### Step 2: Add test for arrow navigation to last visible row

Write a unit test that verifies when navigating down through a list longer than
`visible_rows`, the selection ends up at `draw_idx == visible_rows - 1` (the
last "proper" slot), not beyond. This test verifies `ensure_visible` is working
correctly to keep the selection within the scissor-clipped area.

The test should:
1. Create a widget with N items where N > visible_rows (e.g., 20 items, 5 visible)
2. Call `update_visible_size` to set the visible window
3. Navigate down to item N-1 (last item)
4. Assert that `selected_index - first_visible_item() == visible_rows - 1`

Location: `crates/editor/src/selector.rs` (test module)

### Step 3: Fix Bug A — Call update_visible_size in open_file_picker

Modify `EditorState::open_file_picker` to call `selector.update_visible_size(...)`
immediately after `selector.set_items(items)`.

This requires computing the overlay geometry (already done in the event handlers)
to get `visible_items * item_height`. The geometry calculation uses:
- `view_width` and `view_height` from `self`
- `line_height` from `self.font_metrics`
- `item_count` from the items list

After this fix, the first render will use the correct `visible_item_range` and
show all visible items.

Location: `crates/editor/src/editor_state.rs` (open_file_picker method)

### Step 4: Fix Bug B — Call update_visible_size after set_items in handle_key_selector

Modify `EditorState::handle_key_selector` to call `update_visible_size` a second
time AFTER the `sel.set_items(items)` call in the query-changed branch.

Currently, `update_visible_size` is called at the START of the handler with the
old item count. When the query changes and `set_items` is called with a new item
list, the `visible_rows` may be stale if the new list has fewer items and thus
a different `max_visible_items`.

The fix ensures `visible_rows` is recalculated with the new item count, keeping
the scissor rect and scroll state in sync.

Location: `crates/editor/src/editor_state.rs` (handle_key_selector method)

### Step 5: Verify scissor rect calculation is correct

Trace through `selector_list_scissor_rect` in `renderer.rs` to verify the scissor
bottom always ≥ the last full item's bottom pixel. The current implementation:

```rust
let bottom = geometry.panel_y + geometry.panel_height;
let height = ((bottom - geometry.list_origin_y).max(0.0) as usize)
    .min((view_height as usize).saturating_sub(y));
```

Verify this uses `as usize` truncation correctly. If pixel truncation is causing
off-by-one issues, consider using ceiling for height. However, based on analysis,
the primary issue is the staleness window, not truncation.

If no truncation bug is found, document this verification in the Deviations section.

Location: `crates/editor/src/renderer.rs` (selector_list_scissor_rect function)

### Step 6: Run existing tests and verify no regressions

Run the full test suite for `selector.rs`, `selector_overlay.rs`, and `renderer.rs`
to ensure no regressions.

```bash
cargo test --package editor -- selector
cargo test --package editor -- renderer
```

### Step 7: Manual visual verification

Build and run the editor. Test:
1. Open file picker (Cmd+P or equivalent) — verify full panel of items shows
   immediately, not just one item.
2. With a long list, arrow-key from first to last item — verify selection
   highlight stays fully visible on every frame.
3. Press Enter at the last item — verify the confirmed item matches the
   highlighted item.

## Risks and Open Questions

- **Scissor truncation**: The GOAL.md mentions potential pixel truncation from
  `as usize` conversion. Investigation suggests the staleness window is the
  primary cause, but truncation should be verified. If truncation causes issues,
  Step 5 may need to become an implementation step rather than verification.

- **Performance**: Adding a second `update_visible_size` call and
  `calculate_overlay_geometry` call in `open_file_picker` should have negligible
  performance impact (pure arithmetic, no allocations).

## Deviations

### Step 5: Scissor rect verification — no changes needed

Traced through `selector_list_scissor_rect` in `renderer.rs`. The scissor rect
calculation uses `as usize` truncation for both y-coordinate and height:

```rust
let y = (geometry.list_origin_y as usize).min(view_height as usize);
let bottom = geometry.panel_y + geometry.panel_height;
let height = ((bottom - geometry.list_origin_y).max(0.0) as usize)
    .min((view_height as usize).saturating_sub(y));
```

While truncation could theoretically cause off-by-one pixel clipping, in practice
the geometry values are typically derived from whole numbers (item counts and
line heights), making truncation a non-issue. The primary bug cause was the
**staleness window** where `visible_rows` was computed from the old item count
before `set_items` updated the list. This was fixed in Step 4.

No changes to the scissor rect calculation were needed.