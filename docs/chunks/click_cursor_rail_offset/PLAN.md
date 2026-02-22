<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix is a straightforward coordinate transformation in `handle_mouse_buffer`. When the workspace model introduced the left rail (`RAIL_WIDTH = 56px`), mouse events continued to be forwarded with raw window coordinates. The buffer's column calculation expects content-area-relative x coordinates, so we need to subtract `RAIL_WIDTH` from the event's x position before forwarding.

Following the project's TDD philosophy from TESTING_PHILOSOPHY.md, we'll write a failing test first that demonstrates the bug, then apply the one-line fix, and verify the test passes.

**Strategy**: Create a modified `MouseEvent` with adjusted x position before forwarding to `focus_target.handle_mouse`. This preserves the original event structure while correcting the coordinate system mismatch.

## Subsystem Considerations

No subsystems are relevant to this bug fix.

## Sequence

### Step 1: Write failing test demonstrating the bug

Add a test to `crates/editor/src/editor_state.rs` that verifies clicking at a specific x coordinate in the content area (after `RAIL_WIDTH`) positions the cursor at the expected column. The test should:

1. Create an `EditorState` with a known text content
2. Simulate a mouse click at x = `RAIL_WIDTH + (column * glyph_width)` (i.e., the expected window coordinate for a content column)
3. Assert that the cursor ends up at the intended column

This test will fail initially because the current code doesn't subtract `RAIL_WIDTH`, causing the cursor to land ~7-8 columns to the right.

**Test pattern**: Following the existing `test_mouse_click_positions_cursor` test in `buffer_target.rs`, but testing at the `EditorState` level which includes the rail offset logic.

Location: `crates/editor/src/editor_state.rs` (in the `#[cfg(test)]` module)

### Step 2: Apply the fix in handle_mouse_buffer

In `handle_mouse_buffer`, create a modified `MouseEvent` with the x position adjusted by subtracting `RAIL_WIDTH`:

```rust
// Adjust x position for content area (subtract rail width)
let adjusted_event = MouseEvent {
    position: (event.position.0 - RAIL_WIDTH as f64, event.position.1),
    ..event
};
self.focus_target.handle_mouse(adjusted_event, &mut ctx);
```

This transforms the window-relative x coordinate to a content-area-relative coordinate before forwarding to the buffer handler.

Location: `crates/editor/src/editor_state.rs`, line ~947

### Step 3: Verify the test passes

Run `cargo test` to confirm:
1. The new test now passes (cursor lands at expected column)
2. All existing mouse handling tests continue to pass
3. No regressions in other tests

### Step 4: Manual verification (optional)

If building and running the editor is practical, manually verify:
- Clicking at column 0 (immediately right of the rail) places cursor at column 0
- Clicking in the middle of a line places cursor at the expected column
- Mouse drag selection works correctly with the offset applied

## Dependencies

None. This fix builds on existing code from the `workspace_model` chunk which introduced both `RAIL_WIDTH` and the `handle_mouse_buffer` function.

## Risks and Open Questions

- **Low risk**: This is a straightforward coordinate transformation. The fix is isolated to a single function and doesn't affect the underlying buffer handling logic.

- **Scroll events**: Need to verify that scroll events in `handle_scroll` don't also need this offset. Looking at the code, scroll events don't use x position for positioning, so they should be unaffected.

- **Selector overlay**: The selector (`handle_mouse_selector`) calculates its own overlay geometry and doesn't use content-area coordinates, so it should be unaffected.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->