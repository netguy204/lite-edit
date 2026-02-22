# Implementation Plan

## Approach

This chunk fixes two interrelated bugs at the scroll-bottom boundary:

1. **Phantom scroll region (deadzone)**: The max scroll offset computation allows offsets past the actual renderable content, creating a gap where scroll input is consumed with no visual effect.

2. **Click-to-cursor misalignment**: When in the phantom region, click hit-testing and rendering disagree on which buffer line corresponds to a given screen position.

### Root Cause Analysis

**The 1-line phantom region without wrapping:**
In `RowScroller::set_scroll_offset_px`, the max is computed as:
```
max_offset_px = (row_count - visible_rows) * row_height
```
This formula is correct, but the issue is that `visible_rows` is computed as `floor(viewport_height / row_height)`, which may truncate fractional rows. If the viewport height isn't an exact multiple of row height, the actual renderable area can display a partial row at the bottom. The max offset formula doesn't account for this, allowing the viewport to scroll ~1 line past what's actually rendered.

**The larger phantom region with wrapping:**
`set_scroll_offset_px_wrapped` computes total screen rows by summing `screen_rows_for_line` for all buffer lines. This sum appears correct. However, the issue is that `visible_lines()` (which equals `visible_rows` from the scroller) doesn't account for the fact that with wrapping enabled, we're in screen-row space. The `max_rows = total_screen_rows - visible_lines` subtraction should work, but if total_screen_rows is overcounted, the deadzone grows proportionally.

**The click-to-cursor misalignment:**
Looking at `pixel_to_buffer_position_wrapped`, the function receives `first_visible_screen_row` and uses `Viewport::buffer_line_for_screen_row` to map screen rows to buffer lines. The bug is that when the scroll offset is clamped to a value higher than the renderer can actually display, the `first_visible_screen_row` value is beyond what the renderer is showing. The renderer clamps its own draw loop to available content, but hit-testing doesn't know about this clamping.

### Fix Strategy

The fix addresses both the scroll clamping and hit-testing from a unified perspective:

1. **Audit `visible_rows` / `visible_lines`**: Ensure the count of visible rows accounts for the actual viewport height in a way that aligns with what the renderer draws.

2. **Make max scroll offset match renderer bounds**: The max scroll offset should be the minimum value that allows the last content to be visible, not the value computed from `total - visible`. This means ensuring the formulas agree on boundaries.

3. **Add tests that detect disagreement**: Create tests that verify scroll position, hit-testing, and rendering all agree at the maximum scroll position.

Following the testing philosophy in TESTING_PHILOSOPHY.md: we'll write failing tests first for the boundary conditions (scrolling to max shows last line, clicking at max positions cursor correctly), then fix the implementation.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS fixes to the scroll clamping logic within this subsystem. The subsystem's invariants (especially "Scroll offset is always clamped to `[0.0, max_offset_px]`") remain valid, but the computation of `max_offset_px` needs correction.

The subsystem is DOCUMENTED (not REFACTORING), so we focus on fixing the specific bugs rather than broader refactoring. However, since we are directly fixing scroll clamping within the subsystem, any improvements we make should align with the documented invariants.

## Sequence

### Step 1: Write failing tests for scroll boundary behavior

Create tests that capture the exact symptoms described in the GOAL.md:

1. **test_scroll_to_max_no_wrapping_shows_last_line**: With no wrapped lines, scroll to max and verify:
   - The last buffer line is visible
   - No additional scroll input is accepted (offset doesn't change when trying to scroll further)
   - Scrolling back up responds immediately (offset decreases by the delta)

2. **test_scroll_to_max_with_wrapping_shows_last_line**: With wrapped lines, scroll to max and verify:
   - The last screen row of the last buffer line is visible
   - No additional scroll input is accepted
   - Scrolling back up responds immediately

3. **test_click_at_max_scroll_no_wrapping_hits_correct_line**: At max scroll without wrapping:
   - Click at each visible line position
   - Verify cursor lands on the expected buffer line (not offset by 1)

4. **test_click_at_max_scroll_with_wrapping_hits_correct_line**: At max scroll with wrapping:
   - Click at each visible screen row
   - Verify cursor lands on the expected buffer line

Location: `crates/editor/src/viewport.rs` (unit tests) and `crates/editor/src/buffer_target.rs` (integration tests)

### Step 2: Investigate the off-by-one in non-wrapped mode

Debug why there's a 1-line phantom region even without wrapping. This requires:

1. Print/trace `visible_rows`, `row_count`, computed `max_offset_px`
2. Compare with the actual content height the renderer would draw
3. Identify if the issue is in `visible_rows` computation or the max formula

The hypothesis is that `visible_rows = floor(viewport_height / row_height)` may be underestimating when the viewport can show a partial row. If the renderer draws rows `0..visible_rows+1` (for partial visibility), but clamping uses just `visible_rows`, there's a mismatch.

Location: `crates/editor/src/row_scroller.rs`

### Step 3: Fix the non-wrapped scroll clamping

Based on Step 2's findings, correct the max offset formula. The fix likely involves one of:

a) Using `visible_rows + 1` in the max computation when there's a fractional row
b) Adjusting how `visible_rows` is computed to include the partial row
c) Computing max as `(total_height - viewport_height)` in pixels, not rows

