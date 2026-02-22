<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk replaces `SelectorWidget`'s ad-hoc integer scroll model with the
`RowScroller` struct extracted in the `row_scroller_extract` chunk. The approach
is field-by-field replacement followed by method-by-method migration:

1. **Replace fields**: Remove `view_offset: usize` and `visible_items: usize`,
   add `scroll: RowScroller`. The scroller's `row_height` is initialized from
   the `FontMetrics.line_height` already used for the MiniBuffer.

2. **Migrate scroll handling**: `handle_scroll` stops rounding pixel deltas to
   integer rows. It accumulates raw pixels via `scroll.set_scroll_offset_px()`.

3. **Migrate arrow-key navigation**: Replace manual `view_offset` adjustments
   with `scroll.ensure_visible()` after moving `selected_index`.

4. **Migrate item updates**: `set_items` re-clamps scroll position to the new
   item count without resetting to zero.

5. **Migrate hit-testing**: `handle_mouse` adds `scroll_fraction_px()` to the
   relative y before dividing by `item_height`, so sub-row scroll state is
   accounted for in click targeting.

6. **Expose new accessors**: `first_visible_item()`, `scroll_fraction_px()`,
   `visible_item_range()` — thin wrappers over the inner `RowScroller`.

7. **Remove old accessor**: Delete `view_offset()` and migrate any callers to
   `first_visible_item()`.

This follows the project's Humble View Architecture: `SelectorWidget` is pure
interaction state with no platform dependencies. The `RowScroller` it contains
is likewise platform-agnostic scroll arithmetic.

**Testing approach**: Per TESTING_PHILOSOPHY.md, tests must assert semantically
meaningful properties. We:
- Update existing tests to use the new API (`first_visible_item()` instead of
  `view_offset()`).
- Add tests verifying that sub-row pixel deltas are preserved across multiple
  scroll events (the bug this chunk fixes).
- Add tests for hit-test correctness with fractional scroll positions.

## Sequence

### Step 1: Add RowScroller import and replace struct fields

**Location**: `crates/editor/src/selector.rs`

Add `use crate::row_scroller::RowScroller;` at the top.

Replace the two fields in `SelectorWidget`:
```rust
// Remove:
view_offset: usize,
visible_items: usize,

// Add:
scroll: RowScroller,
```

Update `SelectorWidget::new()` to initialize `scroll` using
`RowScroller::new(metrics.line_height)`. The `row_height` comes from the same
`FontMetrics` already used for the `MiniBuffer`.

Update `Default` impl (it delegates to `new()`, so no change needed there).

### Step 2: Migrate set_visible_items to update_visible_size

**Location**: `crates/editor/src/selector.rs`

Replace `set_visible_items(&mut self, n: usize)` with:

```rust
/// Updates the visible size from the pixel height of the list area.
///
/// This forwards to `RowScroller::update_size(height_px)`, which computes
/// visible_rows from `height_px / row_height`.
pub fn update_visible_size(&mut self, height_px: f32) {
    self.scroll.update_size(height_px);
}

/// Sets the row height (item height) in pixels.
///
/// Call this when font metrics change.
pub fn set_item_height(&mut self, height: f32) {
    // RowScroller doesn't have a setter for row_height (it's set at construction),
    // so we create a new scroller preserving scroll position.
    let offset = self.scroll.scroll_offset_px();
    let visible_rows = self.scroll.visible_rows();
    self.scroll = RowScroller::new(height);
    self.scroll.update_size(visible_rows as f32 * height);
    self.scroll.set_scroll_offset_px(offset, self.items.len());
}
```

Note: If `RowScroller` doesn't expose a `row_height` setter, we need to
reconstruct it. Alternatively, if callers never change `item_height` after
construction, we can omit `set_item_height` and document that constraint.

### Step 3: Migrate handle_scroll to accumulate raw pixels

**Location**: `crates/editor/src/selector.rs`

Rewrite `handle_scroll`:

```rust
/// Handles a scroll event by adjusting the scroll offset.
///
/// # Arguments
///
/// * `delta_y` - The raw pixel delta (positive = scroll down / content moves up).
///
/// # Behavior
///
/// Accumulates the raw pixel delta via `RowScroller::set_scroll_offset_px`.
/// No rounding to row boundaries — fractional positions are preserved for
/// smooth scrolling.
pub fn handle_scroll(&mut self, delta_y: f64) {
    let new_px = self.scroll.scroll_offset_px() + delta_y as f32;
    self.scroll.set_scroll_offset_px(new_px, self.items.len());
}
```

The `item_height` and `visible_items` parameters are removed from the signature;
the scroller already knows `row_height` and `visible_rows`.

### Step 4: Migrate arrow-key navigation to use ensure_visible

**Location**: `crates/editor/src/selector.rs`, in `handle_key`

Replace the manual `view_offset` adjustments in the `Key::Up` and `Key::Down`
arms with calls to `scroll.ensure_visible()`:

```rust
Key::Up => {
    self.selected_index = self.selected_index.saturating_sub(1);
    self.scroll.ensure_visible(self.selected_index, self.items.len());
    SelectorOutcome::Pending
}
Key::Down => {
    if !self.items.is_empty() {
        let max_index = self.items.len() - 1;
        if self.selected_index < max_index {
            self.selected_index += 1;
        }
    }
    self.scroll.ensure_visible(self.selected_index, self.items.len());
    SelectorOutcome::Pending
}
```

### Step 5: Migrate set_items to re-clamp scroll position

**Location**: `crates/editor/src/selector.rs`

Update `set_items` to re-clamp the scroll offset when the item list changes:

```rust
pub fn set_items(&mut self, items: Vec<String>) {
    self.items = items;
    // Clamp selected_index to valid range
    if self.items.is_empty() {
        self.selected_index = 0;
    } else {
        self.selected_index = self.selected_index.min(self.items.len() - 1);
    }
    // Re-clamp scroll offset to new item count without resetting to zero
    let px = self.scroll.scroll_offset_px();
    self.scroll.set_scroll_offset_px(px, self.items.len());
}
```

### Step 6: Migrate handle_mouse hit-testing to account for fractional offset

**Location**: `crates/editor/src/selector.rs`

Update `handle_mouse` to compute the clicked row accounting for `scroll_fraction_px`:

```rust
pub fn handle_mouse(
    &mut self,
    position: (f64, f64),
    kind: MouseEventKind,
    item_height: f64,
    list_origin_y: f64,
) -> SelectorOutcome {
    // Check if position is within list bounds
    if position.1 < list_origin_y || self.items.is_empty() {
        return SelectorOutcome::Pending;
    }

    // Compute which item was clicked, accounting for fractional scroll offset
    let first = self.scroll.first_visible_row();
    let frac = self.scroll.scroll_fraction_px() as f64;
    let relative_y = position.1 - list_origin_y + frac;
    let row = (relative_y / item_height).floor() as usize;
    let item_index = first + row;

    // Check if item_index is within valid range
    if item_index >= self.items.len() {
        return SelectorOutcome::Pending;
    }

    match kind {
        MouseEventKind::Down => {
            self.selected_index = item_index;
            SelectorOutcome::Pending
        }
        MouseEventKind::Up => {
            if item_index == self.selected_index {
                SelectorOutcome::Confirmed(self.selected_index)
            } else {
                self.selected_index = item_index;
                SelectorOutcome::Pending
            }
        }
        MouseEventKind::Moved => SelectorOutcome::Pending,
    }
}
```

### Step 7: Add new public accessors and remove view_offset()

**Location**: `crates/editor/src/selector.rs`

Remove `view_offset()` and add three new accessors:

```rust
/// Returns the index of the first visible item.
///
/// Delegates to `RowScroller::first_visible_row()`.
pub fn first_visible_item(&self) -> usize {
    self.scroll.first_visible_row()
}

/// Returns the fractional pixel offset within the top row.
///
/// Delegates to `RowScroller::scroll_fraction_px()`. Renderers use this
/// to offset item drawing for smooth sub-row scrolling.
pub fn scroll_fraction_px(&self) -> f32 {
    self.scroll.scroll_fraction_px()
}

/// Returns the range of items visible in the viewport.
///
/// Delegates to `RowScroller::visible_range(item_count)`. The range
/// includes partially visible items at the top and bottom.
pub fn visible_item_range(&self) -> std::ops::Range<usize> {
    self.scroll.visible_range(self.items.len())
}
```

### Step 8: Update existing tests to use new API

**Location**: `crates/editor/src/selector.rs`, in `#[cfg(test)] mod tests`

All tests that reference `view_offset()` must be updated to use
`first_visible_item()`. All tests that call `handle_scroll` with three arguments
must be updated to call the new single-argument version, setting up visible size
via `update_visible_size()` beforehand.

Specific test updates:
- `new_widget_has_view_offset_zero` → `new_widget_has_first_visible_item_zero`
- Tests calling `widget.view_offset()` → `widget.first_visible_item()`
- Tests calling `widget.handle_scroll(delta, height, visible)` → first call
  `widget.update_visible_size(visible * height)`, then `widget.handle_scroll(delta)`
- Tests calling `widget.set_visible_items(n)` → compute pixel height and call
  `widget.update_visible_size(n * row_height)`

### Step 9: Add tests for smooth scroll accumulation

**Location**: `crates/editor/src/selector.rs`, in `#[cfg(test)] mod tests`

Add tests that verify sub-row deltas are preserved across multiple events:

