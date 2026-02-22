# Implementation Plan

## Approach

The selector overlay cannot scroll to the true bottom of a long item list. The
GOAL.md identifies three likely root causes:

1. `visible_items` (capped by `max_visible_items`) being confused with total item count
2. Panel sizing logic constraining scroll range incorrectly
3. `RowScroller::set_scroll_offset_px` clamping based on wrong `row_count`

**Analysis:**

After examining the code, the bug is in how `update_visible_size` is called:

```rust
// editor_state.rs (line 479, 784, 819-820, 1029, 1183)
selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);
```

This call to `update_visible_size(height_px)` passes the **panel height** (visible
items × item height), not the **actual visible pixel height**. Inside
`RowScroller::update_size(height_px, row_count)`:

```rust
self.visible_rows = (height_px / self.row_height).floor() as usize;
```

So `visible_rows` gets set to `visible_items` (e.g., 18 if the panel can show 18
items). But the critical issue is in `set_scroll_offset_px`:

```rust
pub fn set_scroll_offset_px(&mut self, px: f32, row_count: usize) {
    let max_rows = row_count.saturating_sub(self.visible_rows);
    let max_offset_px = max_rows as f32 * self.row_height;
    self.scroll_offset_px = px.clamp(0.0, max_offset_px);
}
```

When `row_count` (total items) is, say, 50 and `visible_rows` is 18:
- `max_rows = 50 - 18 = 32`
- `max_offset_px = 32 * row_height`
- We can scroll 32 rows, showing rows 32-49 in the viewport ✓

**Wait, that math is correct.** Let me re-examine...

Actually, the issue is that `RowScroller` is receiving the correct `row_count`
from `self.items.len()` but the wrong information about how many items actually
fit in the viewport. The call chain is:

1. `calculate_overlay_geometry` computes `visible_items = item_count.min(max_visible_items)`
2. `update_visible_size(visible_items * item_height)` passes viewport height based on
   the **capped visible count**, not the panel's physical pixel height

If there are 50 items and `max_visible_items` is 18, then `visible_items = 18`.
But if there are only 5 items, `visible_items = 5`, and the panel shrinks accordingly.

**The real bug**: When `item_count > max_visible_items`, the overlay panel height is
`max_visible_items * item_height`, and `visible_rows` = `max_visible_items`. This is
correct. When you scroll down, `ensure_visible(selected_index, items.len())` uses
the full item count. The scroll clamping should work.

Let me trace through a concrete example with 50 items and `max_visible_items` = 18:
- `visible_items` = 18, `visible_rows` = 18
- `set_scroll_offset_px(px, 50)` → `max_rows = 50 - 18 = 32`
- We can scroll to row 32, seeing items 32-49

That's only 18 items visible at max scroll, but the last item is 49 (index 49).
So the last visible range should be `32..50` (items 32-49). That seems correct.

**Deeper investigation needed**: The bug may be in how `visible_item_range()` is
computed or used by the renderer. Let me check `RowScroller::visible_range`:

```rust
pub fn visible_range(&self, row_count: usize) -> Range<usize> {
    let first_row = self.first_visible_row();
    let start = first_row;
    let end = (first_row + self.visible_rows + 1).min(row_count);
    start..end
}
```

At max scroll (first_row = 32), `end = min(32 + 18 + 1, 50) = min(51, 50) = 50`.
So visible_range = `32..50`. That's correct.

**Hypothesis refinement**: The issue may be in `calculate_overlay_geometry` itself.
Looking at line 176:

```rust
let visible_items = item_count.min(max_visible_items).max(0);
```

This is used for **panel sizing** but is then passed to `update_visible_size`:

```rust
selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);
```

So when we have 50 items, `visible_items = 18`, and `update_visible_size(18 * item_height)`.
Inside `RowScroller::update_size(288, 50)` (assuming item_height = 16):
- `visible_rows = floor(288 / 16) = 18` ✓

Then `set_scroll_offset_px(px, 50)`:
- `max_rows = 50 - 18 = 32`
- `max_offset_px = 32 * 16 = 512` ✓

This all looks correct. Let me look for the actual bug by re-reading GOAL.md
more carefully:

> "the user can arrow-key or scroll down but the viewport stops well before the
> end of the list — there are still >3 selectable items below the lowest visible item"

This suggests `ensure_visible` isn't scrolling far enough. Let's trace `ensure_visible`:

