# Implementation Plan

## Approach

The parent chunk (`scroll_bottom_deadzone`) introduced `set_scroll_offset_px_wrapped`, which computes `max_offset_px` from total screen rows. The symptom reported is that the deadzone has become *larger*, meaning `max_offset_px` is *over-estimated* — the scroll position is allowed to go further than the renderer actually renders.

### Root Cause Analysis

After careful examination of the code, the issue is a **unit mismatch** between:

1. **`compute_total_screen_rows`** in `viewport.rs` — sums `screen_rows_for_line(line_len)` across all buffer lines
2. **The renderer** in `glyph_buffer.rs` — uses the same calculation, but tracks `cumulative_screen_row` relative to the viewport top

The key insight is in how the renderer handles the **first visible buffer line**:

```rust
// In glyph_buffer.rs render loop:
let (first_visible_buffer_line, screen_row_offset_in_line, _) =
    Viewport::buffer_line_for_screen_row(first_visible_screen_row, ...);

// When rendering the first visible line:
let start_row_offset = if is_first_buffer_line {
    screen_row_offset_in_line  // Skip rows above viewport
} else {
    0
};

// cumulative_screen_row increments by: rows_for_line - start_row_offset
```

The renderer correctly skips `screen_row_offset_in_line` rows of the first visible line because they're above the viewport. However, `compute_total_screen_rows` counts *all* rows for *all* lines.

**The bug**: When computing `max_offset_px`, we use:
```
max_offset_px = (total_screen_rows - visible_lines) * line_height
```

This is correct! The issue is elsewhere. Let me re-examine...

Actually, looking more carefully at `pixel_to_buffer_position_wrapped`:

```rust
// Walk buffer lines from first_visible_line, summing screen rows
for buffer_line in first_visible_line..line_count {
    let rows_for_line = wrap_layout.screen_rows_for_line(line_len);
    if target_screen_row < next_cumulative {
        // Found it
    }
    cumulative_screen_row = next_cumulative;
}
```

This walks from `first_visible_line` (a **buffer line index**), but `first_visible_line` is computed from `first_visible_row()` which is `floor(scroll_offset_px / line_height)`. In wrapped mode, `scroll_offset_px` is in **screen row** space, so `first_visible_line()` returns the **screen row index**, not the buffer line index!

Wait, that's not right either. Let me check the code again...

Actually, `viewport.first_visible_line()` just delegates to `scroller.first_visible_row()`, which computes `floor(scroll_offset_px / row_height)`. This gives us the first visible **screen row** index. But `pixel_to_buffer_position_wrapped` uses this as if it were a **buffer line** index.

**The actual bug in `pixel_to_buffer_position_wrapped`**:

```rust
for buffer_line in first_visible_line..line_count {  // <-- WRONG: using screen row as buffer line
```

But wait — I see there's also `buffer_line_for_screen_row` in the renderer which correctly maps screen row to buffer line. Let me check if `pixel_to_buffer_position_wrapped` should be using that...

Looking at the glyph_buffer.rs renderer:
```rust
let first_visible_screen_row = viewport.first_visible_screen_row();
let (first_visible_buffer_line, screen_row_offset_in_line, _) =
    Viewport::buffer_line_for_screen_row(first_visible_screen_row, ...);
```

But `pixel_to_buffer_position_wrapped` in buffer_target.rs:
```rust
for buffer_line in first_visible_line..line_count {  // first_visible_line is passed in
```

Where `first_visible_line` comes from:
```rust
ctx.viewport.first_visible_line(),  // <-- This returns first_visible_row() which is screen row!
```

**THIS IS THE BUG!** In wrapped mode, `first_visible_line()` returns the first visible **screen row** index, but `pixel_to_buffer_position_wrapped` interprets it as a **buffer line** index.

However, this would cause clicks to land on the wrong buffer line, not a scroll deadzone. Let me re-examine the scroll clamping issue...

**Re-examining `compute_total_screen_rows` vs actual rendered content:**

The issue is more subtle. When we have:
- 10 buffer lines
- Line 0: 160 chars (2 screen rows at 80 cols)
- Lines 1-9: 40 chars each (1 screen row each)
- Total: 11 screen rows

With 5 visible rows:
- `max_offset_px = (11 - 5) * line_height = 6 * line_height`
- At max scroll, `first_visible_screen_row = 6`

At screen row 6, we should see screen rows 6, 7, 8, 9, 10 (5 rows).
- Screen row 0-1 = buffer line 0
- Screen rows 2-10 = buffer lines 1-9

Screen row 6 corresponds to buffer line 5 (rows 2-10 map to lines 1-9, so row 6 = line 5).

