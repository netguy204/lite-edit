<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk refines the scroll position representation in `Viewport` from integer lines to floating-point pixels. The core strategy is:

1. **Replace `scroll_offset: usize` with `scroll_offset_px: f32`** in `Viewport`. This field accumulates raw pixel deltas from scroll events without rounding.

2. **Derive integer line index on demand** via `first_visible_line()` which computes `(scroll_offset_px / line_height).floor() as usize`, clamped to valid bounds.

3. **Expose the fractional remainder** via `scroll_fraction_px()` which returns `scroll_offset_px % line_height`. The renderer uses this to offset all drawn lines vertically by `-remainder_px`.

4. **Update clamping to work in pixel space** so `scroll_offset_px` is bounded between `0.0` and `(buffer_line_count - visible_lines) * line_height`.

5. **Modify `ensure_visible`** to snap to whole-line boundaries (pixel offsets that are multiples of `line_height`) while still operating in pixel space internally.

6. **Update `handle_scroll`** in `BufferFocusTarget` to accumulate raw pixel deltas into `scroll_offset_px` rather than rounding to integer lines.

7. **Modify the glyph buffer** to accept a `y_offset: f32` parameter that shifts all line positions down, enabling sub-pixel rendering.

This approach follows the existing architecture: the viewport remains the authoritative scroll state, and the renderer remains a humble view that projects state to screen coordinates.

## Sequence

### Step 1: Add failing tests for fractional scroll behavior

Write tests in `crates/editor/src/viewport.rs` that verify:
- Sub-line deltas accumulate correctly without triggering a line change
- Deltas that cross a line boundary advance `first_visible_line` by exactly one
- The fractional remainder is correct after several accumulated deltas
- Clamping at both ends works in pixel space

These tests will fail initially since `scroll_offset_px` doesn't exist yet.

Location: `crates/editor/src/viewport.rs` (in the `#[cfg(test)]` module)

### Step 2: Replace scroll_offset with scroll_offset_px in Viewport

Modify the `Viewport` struct:
- Replace `pub scroll_offset: usize` with `scroll_offset_px: f32` (private)
- Add `first_visible_line(&self) -> usize` that computes `(self.scroll_offset_px / self.line_height).floor() as usize`
- Add `scroll_fraction_px(&self) -> f32` that returns `self.scroll_offset_px % self.line_height`
- Add `scroll_offset_px(&self) -> f32` getter for the raw pixel offset
- Add `set_scroll_offset_px(&mut self, px: f32, buffer_line_count: usize)` for setting with clamping

Update all internal uses of `scroll_offset` to use the new methods.

Location: `crates/editor/src/viewport.rs`

### Step 3: Update visible_range to use first_visible_line

Modify `visible_range()` to use `self.first_visible_line()` instead of `self.scroll_offset`. This is a mechanical replacement since the derived line index has the same semantics.

Location: `crates/editor/src/viewport.rs`

### Step 4: Update scroll_to to work in pixel space

Modify `scroll_to(line, buffer_line_count)` to:
- Convert the target line to pixels: `target_px = line as f32 * self.line_height`
- Clamp to pixel bounds: `max_px = (buffer_line_count.saturating_sub(visible_lines) as f32) * line_height`
- Set `scroll_offset_px = target_px.min(max_px).max(0.0)`

This maintains the same API but operates in pixel space internally.

Location: `crates/editor/src/viewport.rs`

### Step 5: Update ensure_visible to snap to whole-line boundaries

Modify `ensure_visible(line, buffer_line_count)` to:
- Check visibility using `first_visible_line()` and `visible_lines`
- When scrolling is needed, compute the target line as before
- Set `scroll_offset_px` to a whole-line boundary: `target_line as f32 * line_height`
- This ensures that `ensure_visible` always snaps to a clean line position

The behavior is: scroll events can leave the viewport mid-line, but `ensure_visible` always snaps to a whole-line boundary.

Location: `crates/editor/src/viewport.rs`

### Step 6: Update buffer_line_to_screen_line and screen_line_to_buffer_line