```rust
pub fn ensure_visible(&mut self, row: usize, row_count: usize) -> bool {
    self.ensure_visible_with_margin(row, row_count, 0)
}

pub fn ensure_visible_with_margin(...) {
    // ...
    } else if row >= first_row + effective_visible {
        let new_row = row.saturating_sub(effective_visible.saturating_sub(1));
        let target_px = new_row as f32 * self.row_height;
        self.set_scroll_offset_px(target_px, row_count);
    }
}
```

When selection reaches row 49 (last item):
- `first_row` = 31 (from previous scroll)
- `effective_visible` = 18
- `row (49) >= first_row (31) + effective_visible (18)` → `49 >= 49` → **FALSE!**

Wait, that's the boundary condition: when `row == first_row + effective_visible - 1`
(row 48), `ensure_visible` doesn't trigger because `48 < 49`. But when `row == 49`,
`49 >= 49` is TRUE (>= not >), so it should trigger.

**Found it!** When `row == first_row + visible_rows`, the condition fires and:
- `new_row = 49 - (18 - 1) = 49 - 17 = 32`
- `set_scroll_offset_px(32 * 16, 50)` = `set_scroll_offset_px(512, 50)`
- `max_offset_px = (50 - 18) * 16 = 32 * 16 = 512` ✓

This should work. But wait—there's an off-by-one: when we want row 49 visible,
setting `first_row = 32` gives visible range `32..50` which includes row 49.

**The actual bug must be elsewhere.** Let me check if `visible_rows` is being
set to something unexpected. Checking `update_visible_size`:

```rust
pub fn update_size(&mut self, height_px: f32, row_count: usize) {
    self.visible_rows = if self.row_height > 0.0 {
        (height_px / self.row_height).floor() as usize
    } else {
        0
    };
    self.set_scroll_offset_px(self.scroll_offset_px, row_count);
}
```

If `geometry.visible_items` is computed incorrectly (e.g., includes only the items
that fit in the panel, not accounting for the full item count in scroll clamping),
and then the code confuses this with `item_count` somewhere...

**FOUND THE BUG!**

Looking at `selector.rs` line 143-144:

```rust
pub fn update_visible_size(&mut self, height_px: f32) {
    self.scroll.update_size(height_px, self.items.len());
}
```

This correctly passes `self.items.len()` as `row_count`. So scroll clamping uses
the full item count. That's correct.

But wait—let me trace through again with the GOAL.md description:

> "there are still >3 selectable items below the lowest visible item"

With 50 items and 18 visible:
- Max scroll puts first_visible at 32
- Visible items: 32-49 (18 items)
- Last item index: 49
- Items "below the lowest visible item" = 0 (there are no items below 49)

Unless... the bug is that `visible_rows` is being set incorrectly! What if the
bug is that `geometry.visible_items` is being calculated from the **current**
item count rather than the max panel capacity?

Looking at `calculate_overlay_geometry` again:

```rust
let visible_items = item_count.min(max_visible_items).max(0);
```

If `item_count` is, say, 50 and `max_visible_items` is 18, then `visible_items = 18`.
This is correct for sizing the panel.

But consider this scenario:
1. User types a query, and matches shrink to 5 items
2. `visible_items = 5`, panel shrinks
3. `update_visible_size(5 * item_height)` → `visible_rows = 5`
4. User clears query, matches expand to 50 items
5. `set_items(50 items)` called
6. `update_visible_size` is called with... what?

Looking at the event handlers:

```rust
// handle_key_selector (line 776-784)
let geometry = calculate_overlay_geometry(
    self.view_width,
    self.view_height,
    line_height,
    selector.items().len(),  // <-- OLD item count (before set_items)
);
selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);
```

This is called BEFORE `set_items`! Then:

```rust
// (line 813-821)
sel.set_items(items);
let new_geometry = calculate_overlay_geometry(
    self.view_width,
    self.view_height,
    line_height,
    sel.items().len(),  // <-- NEW item count
);
sel.update_visible_size(
    new_geometry.visible_items as f32 * new_geometry.item_height,
);
```

This is called AFTER `set_items` ✓. So the geometry should be recalculated.

**Aha!** The issue might be that `visible_items` is capped by `max_visible_items`,
but the rendering uses `geometry.visible_items` for the scissor rect height, while
scrolling uses `visible_rows` (which equals `visible_items`). Both should be 18
when there are 50 items.

Let me trace through the specific failure case from GOAL.md more carefully:

> "search for a common term (e.g. "GOAL" in a project with many matching files)"