The renderer will:
1. Call `buffer_line_for_screen_row(6)` → (5, 0, 6)
2. Render from buffer line 5 onwards, showing lines 5, 6, 7, 8, 9
3. That's 5 lines = 5 screen rows ✓

This seems correct. Let me look for the actual discrepancy...

**Found it!** The issue is in how `visible_lines()` is computed vs how the renderer counts visible screen rows:

In `RowScroller::update_size`:
```rust
self.visible_rows = (height_px / self.row_height).floor() as usize
```

But the renderer uses `max_screen_rows = viewport.visible_lines() + 2` for partial visibility.

The max offset formula uses `visible_lines()` (without +2), which is correct for clamping.

**Real root cause — looking at the symptom more carefully:**

The symptom says "the viewport stops visually scrolling well before reaching the actual bottom of the file". This means content is still below, but we can't scroll to see it.

If `max_offset_px` is computed correctly, then either:
1. The `visible_lines()` is too large (making max_offset too small)
2. The `total_screen_rows` is too small
3. There's a different calculation elsewhere that's wrong

Wait — I think I found it! Looking at `pixel_to_buffer_position_wrapped` again:

```rust
for buffer_line in first_visible_line..line_count {
```

**`first_visible_line` is passed to this function, and the caller passes `ctx.viewport.first_visible_line()`**

But `first_visible_line()` in wrapped mode returns the **screen row** index, not the buffer line index! The function then iterates buffer lines starting from a screen row index, which is almost always larger than the actual first visible buffer line when there are wrapped lines.

If screen row 6 is the first visible, but we start iterating buffer lines from index 6 (instead of buffer line 5), we'll:
- Skip buffer lines 0-5 in the iteration
- Only sum screen rows for lines 6-9 (4 lines)
- Map clicks incorrectly

This would cause clicks to land on the wrong line, which matches the symptom "cursor appearing ~10 lines below the click point".

**The fix**: The callers of `pixel_to_buffer_position_wrapped` should use `buffer_line_for_screen_row` to convert the first visible screen row to a buffer line before passing it.

### Summary of Issues

1. **`pixel_to_buffer_position_wrapped` receives wrong first visible line**: Callers pass `first_visible_line()` (which is a screen row index in wrapped mode) but the function expects a buffer line index.

2. **Scroll clamping is likely correct**: The `set_scroll_offset_px_wrapped` uses `compute_total_screen_rows` which correctly sums all screen rows. The deadzone symptom may be secondary to the click misalignment.

### Fix Strategy

1. **Fix `pixel_to_buffer_position_wrapped`**: Either:
   - Change the function to accept a screen row and convert internally, or
   - Fix callers to pass the actual first visible buffer line

2. **Add regression tests**: Create tests that verify click-to-cursor mapping and scroll behavior match the renderer's actual coordinate system.

3. **Verify scroll clamping**: After fixing the coordinate mismatch, verify that scroll clamping produces the correct max offset.


## Sequence

### Step 1: Write failing tests for click-to-cursor at max scroll (TDD red phase)

Create tests that demonstrate the coordinate mismatch in `pixel_to_buffer_position_wrapped`. Tests should:

1. Set up a wrapped file where total screen rows > buffer lines
2. Scroll to maximum position
3. Click at specific screen coordinates
4. Verify cursor lands on the expected buffer line/column

**Expected behavior**: These tests should fail initially because `first_visible_line` is being passed as a screen row index rather than a buffer line index.

**Location**: `crates/editor/src/buffer_target.rs` (in `#[cfg(test)]` module)

```rust
#[test]
fn test_click_at_max_scroll_wrapped_maps_correctly() {
    // 5 buffer lines, line 0 has 160 chars (2 screen rows at 80 cols)
    // Total: 6 screen rows
    // Viewport: 3 visible rows
    // Max scroll: screen row 3 (6 - 3 = 3)

    // At screen row 3, we should see buffer line 2 (since line 0 takes rows 0-1, line 1 is row 2)
    // Clicking on screen row 0 of the viewport should place cursor on buffer line 2
    // NOT buffer line 3 (which would happen if we iterate from buffer line 3)
}
```

### Step 2: Write failing tests for scroll responsiveness at wrapped max (TDD red phase)

Create tests that verify scroll input at the maximum position responds immediately.

**Location**: `crates/editor/src/buffer_target.rs` (in `#[cfg(test)]` module)

The existing test `test_scroll_at_max_wrapped_responds_to_scroll_up` should already pass if the scroll clamping is correct. If it fails, that indicates a problem in `set_scroll_offset_px_wrapped` or `compute_total_screen_rows`.

### Step 3: Fix `pixel_to_buffer_position_wrapped` coordinate handling