These methods should use `first_visible_line()` instead of `scroll_offset`:
- `buffer_line_to_screen_line`: compare against `first_visible_line()`
- `screen_line_to_buffer_line`: add `first_visible_line()` to screen line

Location: `crates/editor/src/viewport.rs`

### Step 7: Update dirty_lines_to_region

This method uses `scroll_offset` to determine visible bounds. Update it to use `first_visible_line()`.

Location: `crates/editor/src/viewport.rs`

### Step 8: Fix all existing viewport tests

Update the existing tests to use the new API. Tests that directly set `scroll_offset` should either:
- Use `scroll_to()` which now sets `scroll_offset_px` appropriately
- Access `first_visible_line()` to check the derived line index

Run `cargo test` to ensure all existing tests pass.

Location: `crates/editor/src/viewport.rs`

### Step 9: Update BufferFocusTarget::handle_scroll to accumulate raw pixels

Modify `handle_scroll` in `crates/editor/src/buffer_target.rs`:
- Remove the integer rounding: `(delta.dy / line_height as f64).round() as i32`
- Instead, accumulate the raw delta: `scroll_offset_px += delta.dy as f32`
- Use the new clamping via `set_scroll_offset_px()` or inline clamping
- Always mark `DirtyRegion::FullViewport` since any scroll requires re-render

Location: `crates/editor/src/buffer_target.rs`

### Step 10: Add failing tests for scroll handler with sub-pixel deltas

Add tests that verify:
- A scroll delta of 5px (less than line_height 16px) changes `scroll_offset_px` but not `first_visible_line`
- Multiple sub-pixel deltas accumulate correctly
- A delta that crosses a line boundary changes `first_visible_line`

Location: `crates/editor/src/buffer_target.rs` (in the `#[cfg(test)]` module)

### Step 11: Update GlyphBuffer to accept y_offset parameter

Modify `GlyphBuffer::update_from_buffer_with_cursor` to accept an additional `y_offset: f32` parameter:
- All `position_for(row, col)` calls should have their y-coordinate adjusted by `-y_offset`
- This shifts all rendered content up by the fractional amount

Also update `GlyphLayout::position_for` or add a variant that accepts y_offset.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 12: Wire the y_offset through the renderer

Modify `Renderer::update_glyph_buffer` to pass `viewport.scroll_fraction_px()` as the y_offset to the glyph buffer update.

Location: `crates/editor/src/renderer.rs`

### Step 13: Add integration tests for smooth scrolling

Add tests that verify the full path from scroll event to rendered y-offset:
- Set up a viewport and buffer
- Apply a sub-pixel scroll delta
- Verify `scroll_fraction_px()` returns the expected value
- Verify `first_visible_line()` hasn't changed for small deltas

Location: `crates/editor/tests/viewport_test.rs`

### Step 14: Update tests that directly access scroll_offset

Search for any tests or code that directly accesses `viewport.scroll_offset` and update them to use the new methods:
- `first_visible_line()` for the integer line index
- `scroll_offset_px()` for the raw pixel value (if needed)

Run the full test suite: `cargo test`

Location: Multiple files (grep for `scroll_offset`)

### Step 15: Visual verification

Run the editor and verify:
- Trackpad scrolling is smooth (content moves sub-pixel amounts)
- Lines don't "jump" one at a time
- The top line is partially clipped when scrolled partway through
- `ensure_visible` (triggered by typing after scrolling) snaps to whole-line position
- Scroll bounds work correctly (can't scroll past start or end)

## Risks and Open Questions

- **Performance**: Computing `first_visible_line()` on every access adds a division and floor. Given the frequency of calls, this should be negligible, but worth profiling if issues arise.

- **Floating-point precision**: Accumulating many small deltas could theoretically accumulate error. Since we're dealing with display coordinates and sub-pixel precision, this is unlikely to be visible. If it becomes an issue, periodic snapping to whole-line boundaries when idle could help.

- **Selector overlay**: The selector rendering may also need y_offset adjustment if it should scroll smoothly. Currently the selector is a fixed overlay, so this is out of scope.

- **API compatibility**: Code that directly accessed `viewport.scroll_offset` will need updates. This is intentional—the field is now private and accessed via methods.

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