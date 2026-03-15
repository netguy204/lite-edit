# Implementation Plan

## Approach

The bug is a coordinate-space confusion: `ensure_visible_wrapped` receives
`first_visible_line()` (which returns a screen-row-derived value in wrapped
mode) as its `first_visible_line` parameter, then uses it as a buffer line index
in its accumulation loop. This causes the loop to start at the wrong buffer line
and under-count cumulative screen rows, producing incorrect scroll targets.

The fix has two parts:

1. **Remove `first_visible_line` as a parameter to `ensure_visible_wrapped`.**
   The function already computes its own max and does its own clamping. It
   doesn't actually need an external "first visible line" — it needs to know the
   cursor's absolute screen row position, which it can compute by iterating from
   buffer line 0. The current code only iterates from `first_visible_line` as an
   optimization (to avoid re-counting rows already scrolled past), but this
   optimization is the source of the bug. Replace the partial loop with a
   full loop from buffer line 0 to the cursor line, computing the absolute
   screen row of the cursor. This is O(line_count) but that's already the cost
   of `compute_total_screen_rows` called in the same function.

2. **Update all call sites** (`editor_state.rs`, `context.rs`, and tests) to
   stop passing `first_visible_line()`.

This follows the viewport_scroll subsystem's pattern: `ensure_visible_wrapped`
does its own clamping (Soft Convention 1), so it should also do its own
coordinate computation rather than trusting a caller-provided value.

Tests follow the TDD approach from TESTING_PHILOSOPHY.md: write failing tests
first that demonstrate the coordinate confusion, then apply the fix.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS a fix
  to `Viewport::ensure_visible_wrapped`, which is a core method in the
  subsystem. The fix strengthens Hard Invariant 1 (scroll_offset_px as single
  source of truth) by ensuring `ensure_visible_wrapped` computes cursor position
  from buffer-line-0 rather than depending on a caller-provided value that may
  be in the wrong coordinate space.

## Sequence

### Step 1: Write failing tests

Location: `crates/editor/src/viewport.rs` (in the `#[cfg(test)]` module)

Write tests that demonstrate the bug by constructing scenarios where
`first_visible_line()` returns a screen row number that differs from the actual
buffer line index:

**Test A: Cursor below viewport with wrapped lines above.**
Set up a viewport with 10 visible rows. Create a buffer with 5 lines where
lines 0-2 each wrap to 3 screen rows (e.g., 250 chars at 100 cols/row). Place
cursor at buffer line 4. If we incorrectly pass screen row N (large) as
`first_visible_line`, the accumulation loop starts too late and under-counts,
causing wrong scroll target. The test asserts correct scroll position.

**Test B: Cursor above viewport with wrapped lines below.**
Scroll the viewport down past several wrapped lines, then move cursor to line 0.
The `cursor_line < first_visible_line` branch fires. Assert that scrolling up
places the cursor at the correct position (screen row 0).

**Test C: Cursor on a continuation row of a wrapped line.**
Place cursor at a column that falls on the second screen row of a wrapped line.
Assert the viewport scrolls to make that specific continuation row visible.

**Test D: Non-wrapped document (regression guard).**
All lines fit in one screen row. Assert that behavior is unchanged from the
current implementation (screen rows == buffer lines, so the bug doesn't
manifest, and the fix shouldn't regress).

### Step 2: Remove `first_visible_line` parameter from `ensure_visible_wrapped`

Location: `crates/editor/src/viewport.rs`

Change the signature of `ensure_visible_wrapped` to remove the
`first_visible_line: usize` parameter.

Replace the accumulation loop body:

**Before** (lines 319-322):
```rust
for buffer_line in first_visible_line..cursor_line.min(line_count) {
    let line_len = line_len_fn(buffer_line);
    cumulative_screen_row += wrap_layout.screen_rows_for_line(line_len);
}
```

**After**: Always iterate from buffer line 0 to compute the absolute screen row:
```rust
for buffer_line in 0..cursor_line.min(line_count) {
    let line_len = line_len_fn(buffer_line);
    cumulative_screen_row += wrap_layout.screen_rows_for_line(line_len);
}
```

Also simplify the `cursor_line < first_visible_line` branch — since we now
always compute the absolute screen row from line 0, the "scroll up" path and
"scroll down" path can share the same computation. The absolute cursor screen
row is `cumulative_screen_row + cursor_row_offset`. Then:

- If `cursor_screen_row < current_top_screen_row`: scroll up (set
  `scroll_offset_px = cursor_screen_row * line_height`).
- If `cursor_screen_row > current_top_screen_row + visible_lines`: scroll down
  (set `scroll_offset_px = (cursor_screen_row - visible_lines + 1) * line_height`).
- Otherwise: cursor is visible, no scroll needed.

The "current top screen row" is derived from the existing `scroll_offset_px`
via `first_visible_screen_row()`.

Add backreference comment:
```rust
// Chunk: docs/chunks/wrap_scroll_to_cursor - Fix coordinate space: always compute from buffer line 0
// Subsystem: docs/subsystems/viewport_scroll - Wrap-aware cursor-following scroll
```

### Step 3: Update call sites

**Location: `crates/editor/src/editor_state.rs` (~line 4341)**

Remove `tab.viewport.first_visible_line()` from the gathered tuple and from the
call to `ensure_visible_wrapped`. The function no longer takes that parameter.

**Location: `crates/editor/src/context.rs` (~line 156)**

Remove `let first_visible_line = self.viewport.first_visible_line();` and remove
the parameter from the `ensure_visible_wrapped` call.

### Step 4: Update existing test

Location: `crates/editor/src/viewport.rs`, test
`test_ensure_visible_wrapped_partial_row_should_not_scroll` (~line 2558)

Remove the `first_visible_line` argument (third positional arg, value `0`) from
both `ensure_visible_wrapped` calls in this test.

### Step 5: Run tests and verify

Run `cargo test -p editor` to ensure:
- All new tests pass (Step 1 tests, which were written to fail before the fix,
  now pass)
- The existing partial-row test still passes
- No regressions in other viewport/scroll tests

## Risks and Open Questions

- **Performance of full iteration from line 0**: `ensure_visible_wrapped` will
  now always iterate from buffer line 0 to the cursor line. For very large files
  (100K+ lines), this is O(N) per cursor movement. However,
  `compute_total_screen_rows` (called in the same function) already iterates all
  lines, so the total cost only increases by a constant factor. If this becomes
  a bottleneck, a prefix-sum cache could be added later, but that's out of scope
  for this bug fix.

- **`pixel_to_buffer_position_wrapped` in editor_state.rs:3083**: This call site
  passes `viewport.first_visible_line()` where the parameter is named
  `first_visible_screen_row`. This is a potential related coordinate confusion
  for click handling in wrapped mode, but it's a separate bug (click position,
  not scroll-to-cursor) and out of scope for this chunk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