Say this returns 100 files. Panel can show 18. The user arrow-keys down:
- Selection goes 0, 1, 2, ... 17 (no scroll yet, items 0-17 visible)
- Selection goes to 18 → `ensure_visible(18, 100)` fires
  - `first_row` = 0
  - `row (18) >= first_row (0) + visible_rows (18)` → TRUE
  - `new_row = 18 - 17 = 1`
  - Scroll to row 1, now items 1-18 visible, selection at 18 ✓
- Continue to selection 96:
  - `first_row` = 79 (items 79-96 visible)
  - Selection at 96, which is 96 - 79 = 17 = visible_rows - 1 ✓
- Selection goes to 97:
  - `row (97) >= first_row (79) + visible_rows (18)` → `97 >= 97` → TRUE
  - `new_row = 97 - 17 = 80`
  - Scroll to row 80, now items 80-97 visible ✓
  - `set_scroll_offset_px(80 * 16, 100)` where `max_offset_px = (100 - 18) * 16 = 1312` ✓
- ...continue to selection 99 (last item):
  - `first_row` = 82
  - Selection goes to 99, `row (99) >= 82 + 18` → `99 >= 100` → FALSE!

**FOUND IT!** When we're at `first_row = 82` and try to go to row 99:
- `99 >= 82 + 18` → `99 >= 100` is **FALSE**

But row 99 IS visible when `first_row = 82`! The visible range is `82..100`
(items 82-99). So `ensure_visible` correctly does NOT fire.

But wait—the user said "there are still >3 selectable items below the lowest
visible item". Let me reconsider.

With 100 items and `max_visible_items = 18`:
- Panel shows 18 items
- Max scroll offset: `(100 - 18) * 16 = 1312 pixels`
- At max scroll, `first_visible_row = 82`, visible range = `82..100`

But what if `visible_rows` is being set incorrectly? What if it's smaller than 18?

Looking at `calculate_overlay_geometry`:

```rust
let max_panel_height = view_height * OVERLAY_MAX_HEIGHT_RATIO;  // 50% of view height
let max_items_height = max_panel_height - fixed_height - OVERLAY_PADDING_Y;
let max_visible_items = (max_items_height / line_height).floor() as usize;
```

If `view_height = 800`, `max_panel_height = 400`. With typical fixed_height and
padding (~37px total), `max_items_height ≈ 363`, and `max_visible_items ≈ 18`
(with 20px line height) or `≈ 22` (with 16px line height).

But the key insight from GOAL.md:

> "the user can arrow-key or scroll down but the viewport stops well before the end"

The **viewport stops** — this means the scroll clamping is preventing further
scroll. This happens in `set_scroll_offset_px`:

```rust
let max_rows = row_count.saturating_sub(self.visible_rows);
let max_offset_px = max_rows as f32 * self.row_height;
self.scroll_offset_px = px.clamp(0.0, max_offset_px);
```

If `visible_rows` is larger than it should be, `max_rows` will be smaller,
and `max_offset_px` will be too small.

**Root cause identified!**

When `calculate_overlay_geometry` computes `visible_items = item_count.min(max_visible_items)`,
it caps at `max_visible_items`. But it ALSO computes this based on `item_count`:

```rust
let visible_items = item_count.min(max_visible_items).max(0);
let items_height = visible_items as f32 * line_height;
```

If `item_count = 100` and `max_visible_items = 18`, then `visible_items = 18` ✓.

But what if somewhere the code is passing `visible_items` (18) instead of
`item_count` (100) to `set_scroll_offset_px`?

Looking at `selector.rs`:

```rust
pub fn set_items(&mut self, items: Vec<String>) {
    self.items = items;
    // ...
    let px = self.scroll.scroll_offset_px();
    self.scroll.set_scroll_offset_px(px, self.items.len());  // ← Uses items.len() ✓
}
```

And `update_visible_size`:

```rust
pub fn update_visible_size(&mut self, height_px: f32) {
    self.scroll.update_size(height_px, self.items.len());  // ← Uses items.len() ✓
}
```

Both correctly use `self.items.len()`. So the bug must be elsewhere.

**Alternative hypothesis**: The bug is in the HEIGHT passed to `update_visible_size`,
not the row count.

The call is:
```rust
selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);
```

If `visible_items = 18` and `item_height = 20`, then `height_px = 360`.
Inside `update_size(360, 100)`:
- `visible_rows = floor(360 / 20) = 18` ✓

This is correct! So `visible_rows` should be 18.

**But what if `geometry.item_height` differs from `self.scroll.row_height`?**

Looking at `SelectorWidget::new()`:

```rust
let metrics = FontMetrics {
    advance_width: 8.0,
    line_height: 16.0,  // ← default row height
    // ...
};
self.scroll = RowScroller::new(metrics.line_height as f32);  // ← row_height = 16.0
```