The fix must preserve the invariant that at max scroll, the last line is at the bottom of the viewport.

Location: `crates/editor/src/row_scroller.rs#set_scroll_offset_px`

### Step 4: Verify non-wrapped tests pass

Run the non-wrapped tests from Step 1. If they pass, proceed. If not, debug and iterate.

### Step 5: Investigate the wrapped-mode clamping

With the non-wrapped case fixed, examine the wrapped case:

1. Verify `compute_total_screen_rows` agrees with what the renderer draws
2. Check that the max offset formula `(total_screen_rows - visible_lines) * line_height` is correct
3. Look for discrepancies between visible_lines (which is in buffer lines for some uses) and visible_rows (which should be screen rows)

Location: `crates/editor/src/viewport.rs#set_scroll_offset_px_wrapped`

### Step 6: Fix the wrapped-mode scroll clamping

Apply the same fix pattern as Step 3, but for `set_scroll_offset_px_wrapped`. The key insight is that in wrapped mode, all units should be in screen rows:

- `total_screen_rows`: sum of screen rows for all buffer lines
- `visible_rows`: number of screen rows that fit in viewport (this is what we already have)
- `max_offset = (total_screen_rows - visible_rows) * line_height`

If the non-wrapped fix changed how `visible_rows` is interpreted, apply the same change here.

Location: `crates/editor/src/viewport.rs#set_scroll_offset_px_wrapped`

### Step 7: Verify wrapped tests pass

Run the wrapped tests from Step 1. If they pass, proceed. If not, debug and iterate.

### Step 8: Address click-to-cursor mapping

With scroll clamping fixed, verify click-to-cursor:

1. Run the click tests from Step 1
2. If they fail, the issue is in `pixel_to_buffer_position_wrapped`
3. The likely fix: ensure `first_visible_screen_row` aligns with what the renderer actually draws at max scroll

The semantic issue: `first_visible_screen_row` should be what the renderer uses for its first visible row, not an abstract computation. At max scroll, if the clamped offset means "last content row at viewport bottom", then `first_visible_screen_row` should reflect that.

Location: `crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped` and `crates/editor/src/buffer_target.rs#handle_mouse`

### Step 9: Run all existing tests

Ensure no regressions:
```
cargo test -p lite-edit-editor
```

All viewport, scroll, and buffer_target tests should pass.

### Step 10: Manual verification

Create a test document that exercises the bug scenarios:
1. A file with ~50 lines that fits in the viewport with room to scroll
2. Resize to force line wrapping
3. Scroll to bottom — verify last line visible, no deadzone
4. Click at various positions — verify cursor placement is correct

## Dependencies

No external dependencies. This chunk builds on the existing viewport_scroll subsystem infrastructure.

## Risks and Open Questions

1. **Partial row rendering**: The renderer may include an extra partial row at the bottom of the viewport. Does the scroll clamping need to account for this, or should it assume whole rows only? The current tests suggest the `+1` row in `visible_range` handles this, but scroll clamping may not.

2. **Interaction with `ensure_visible_wrapped`**: This method does its own scroll clamping via `set_scroll_offset_unclamped`. If we change the max offset formula, we need to verify `ensure_visible_wrapped` still behaves correctly.

3. **Terminal scrollback**: The terminal uses `Viewport::is_at_bottom` and `scroll_to_bottom`. Changes to scroll bounds may affect terminal auto-follow behavior. Need to verify terminal scrolling still works after the fix.

4. **Performance**: The current `compute_total_screen_rows` iterates over all buffer lines. This is O(n) per scroll event. For large files, this could be noticeable. If profiling shows issues, we may need to cache the total or use incremental updates.

## Deviations

(To be populated during implementation)