The function receives `first_visible_line` which in wrapped mode is actually the first visible **screen row** index. The fix should convert this to the actual first visible buffer line.

**Option A** (preferred): Fix at the function level
```rust
fn pixel_to_buffer_position_wrapped<F>(
    position: (f64, f64),
    view_height: f32,
    wrap_layout: &WrapLayout,
    scroll_fraction_px: f32,
    first_visible_screen_row: usize,  // Rename parameter to be clearer
    line_count: usize,
    line_len_fn: F,
) -> Position
where
    F: Fn(usize) -> usize,
{
    // Convert first visible screen row to first visible buffer line
    let (first_buffer_line, screen_row_offset_in_first_line, _) =
        Viewport::buffer_line_for_screen_row(
            first_visible_screen_row,
            line_count,
            wrap_layout,
            &line_len_fn,
        );

    // Walk buffer lines starting from the actual first visible buffer line
    for buffer_line in first_buffer_line..line_count {
        // ... rest of the iteration logic
    }
}
```

**Additional consideration**: The cumulative screen row tracking also needs to account for `screen_row_offset_in_first_line` — the rows of the first buffer line that are above the viewport should not be counted in the cumulative sum.

**Location**: `crates/editor/src/buffer_target.rs`

### Step 4: Verify and update callers

The callers of `pixel_to_buffer_position_wrapped` currently pass:
- `ctx.viewport.first_visible_line()` — which returns `first_visible_row()` = `floor(scroll_offset_px / line_height)`

After the fix in Step 3, the function will internally convert this screen row to a buffer line, so callers should continue to pass `first_visible_screen_row()` (or `first_visible_line()`, which is the same value).

**Verify callers**:
- `MouseEventKind::Down` handler
- `MouseEventKind::Moved` handler

Both should use `ctx.viewport.first_visible_line()` as the screen row input.

### Step 5: Run tests and verify fix (TDD green phase)

1. Run the new tests from Steps 1-2 — they should now pass
2. Run all existing viewport, scroll, and buffer_target tests
3. Run full test suite: `cargo test -p lite-edit-editor`

### Step 6: Investigate scroll deadzone symptom

If the click-to-cursor fix doesn't resolve the scroll deadzone symptom, investigate further:

1. **Verify `compute_total_screen_rows`**: Add a test that compares the computed total against a manually calculated expected value for a specific wrapped file.

2. **Check `visible_lines()` computation**: Ensure the viewport height used for `visible_lines` matches the wrap layout width used for wrapping. A mismatch could cause the max offset calculation to be off.

3. **Consider edge cases**:
   - Empty lines (should take 1 screen row)
   - Lines that wrap to exactly N rows (no rounding error)
   - Very long lines with many wrap rows

### Step 7: Add regression test for screen row count consistency

Create a test that verifies `compute_total_screen_rows` produces the same count as manually walking the buffer with `screen_rows_for_line`.

**Location**: `crates/editor/src/viewport.rs`

```rust
#[test]
fn test_compute_total_screen_rows_matches_manual_count() {
    // Create a known wrapped file
    // Manually compute expected screen rows
    // Verify compute_total_screen_rows returns the same value
}
```

### Step 8: Update code_paths and code_references in GOAL.md

After implementation, update the frontmatter:
- `code_paths`: Files that were modified
- `code_references`: Symbolic references to key code locations

## Dependencies

This chunk depends on `scroll_bottom_deadzone` (the parent chunk), which introduced:
- `set_scroll_offset_px_wrapped` in `Viewport`
- `compute_total_screen_rows` helper
- Routing of `handle_scroll` through wrap-aware clamping

These are already implemented and merged.

## Risks and Open Questions

1. **Root cause uncertainty**: The analysis identifies a coordinate mismatch in `pixel_to_buffer_position_wrapped`, but this primarily affects click-to-cursor mapping. The scroll deadzone symptom may have a different or additional root cause. If fixing the coordinate mismatch doesn't resolve the scroll issue, further investigation (Step 6) will be needed.

2. **Performance of `buffer_line_for_screen_row`**: The fix adds a call to `buffer_line_for_screen_row` in the click handler hot path. This iterates buffer lines from 0 to find the target. For large files at high scroll positions, this could add latency. However:
   - Click events are infrequent relative to rendering
   - The same function is already called in the render path
   - If profiling shows issues, we could cache the first visible buffer line

3. **Test isolation**: The tests depend on specific wrap widths and line lengths. Changes to font metrics or wrap calculation could affect test validity. Tests should use explicit, known configurations.

4. **Interaction with `ensure_visible_wrapped`**: This function also uses coordinate mapping. After the fix, verify that cursor-following scrolling still works correctly.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->