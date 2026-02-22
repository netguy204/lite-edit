<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a surgical bug fix in `handle_mouse_selector` within `editor_state.rs`. The fix applies the same coordinate transformation pattern already established in `buffer_target.rs` for the main buffer's hit-testing.

The key insight: macOS delivers mouse events with `y = 0` at the **bottom** of the screen, but `calculate_overlay_geometry` computes `list_origin_y` as a **top-relative** offset. The main buffer correctly handles this in `pixel_to_buffer_position` and `pixel_to_buffer_position_wrapped` by computing `flipped_y = view_height - y` before any hit-testing math. The selector handler must do the same.

**Strategy:**
1. Before forwarding the mouse event to `SelectorWidget::handle_mouse`, flip the y coordinate using `view_height - y`
2. Pass this flipped coordinate so the selector operates entirely in top-relative coordinates, matching `list_origin_y`

This fix is self-contained and does not change `SelectorWidget` itself — it only corrects the coordinate transformation at the call site.

## Subsystem Considerations

No subsystems directory exists yet. This chunk touches the `selector_*` cluster (6 chunks), which may warrant subsystem documentation in the future. However, this is a targeted bug fix and does not require introducing new architectural patterns.

## Sequence

### Step 1: Add a unit test for coordinate flip in handle_mouse_selector

Write a test that exercises the coordinate transformation logic. Since `handle_mouse_selector` is a private method that depends on `EditorState`, we will test via the public interface or add a focused test at the appropriate level.

**Test case**: Given a mouse click with raw macOS coordinates (y=0 at bottom), verify that the selector correctly interprets which row was clicked. Specifically:
- Click position near the **top** of the view (large raw y) should map to items at the **top** of the list (small list index)
- Click position near the **bottom** of the view (small raw y) should map to items at the **bottom** of the visible list

Since `SelectorWidget::handle_mouse` expects coordinates in top-relative space (matching `list_origin_y`), the test will verify that after the fix, clicking at position `(x, raw_y)` with `raw_y` near `view_height` (top of screen in macOS coords) correctly targets the first visible item.

**Location**: `crates/editor/src/editor_state.rs` (test module or integration test)

**Note**: If testing at the EditorState level is complex due to setup requirements, we may instead add a comment documenting the invariant and rely on the existing `SelectorWidget::handle_mouse` tests which already assume correct coordinate input.

### Step 2: Flip the y coordinate before calling SelectorWidget::handle_mouse

Modify `handle_mouse_selector` in `editor_state.rs` (around line 891-899) to:

1. Compute `flipped_y = self.view_height as f64 - position.1`
2. Pass `(position.0, flipped_y)` to `selector.handle_mouse` instead of `position`

**Current code** (lines 889-899):
```rust
// Convert mouse position to the format expected by selector
// Mouse events arrive in view coordinates (y=0 at top)
let position = event.position;

// Forward to selector widget
let outcome = selector.handle_mouse(
    position,
    event.kind,
    geometry.item_height as f64,
    geometry.list_origin_y as f64,
);
```

**After fix**:
```rust
// Chunk: docs/chunks/selector_coord_flip - Y-coordinate flip for macOS mouse events
// Flip y-coordinate: macOS uses bottom-left origin, overlay geometry uses top-left
let flipped_y = (self.view_height as f64) - event.position.1;
let flipped_position = (event.position.0, flipped_y);

// Forward to selector widget with flipped coordinates
let outcome = selector.handle_mouse(
    flipped_position,
    event.kind,
    geometry.item_height as f64,
    geometry.list_origin_y as f64,
);
```

This matches the pattern in `buffer_target.rs:576` and `buffer_target.rs:645`.

### Step 3: Update the comment to reflect actual coordinate system

The existing comment says "Mouse events arrive in view coordinates (y=0 at top)" which is **incorrect** — macOS uses y=0 at **bottom**. Update the comment to accurately describe the transformation being applied.

### Step 4: Verify existing selector widget tests still pass

Run `cargo test` in `crates/editor` to confirm that:
- All `SelectorWidget` tests pass (they operate in the correct coordinate space)
- No regressions in mouse handling
- The fix doesn't break any other selector behavior

**Command**: `cargo test -p lite_edit_editor`

### Step 5: Manual verification (if possible)

If a test harness exists for running the editor, manually verify:
- Clicking the first item in the file picker selects item 0
- Clicking any visible item selects that exact item
- Scrolling + clicking still works correctly

## Dependencies

None. This is an independent bug fix with no dependencies on other chunks. The `created_after` entries in the GOAL.md frontmatter are informational ordering, not implementation dependencies.

## Risks and Open Questions

- **Test complexity**: Testing `handle_mouse_selector` directly may require substantial EditorState setup. If this proves unwieldy, we may rely on existing `SelectorWidget::handle_mouse` tests (which assume correct input coordinates) combined with code review of the coordinate flip.

- **Double-flip risk**: If any intermediate layer also flips coordinates, we could end up with a double-flip. Code review confirms this is not the case — `calculate_overlay_geometry` returns top-relative values, and `SelectorWidget::handle_mouse` expects top-relative input.

- **Integration verification**: The fix is mechanical, but full confidence requires manual testing in the actual editor. If the editor can't be run in the test environment, we rely on the test suite and code review.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->