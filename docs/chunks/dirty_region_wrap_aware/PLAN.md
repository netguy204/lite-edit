<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is a type mismatch: `dirty_lines_to_region()` treats `DirtyLines` indices as screen rows, but they're actually buffer line indices. With soft wrapping, one buffer line can occupy multiple screen rows, so a buffer line index will diverge from its cumulative screen row index as scroll position increases.

**Strategy**: Add a wrap-aware overload `dirty_lines_to_region_wrapped()` that:
1. Converts buffer line indices to absolute screen row indices using the existing `WrapLayout::screen_rows_for_line()` helper
2. Compares those screen row indices against the viewport's screen-row-based scroll position
3. Produces correct `DirtyRegion` output even when buffer line indices are much smaller than screen row indices

**Why a new method instead of modifying the existing one?**

The existing `dirty_lines_to_region(&self, dirty: &DirtyLines, buffer_line_count: usize)` signature doesn't have access to `WrapLayout` or line lengths. Adding those parameters would break all callers and make the method signatures large. Instead:
- Keep `dirty_lines_to_region` for the common no-wrap case (it's still correct when screen rows ≈ buffer lines)
- Add `dirty_lines_to_region_wrapped` for wrap-aware callers
- Update `EditorContext::mark_dirty()` and `EditorState::cursor_dirty_region()` to use the wrapped variant when wrapping is enabled

**Testing approach** (per TESTING_PHILOSOPHY.md):
- Write failing tests first that reproduce the bug: a buffer line visible on screen but with buffer index < first_visible_screen_row should produce a non-None dirty region
- The tests will use `WrapLayout` and viewport methods to set up the wrap-aware scenario

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS a wrap-aware extension to the dirty region conversion pattern. The subsystem documents that `dirty_lines_to_region` bridges `DirtyLines` to `DirtyRegion`; we're adding a wrap-aware variant that follows the same pattern but accounts for screen-row accumulation.

No deviations discovered—the existing code follows the subsystem's patterns correctly for the unwrapped case.

## Sequence

### Step 1: Write failing tests for wrap-aware dirty region conversion

Create tests in `viewport.rs` that reproduce the bug:

1. **test_dirty_single_visible_wrapped**: Set up a viewport scrolled to screen row ~400 with lines that wrap heavily. A buffer line 250 that is visible on screen (because its cumulative screen rows place it in view) should produce a non-None dirty region.

2. **test_dirty_range_wrapped**: Similar test for `DirtyLines::Range` with wrapping.

3. **test_dirty_from_line_to_end_wrapped**: Test that `FromLineToEnd` works correctly when buffer line index is below first_visible_screen_row but the dirty region still overlaps the visible screen rows.

These tests will fail initially because no `dirty_lines_to_region_wrapped` method exists.

Location: `crates/editor/src/viewport.rs` test module

### Step 2: Implement `dirty_lines_to_region_wrapped`

Add a new method to `Viewport`:

```rust
/// Converts buffer-space `DirtyLines` to screen-space `DirtyRegion` with soft wrapping.
///
/// Unlike `dirty_lines_to_region`, this method accounts for the fact that
/// buffer lines may wrap to multiple screen rows. It computes the cumulative
/// screen row for each dirty buffer line and compares against the viewport's
/// screen-row-based scroll position.
pub fn dirty_lines_to_region_wrapped<F>(
    &self,
    dirty: &DirtyLines,
    line_count: usize,
    wrap_layout: &WrapLayout,
    line_len_fn: F,
) -> DirtyRegion
where
    F: Fn(usize) -> usize,
```

Implementation approach:
1. Use `first_visible_screen_row()` to get the current scroll position in screen rows
2. For each dirty buffer line, compute its absolute screen row by summing `screen_rows_for_line()` for all preceding lines
3. Compute `visible_screen_rows` = `visible_lines()` (in screen row terms)
4. Intersect the dirty screen row range with `[first_visible_screen_row, first_visible_screen_row + visible_screen_rows)`
5. Return the appropriate `DirtyRegion` variant

The helper `buffer_line_to_screen_row` (cumulative sum) will be factored out for reuse.

Location: `crates/editor/src/viewport.rs`

### Step 3: Add helper for buffer line → absolute screen row conversion

Add a helper method or inline logic to convert a buffer line index to its absolute screen row:

```rust
/// Computes the absolute screen row where a buffer line starts.
///
/// This is the cumulative sum of screen rows for all preceding buffer lines.
fn buffer_line_to_abs_screen_row<F>(
    buffer_line: usize,
    wrap_layout: &WrapLayout,
    line_len_fn: F,
) -> usize
where
    F: Fn(usize) -> usize,
```

This can be a static method since it doesn't need `&self`. It will be used by `dirty_lines_to_region_wrapped` to convert dirty buffer line indices to screen row indices.

Location: `crates/editor/src/viewport.rs`

### Step 4: Update `EditorContext::mark_dirty` to use wrap-aware conversion

Modify `EditorContext::mark_dirty()` to call `dirty_lines_to_region_wrapped` instead of `dirty_lines_to_region`. The context already has access to `wrap_layout()` and `buffer.line_len()`.

```rust
pub fn mark_dirty(&mut self, dirty_lines: DirtyLines) {
    let line_count = self.buffer.line_count();
    let wrap_layout = self.wrap_layout();
    let screen_dirty = self.viewport.dirty_lines_to_region_wrapped(
        &dirty_lines,
        line_count,
        &wrap_layout,
        |line| self.buffer.line_len(line),
    );
    self.dirty_region.merge(screen_dirty);
}
```

Location: `crates/editor/src/context.rs`

### Step 5: Update `EditorState::cursor_dirty_region` to use wrap-aware conversion

The `cursor_dirty_region()` method in `editor_state.rs` also calls `dirty_lines_to_region` with a buffer line index. Update it to use the wrap-aware variant.

This method needs access to `WrapLayout` and line lengths. The `EditorState` has access to:
- `font_metrics` (for `WrapLayout::new`)
- `viewport_size.width` (for `WrapLayout::new`)
- `buffer.line_len()` (for line lengths)

Location: `crates/editor/src/editor_state.rs`

### Step 6: Verify existing tests still pass

Run the existing `dirty_lines_to_region` tests to ensure they still pass. They test the unwrapped case where screen rows ≈ buffer lines, and should continue to work because:
- Either the tests use the old method (which remains correct for unwrapped)
- Or the tests use the new method with short lines (where cumulative screen rows ≈ buffer lines)

Run: `cargo test -p lite-edit-editor`

### Step 7: Add integration test for mouse click at scroll position with heavy wrapping

Create a test that simulates the original bug scenario:
1. Create a buffer with many long lines (>200 chars each)
2. Scroll to a position where cumulative screen rows >> buffer line index
3. Simulate a mouse click on a visible line
4. Assert that `dirty_region` is non-None

This verifies the end-to-end fix through `mark_cursor_dirty()`.

Location: `crates/editor/src/context.rs` or `crates/editor/src/buffer_target.rs` test module

### Step 8: Add backreference comment to new method

Add chunk backreference to `dirty_lines_to_region_wrapped`:

```rust
// Chunk: docs/chunks/dirty_region_wrap_aware - Wrap-aware dirty region conversion
pub fn dirty_lines_to_region_wrapped<F>(...) -> DirtyRegion { ... }
```

Location: `crates/editor/src/viewport.rs`

## Risks and Open Questions

1. **Performance**: Computing cumulative screen rows for each dirty line is O(buffer_line), not O(1). For very long documents with many dirty lines, this could be slow. However:
   - `DirtyLines::Single` (the most common case for cursor movement) only needs one cumulative sum
   - The existing `ensure_visible_wrapped` already does similar O(n) iteration
   - This is consistent with the subsystem's "no cache, pure arithmetic" principle from `WrapLayout`

2. **Backward compatibility**: The old `dirty_lines_to_region` method signature is preserved. Callers that don't need wrap awareness continue to work. However, if a caller should have been using the wrap-aware version, they'll see the existing bug. Consider whether to deprecate the unwrapped version or add a lint.

3. **Edge cases**: Need to handle:
   - Empty buffer (line_count = 0)
   - Buffer line index >= line_count
   - Viewport not initialized (visible_lines = 0)

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