And `set_item_height`:

```rust
pub fn set_item_height(&mut self, height: f32) {
    let offset = self.scroll.scroll_offset_px();
    let visible_rows = self.scroll.visible_rows();
    self.scroll = RowScroller::new(height);
    self.scroll.update_size(visible_rows as f32 * height, self.items.len());
    self.scroll.set_scroll_offset_px(offset, self.items.len());
}
```

I don't see where `set_item_height` is called in the event handlers. Let me check:

Searching for `set_item_height` in `editor_state.rs`... Not found!

**BUG CONFIRMED!** The `RowScroller` is initialized with `row_height = 16.0` (the
default FontMetrics), but the actual line height used for rendering comes from
`self.font_metrics.line_height` in `editor_state.rs`. If these differ, the scroll
arithmetic will be wrong.

For example, if `font_metrics.line_height = 20.0` but `row_scroller.row_height = 16.0`:
- `geometry.item_height = 20.0`
- `visible_items = 18`
- `update_visible_size(18 * 20 = 360)`
- In `update_size(360, 100)`: `visible_rows = floor(360 / 16) = 22`
- `max_rows = 100 - 22 = 78`
- `max_offset_px = 78 * 16 = 1248`
- `max_scroll_row = 1248 / 16 = 78` (but we wanted to see row 82 as first visible!)

The scroll range is too short because `row_height` in the scroller doesn't match
the actual `item_height` used for rendering!

**Fix Strategy:**

1. Ensure `RowScroller::row_height` matches the actual `item_height` used for
   rendering (from `font_metrics.line_height`)
2. Either:
   a. Call `set_item_height` when geometry is calculated, OR
   b. Pass the correct row_height when constructing RowScroller

Option (a) is better because it handles dynamic font changes. The event handlers
already calculate `geometry.item_height` (which equals `line_height`), so we should
call `selector.set_item_height(geometry.item_height)` before `update_visible_size`.

## Sequence

### Step 1: Write failing unit test demonstrating the bug

Create a test in `selector.rs` that:
1. Sets up a SelectorWidget with many items (e.g., 50)
2. Uses a row_height that differs from the internal default (e.g., 20.0 vs 16.0)
3. Calls `update_visible_size` with the correct height based on external row_height
4. Attempts to scroll to the bottom
5. Verifies that the last item is within `visible_item_range()`

This test should FAIL before the fix, demonstrating the row_height mismatch.

Location: `crates/editor/src/selector.rs` (tests module)

### Step 2: Add set_item_height calls in editor_state.rs event handlers

Add `selector.set_item_height(geometry.item_height)` before each
`update_visible_size` call in:
- `open_file_picker`
- `handle_key_selector`
- `handle_mouse_selector`
- `handle_scroll_selector`

This ensures the `RowScroller` uses the same row_height as the renderer.

Location: `crates/editor/src/editor_state.rs`

### Step 3: Verify the failing test now passes

Run the test from Step 1 to confirm the fix works.

### Step 4: Add regression test for end-of-list scrolling

Add a comprehensive test that:
1. Creates a SelectorWidget with 2× panel capacity items
2. Sets correct item_height
3. Navigates to the last item via repeated Down key presses
4. Verifies:
   - `selected_index()` == last item index
   - `visible_item_range()` contains the last item
   - The selection's draw_idx is within `[0, visible_rows - 1]`

This documents the success criteria from GOAL.md.

Location: `crates/editor/src/selector.rs` (tests module)

### Step 5: Add test for mouse scroll to bottom

Add a test that:
1. Creates a SelectorWidget with many items
2. Sets correct item_height and visible size
3. Applies enough scroll delta to reach the maximum scroll offset
4. Verifies the last item is within visible_item_range()

Location: `crates/editor/src/selector.rs` (tests module)

### Step 6: Run existing tests and verify no regressions

Run `cargo test` to ensure all existing selector and overlay tests pass.

## Risks and Open Questions

1. **set_item_height reconstructs the RowScroller** - This preserves scroll offset
   but may have subtle edge cases. The current implementation looks correct, but
   we should verify scroll state is preserved.

2. **Multiple calls to set_item_height per event** - Each event handler may call
   set_item_height redundantly. This is safe but slightly inefficient. We could
   optimize by checking if the height changed, but that's premature optimization.

3. **Font metrics may not be available at SelectorWidget::new() time** - The
   current design uses default FontMetrics in the constructor. This is acceptable
   because the actual font metrics are set via `set_item_height` before any
   meaningful scroll operations.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->