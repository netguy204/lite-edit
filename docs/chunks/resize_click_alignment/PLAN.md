<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause is well-understood from the GOAL.md:

1. When the viewport resizes (e.g., entering fullscreen), `update_viewport_dimensions`
   calls `Viewport::update_size`, which updates `visible_rows` inside the `RowScroller`.

2. However, the existing `scroll_offset_px` is **not re-clamped** to the new valid
   bounds. After a resize that increases the viewport height, `visible_rows` grows,
   which decreases `max_offset_px = (row_count - visible_rows) * row_height`.

3. If the previous `scroll_offset_px` exceeds the new `max_offset_px`, the
   `first_visible_line()` derivation (`floor(scroll_offset_px / line_height)`)
   returns a value larger than what the renderer actually draws, causing click
   misalignment.

**The fix** is minimal:

- Add a `row_count` parameter to `RowScroller::update_size` (and propagate through
  `Viewport::update_size` and `EditorState::update_viewport_dimensions`).
- After computing the new `visible_rows`, call `set_scroll_offset_px` with the
  current offset to re-clamp it to the new valid bounds.

This follows the **Humble View Architecture** pattern (Decision 002): the fix is
pure state manipulation inside `RowScroller`, fully testable without platform
mocks. We add a unit test that simulates the problematic scenario and verifies
the scroll offset is clamped correctly.

Per **TESTING_PHILOSOPHY.md**, we write the failing test first (red), then
implement the fix (green), then verify existing tests still pass.

## Subsystem Considerations

No subsystems directory exists in this project. This chunk does not touch any
existing cross-cutting patterns.

## Sequence

### Step 1: Add failing regression test to `RowScroller`

Add a test to `row_scroller.rs` that:

1. Creates a `RowScroller` with `row_height = 16.0`.
2. Calls `update_size(160.0)` → 10 visible rows.
3. Sets scroll to near max for a 100-row buffer: `scroll_to(90, 100)`.
   (This puts `scroll_offset_px = 1440.0`, `first_visible_row = 90`.)
4. Simulates a resize that **increases** viewport height: `update_size(320.0, 100)`.
   Now there are 20 visible rows, so `max_offset_px = (100 - 20) * 16 = 1280`.
5. Asserts that `scroll_offset_px` was clamped to `1280.0` (not still `1440.0`).
6. Asserts that `first_visible_row()` returns `80`, not `90`.

The test should fail initially because `update_size` currently does not clamp.

Location: `crates/editor/src/row_scroller.rs` (test module)

### Step 2: Update `RowScroller::update_size` to accept `row_count` and re-clamp

Modify `RowScroller::update_size`:

```rust
/// Updates the viewport size based on height in pixels.
///
/// This recomputes `visible_rows` = floor(height_px / row_height) and re-clamps
/// `scroll_offset_px` to the new valid bounds.
// Chunk: docs/chunks/resize_click_alignment - Re-clamp scroll offset on resize
pub fn update_size(&mut self, height_px: f32, row_count: usize) {
    self.visible_rows = if self.row_height > 0.0 {
        (height_px / self.row_height).floor() as usize
    } else {
        0
    };
    // Re-clamp scroll offset to new valid bounds
    self.set_scroll_offset_px(self.scroll_offset_px, row_count);
}
```

The key addition is calling `set_scroll_offset_px` with the current offset after
updating `visible_rows`, which forces a re-clamp.

Location: `crates/editor/src/row_scroller.rs`

### Step 3: Update `Viewport::update_size` to accept `buffer_line_count`

Propagate the `row_count` parameter:

```rust
/// Updates the viewport size based on window height in pixels.
///
/// This recomputes `visible_lines` = floor(window_height / line_height) and
/// re-clamps the scroll offset to the new valid bounds.
// Chunk: docs/chunks/resize_click_alignment - Re-clamp scroll offset on resize
pub fn update_size(&mut self, window_height: f32, buffer_line_count: usize) {
    self.scroller.update_size(window_height, buffer_line_count);
}
```

Location: `crates/editor/src/viewport.rs`

### Step 4: Update `EditorState::update_viewport_dimensions` to pass line count

Modify `update_viewport_dimensions` to pass the buffer line count:

```rust
/// Updates the viewport size with both width and height.
// Chunk: docs/chunks/resize_click_alignment - Pass line count for scroll clamping
pub fn update_viewport_dimensions(&mut self, window_width: f32, window_height: f32) {
    let line_count = self.buffer().line_count();
    self.viewport_mut().update_size(window_height, line_count);
    self.view_height = window_height;
    self.view_width = window_width;
}
```

This ensures that on every resize, the scroll offset is re-clamped to valid bounds.

Location: `crates/editor/src/editor_state.rs`

### Step 5: Fix all call sites of `update_size`

Search for other call sites of `Viewport::update_size` and `RowScroller::update_size`
and update them to pass the appropriate row count:

1. **`Viewport` tests** in `viewport.rs`: Update test calls to pass a buffer line
   count (e.g., `100` for most tests, or a smaller value for small-buffer tests).

2. **`RowScroller` tests** in `row_scroller.rs`: Update test calls similarly.

3. **Any other `update_size` calls**: Check `editor_state.rs` and `main.rs` for
   additional sites.

Location: Multiple files

### Step 6: Verify the regression test passes

Run `cargo test -p lite-edit test_resize_clamps_scroll_offset` (or the chosen
test name) and verify it now passes.

### Step 7: Run all tests

Run `cargo test -p lite-edit` to verify:

- The new regression test passes.
- All existing `Viewport` and `RowScroller` tests still pass (after updating
  their `update_size` calls).
- All `EditorState` tests still pass.

### Step 8: Add backreference comment

Ensure the modified methods have a backreference comment pointing to this chunk:

```rust
// Chunk: docs/chunks/resize_click_alignment - Re-clamp scroll offset on resize
```

## Dependencies

This chunk depends on `row_scroller_extract` being complete (which introduced
the `RowScroller` struct). Per the GOAL.md frontmatter, `row_scroller_extract`
is listed in `created_after`, confirming it is already ACTIVE.

## Risks and Open Questions

1. **Wrapped line handling**: The `Viewport` also has `ensure_visible_wrapped`,
   which does its own scroll clamping based on screen rows rather than buffer
   lines. The simple `row_count` parameter we're adding represents buffer lines,
   not wrapped screen rows. This is acceptable because:
   - `update_viewport_dimensions` is called on window resize, which affects
     `visible_rows` (screen rows), not wrapped layout.
   - The re-clamp uses the buffer line count, which is the same semantics as
     `set_scroll_offset_px` already uses.
   - For wrapped content, scroll positions near the end may be sub-optimal
     after resize, but clicking will be correct because `first_visible_line`
     will match what the renderer draws.

2. **Editor vs Workspace**: The `EditorState` currently accesses `self.buffer()`
   to get line count. If the editor manages multiple workspaces with different
   buffers, we need to ensure we're getting the active workspace's buffer. The
   current code already calls `self.buffer()`, which returns the active buffer,
   so this should be correct.

3. **Performance**: Calling `set_scroll_offset_px` on every resize adds a small
   amount of work (computing `max_offset_px` and clamping). This is negligible
   compared to the overall resize handling cost.

## Deviations

<!-- Populated during implementation -->