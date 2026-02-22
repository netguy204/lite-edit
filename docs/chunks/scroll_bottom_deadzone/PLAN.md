# Implementation Plan

## Approach

Both bugs stem from a single root cause: `RowScroller::set_scroll_offset_px` clamps
scroll position using `buffer_line_count`, but when line wrapping is enabled, the
scroll position operates in **screen row** space (as set by `ensure_visible_wrapped`).
The clamping formula `max_offset_px = (row_count - visible_rows) * row_height` uses
buffer line count, but should use total screen row count when wrapping is active.

**Fix strategy:**

1. **Add a wrap-aware clamping variant** in `Viewport` that computes the maximum
   scroll position based on total screen rows (sum of all lines' screen row counts),
   not buffer lines.

2. **Modify `handle_scroll` in `BufferFocusTarget`** to use the wrap-aware clamping
   when wrapping is enabled. This ensures the scroll offset is always clamped
   correctly, whether set by user scrolling or by `ensure_visible_wrapped`.

3. **Ensure hit-testing consistency**: The `pixel_to_buffer_position_wrapped`
   function already walks from `first_visible_line` accumulating screen rows.
   Once the scroll clamping is correct, hit-testing will naturally align because
   the viewport's `first_visible_line()` will return the correct buffer line.

**Testing approach (per TESTING_PHILOSOPHY.md):**

Write tests that verify:
- Scrolling to max position and back up responds immediately (no deadzone)
- Click at max scroll position maps to the correct buffer line
- Tests exercise the boundary: at max scroll, visible_lines before end of content

Since viewport/scroll math is pure Rust with no platform dependencies, tests can
be unit tests in `viewport.rs` and `row_scroller.rs`.

## Sequence

### Step 1: Add `set_scroll_offset_px_wrapped` to Viewport

Add a new method to `Viewport` that clamps scroll offset using total screen rows
instead of buffer lines. This method should:

1. Accept a closure to get line lengths (like `ensure_visible_wrapped` does)
2. Accept the `WrapLayout` for computing screen rows per line
3. Compute `total_screen_rows = sum(screen_rows_for_line(line_len_fn(i)) for i in 0..line_count)`
4. Compute `max_offset_px = (total_screen_rows - visible_rows).max(0) * line_height`
5. Clamp `scroll_offset_px` to `[0.0, max_offset_px]`

Location: `crates/editor/src/viewport.rs`

```rust
// Chunk: docs/chunks/scroll_bottom_deadzone - Wrap-aware scroll clamping
pub fn set_scroll_offset_px_wrapped<F>(
    &mut self,
    px: f32,
    line_count: usize,
    wrap_layout: &WrapLayout,
    line_len_fn: F,
) where
    F: Fn(usize) -> usize,
{
    let total_screen_rows = self.compute_total_screen_rows(line_count, wrap_layout, &line_len_fn);
    let max_rows = total_screen_rows.saturating_sub(self.visible_lines());
    let max_offset_px = max_rows as f32 * self.line_height();
    self.scroller.set_scroll_offset_unclamped(px.clamp(0.0, max_offset_px));
}
```

### Step 2: Write failing tests for scroll deadzone

Before implementing the fix in `handle_scroll`, write tests that demonstrate the
bug. Tests should:

1. Set up a viewport with wrapping enabled
2. Create a scenario where wrapped lines produce more screen rows than buffer lines
3. Scroll to the maximum position
4. Verify that scrolling back up responds immediately (no stuck offset)

Location: `crates/editor/src/viewport.rs` (in `#[cfg(test)]` module)

Example test structure:
```rust
#[test]
fn test_scroll_at_max_wrapped_responds_immediately() {
    // 10 buffer lines, some wrap to multiple screen rows
    // Total screen rows > buffer lines
    // Scroll to max, then scroll back up by 1px
    // Assert: scroll_offset_px decreased by 1px (not stuck)
}
```

### Step 3: Write failing tests for click-to-cursor at max scroll

Test that clicking at the bottom of the viewport when scrolled to max maps to
the correct buffer position.

Location: `crates/editor/src/buffer_target.rs` (in `#[cfg(test)]` module)

Example test structure:
```rust
#[test]
fn test_click_at_max_scroll_maps_correctly() {
    // Set up buffer with wrapped lines
    // Scroll to maximum position
    // Click on the last visible line
    // Assert: cursor is on the expected buffer line (not off-by-one)
}
```

### Step 4: Update handle_scroll to use wrap-aware clamping

Modify `BufferFocusTarget::handle_scroll` to use `set_scroll_offset_px_wrapped`
when wrapping is enabled.

Location: `crates/editor/src/buffer_target.rs`

The modification:
```rust
fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext) {
    let current_px = ctx.viewport.scroll_offset_px();
    let new_px = current_px + delta.dy as f32;
    let line_count = ctx.buffer.line_count();

    // Chunk: docs/chunks/scroll_bottom_deadzone - Wrap-aware scroll clamping
    let wrap_layout = ctx.wrap_layout();
    ctx.viewport.set_scroll_offset_px_wrapped(
        new_px,
        line_count,
        &wrap_layout,
        |line| ctx.buffer.line_len(line),
    );

    // ... rest unchanged
}
```

### Step 5: Verify tests pass and no regressions

1. Run the new tests - they should now pass
2. Run all existing viewport and scroll tests - they should still pass
3. Run the full test suite: `cargo test -p lite-edit-editor`

### Step 6: Add regression tests for non-wrapped case

Ensure that the wrap-aware clamping doesn't break the non-wrapped case:

1. Test scroll clamping when no lines wrap (total_screen_rows == line_count)
2. Verify same behavior as before for simple scrolling scenarios

Location: `crates/editor/src/viewport.rs`

### Step 7: Update code_paths in GOAL.md

Confirm the code_paths frontmatter is accurate:
- `crates/editor/src/row_scroller.rs` - If modified for clamping helpers
- `crates/editor/src/viewport.rs` - New wrap-aware clamping method
- `crates/editor/src/buffer_target.rs` - Modified handle_scroll

## Risks and Open Questions

1. **Performance**: `compute_total_screen_rows` iterates all lines on every scroll
   event. For large files with many wrapped lines, this could add latency. If
   profiling shows this is a problem, we could cache the total screen row count
   and invalidate on buffer/wrap changes.

2. **Non-wrapped scroll path**: The existing `set_scroll_offset_px` takes
   `buffer_line_count` and works correctly for non-wrapped mode. We're adding a
   parallel path for wrapped mode. Need to ensure callers use the right one.

3. **Interaction with ensure_visible_wrapped**: This method already computes
   max_offset_px using screen rows. After our change, both user scrolling and
   cursor-following will use consistent coordinate spaces.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->