```rust
#[test]
fn scroll_accumulates_sub_row_deltas() {
    let mut widget = SelectorWidget::new();
    // 20 items
    widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
    // row_height is 16.0 from default FontMetrics
    widget.update_visible_size(80.0); // 5 visible rows

    // Scroll 5 pixels (less than one row)
    widget.handle_scroll(5.0);
    assert_eq!(widget.first_visible_item(), 0);
    assert!((widget.scroll_fraction_px() - 5.0).abs() < 0.001);

    // Scroll another 5 pixels (total 10, still less than row)
    widget.handle_scroll(5.0);
    assert_eq!(widget.first_visible_item(), 0);
    assert!((widget.scroll_fraction_px() - 10.0).abs() < 0.001);

    // Scroll 6 more pixels (total 16, exactly one row)
    widget.handle_scroll(6.0);
    assert_eq!(widget.first_visible_item(), 1);
    assert!((widget.scroll_fraction_px() - 0.0).abs() < 0.001);
}

#[test]
fn scroll_preserves_fraction_across_row_boundary() {
    let mut widget = SelectorWidget::new();
    widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
    widget.update_visible_size(80.0);

    // Scroll 20 pixels (1 row + 4 pixels)
    widget.handle_scroll(20.0);
    assert_eq!(widget.first_visible_item(), 1);
    assert!((widget.scroll_fraction_px() - 4.0).abs() < 0.001);
}
```

### Step 10: Add tests for fractional hit-testing

**Location**: `crates/editor/src/selector.rs`, in `#[cfg(test)] mod tests`

Add tests verifying that clicks at fractional scroll positions select the
correct item:

```rust
#[test]
fn mouse_click_with_fractional_scroll_selects_correct_item() {
    let mut widget = SelectorWidget::new();
    widget.set_items((0..20).map(|i| format!("item{}", i)).collect());
    widget.update_visible_size(80.0); // 5 visible rows, row_height = 16

    // Scroll 8 pixels (half a row)
    widget.handle_scroll(8.0);
    assert_eq!(widget.first_visible_item(), 0);
    assert!((widget.scroll_fraction_px() - 8.0).abs() < 0.001);

    // Click at y=4 (within the visible portion of item 0, which starts at -8)
    // The visible portion of item 0 is y=0..8 on screen
    // relative_y + frac = 4 + 8 = 12, row = floor(12/16) = 0
    let outcome = widget.handle_mouse((50.0, 4.0), MouseEventKind::Down, 16.0, 0.0);
    assert_eq!(outcome, SelectorOutcome::Pending);
    assert_eq!(widget.selected_index(), 0);

    // Click at y=10 (within item 1, which starts at y=8 on screen)
    // relative_y + frac = 10 + 8 = 18, row = floor(18/16) = 1
    let outcome = widget.handle_mouse((50.0, 10.0), MouseEventKind::Down, 16.0, 0.0);
    assert_eq!(outcome, SelectorOutcome::Pending);
    assert_eq!(widget.selected_index(), 1);
}
```

### Step 11: Verify all tests pass and clean up

**Location**: `crates/editor/src/selector.rs`

Run `cargo test -p editor` to verify all tests pass. Fix any compilation errors
or test failures.

Remove any unused imports or dead code (e.g., if `visible_items` was used
elsewhere in calculations, those calculations now use `scroll.visible_rows()`).

---

**BACKREFERENCE COMMENTS**

The module-level backreference at the top of `selector.rs` already references
`docs/chunks/selector_widget`. This chunk modifies that code but doesn't change
its fundamental purpose, so no additional backreference is needed. The
`RowScroller` backreference in `row_scroller.rs` is already in place from the
`row_scroller_extract` chunk.

## Dependencies

- **`row_scroller_extract` chunk (ACTIVE)**: Provides the `RowScroller` struct
  with `new()`, `row_height()`, `visible_rows()`, `first_visible_row()`,
  `scroll_fraction_px()`, `scroll_offset_px()`, `set_scroll_offset_px()`,
  `update_size()`, `visible_range()`, `scroll_to()`, `ensure_visible()`,
  `row_to_visible_offset()`, and `visible_offset_to_row()`.

- **`selector_coord_flip` chunk (HISTORICAL)**: Ensures that mouse y-coordinates
  passed to `SelectorWidget::handle_mouse` are already flipped to top-relative
  coordinates. This chunk assumes that flip is in place; without it, hit-testing
  math would be incorrect in a different way.

## Risks and Open Questions

- **RowScroller row_height immutability**: `RowScroller::new(row_height)` sets
  the row height at construction time, and the struct doesn't expose a setter.
  If callers need to change `item_height` dynamically (e.g., after font size
  change), we'll need to either reconstruct the scroller or add a setter to
  `RowScroller`. For now, assume font metrics are fixed at selector construction.
  If this becomes a problem, escalate to the operator.

- **Test row_height mismatch**: Existing tests use `item_height = 20.0` in
  `handle_scroll` calls, but the default `FontMetrics.line_height` is `16.0`.
  After migration, tests must use the scroller's actual `row_height` (16.0) or
  explicitly set a different height via `set_item_height()` if we add that
  method.

- **Caller migration for handle_scroll signature change**: Any code outside
  `selector.rs` that calls `handle_scroll(delta, height, visible)` will break.
  Need to audit callers. From the file picker narrative, this is likely only
  called from `handle_mouse_selector` in `editor_state.rs` — that call site
  must be updated to first set visible size via `update_visible_size()` and then
  call `handle_scroll(delta)`.

- **Caller migration for set_visible_items**: Similarly, `set_visible_items(n)`
  becomes `update_visible_size(height_px)`. Callers that were passing an integer
  count must now compute `count * item_height` or get the pixel height from
  layout geometry.

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