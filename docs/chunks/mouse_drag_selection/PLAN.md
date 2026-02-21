# Implementation Plan

## Approach

This chunk extends the existing mouse event infrastructure (from `mouse_click_cursor`) to support drag-to-select behavior. The architecture follows the established patterns:

1. **Event Pipeline Extension**: Add `mouseDragged:` and `mouseUp:` handlers in MetalView, mirroring the existing `mouseDown:` pattern. Each forwards events through the mouse handler callback using the existing `convert_mouse_event` helper.

2. **Selection Lifecycle in BufferFocusTarget**: Extend `handle_mouse` to manage the full selection lifecycle:
   - On `Down`: Position cursor AND set selection anchor (leveraging `text_selection_model` APIs)
   - On `Moved` (drag): Move cursor to extend selection from anchor
   - On `Up`: Finalize selection (clear if click-without-drag)

3. **Dirty Region Tracking**: Selection changes require marking affected lines dirty so the renderer (handled by `selection_rendering` chunk) can update highlights. We'll mark lines from the previous selection extent to the new extent.

The implementation builds directly on:
- `pixel_to_buffer_position` from `mouse_click_cursor` for coordinate conversion
- `set_selection_anchor_at_cursor`, `clear_selection`, `has_selection` from `text_selection_model`
- The existing `MouseEvent` / `MouseEventKind` types

Per TESTING_PHILOSOPHY.md, we follow the Humble View Architecture: all selection state manipulation is testable via `BufferFocusTarget::handle_mouse` with mocked events, without requiring a window or GPU.

## Sequence

### Step 1: Add mouseDragged handler to MetalView

Override `mouseDragged:` in MetalView to forward drag events through the mouse handler callback.

**Location**: `crates/editor/src/metal_view.rs`

**Details**:
- Add `#[unsafe(method(mouseDragged:))]` handler in the `impl MetalView` block within `define_class!`
- Call `convert_mouse_event(event, MouseEventKind::Moved)` and forward to `mouse_handler`
- Pattern mirrors the existing `__mouse_down` implementation

**Test**: Not directly unit-testable (NSView override). Integration tested via Step 5.

### Step 2: Add mouseUp handler to MetalView

Override `mouseUp:` in MetalView to forward mouse-up events.

**Location**: `crates/editor/src/metal_view.rs`

**Details**:
- Add `#[unsafe(method(mouseUp:))]` handler in the `define_class!` block
- Call `convert_mouse_event(event, MouseEventKind::Up)` and forward to `mouse_handler`
- Same pattern as Step 1

**Test**: Not directly unit-testable. Integration tested via Step 5.

### Step 3: Extend handle_mouse to set selection anchor on mouse down

Modify `BufferFocusTarget::handle_mouse` to set the selection anchor when mouse down occurs.

**Location**: `crates/editor/src/buffer_target.rs`

**Details**:
- In the `MouseEventKind::Down` branch, after setting the cursor position, call `ctx.buffer.set_selection_anchor_at_cursor()`
- This establishes the anchor for potential drag selection

**Test**: Add test `test_mouse_down_sets_selection_anchor` - verify that after a `MouseEventKind::Down` event, `buffer.selection_anchor` equals the clicked position.

### Step 4: Implement drag handling in handle_mouse

Handle `MouseEventKind::Moved` events to extend selection during drag.

**Location**: `crates/editor/src/buffer_target.rs`

**Details**:
- In the `MouseEventKind::Moved` branch:
  1. Convert pixel position to buffer position using `pixel_to_buffer_position`
  2. Store the previous cursor position for dirty tracking
  3. Move cursor directly (without clearing selection - need to update cursor without calling `set_cursor` which clears selection)
  4. Mark dirty lines from min(old_cursor.line, new_cursor.line) to max(old_cursor.line, new_cursor.line)

**Important**: We cannot use `set_cursor` because it clears the selection. We need to add a method that moves the cursor without clearing selection, or directly manipulate the cursor while preserving the anchor.

**Solution**: Add a helper method `move_cursor_preserving_selection` to TextBuffer that sets cursor position without clearing the selection anchor.

**Test**: Add test `test_mouse_drag_extends_selection` - simulate down→moved→moved sequence and verify selection_range spans from anchor to final cursor position.

### Step 5: Implement mouse-up finalization in handle_mouse

Handle `MouseEventKind::Up` to finalize or clear selection.

**Location**: `crates/editor/src/buffer_target.rs`

**Details**:
- In the `MouseEventKind::Up` branch:
  1. If anchor equals cursor (click without drag), call `ctx.buffer.clear_selection()`
  2. Otherwise, leave selection active for subsequent copy/replace operations
  3. No cursor position change on mouse-up

**Test**: Add test `test_click_without_drag_clears_selection` - set up existing selection, then simulate down+up at same position, verify selection is cleared. Add test `test_drag_then_release_preserves_selection` - verify selection remains after drag+release.

### Step 6: Add move_cursor_preserving_selection to TextBuffer

Add a method to TextBuffer that sets cursor position without clearing the selection anchor.

**Location**: `crates/buffer/src/text_buffer.rs`

**Details**:
```rust
/// Sets the cursor to an arbitrary position without clearing selection.
///
/// This is used during drag operations where we want to extend the selection
/// from a fixed anchor. The position is clamped to valid bounds.
pub fn move_cursor_preserving_selection(&mut self, pos: Position) {
    let line = pos.line.min(self.line_count().saturating_sub(1));
    let col = pos.col.min(self.line_len(line));
    self.cursor = Position::new(line, col);
}
```

**Test**: Add test `test_move_cursor_preserving_selection_keeps_anchor` - set anchor, call method, verify anchor unchanged and cursor moved.

### Step 7: Compute dirty region for selection changes during drag

Track old and new selection extents and mark the full range dirty.

**Location**: `crates/editor/src/buffer_target.rs`

**Details**:
- In the `MouseEventKind::Moved` handler, before moving the cursor:
  1. Get the current selection range (anchor to old cursor)
  2. After moving cursor, get the new selection range
  3. Compute the union of old and new ranges
  4. Mark `DirtyLines::Range(min_line, max_line)` or use `FullViewport` if range is large

**Alternative**: For simplicity, mark from `min(anchor.line, old_cursor.line, new_cursor.line)` to `max(...)` as dirty.

**Note**: Need to check how DirtyRegion handles line ranges. Current implementation has `DirtyLines::Single(line)` and `DirtyLines::FromLineToEnd(line)`. May need to add a range variant or use `FullViewport`.

**Test**: Add test verifying dirty region covers both old and new selection lines.

### Step 8: Write comprehensive unit tests

Add remaining tests per success criteria.

**Location**: `crates/editor/src/buffer_target.rs` (tests module)

**Tests to add**:
1. `test_mouse_sequence_down_moved_up` - Full lifecycle test
2. `test_drag_past_line_end_clamps_column` - Click, drag past line end, verify column clamped
3. `test_drag_below_last_line_clamps_to_last_line` - Drag below buffer, verify line clamped
4. `test_drag_above_first_line_clamps_to_first_line` - Drag with negative y (above view), verify line 0
5. `test_selection_range_during_drag` - Verify selection_range() returns correct ordered range at each step
6. `test_drag_updates_dirty_region` - Verify correct lines are marked dirty

## Dependencies

- **mouse_click_cursor**: Provides `pixel_to_buffer_position`, `MouseEvent`, `MetalView::convert_mouse_event`, and the mouse handler callback infrastructure. **Status: ACTIVE (complete)**
- **text_selection_model**: Provides `set_selection_anchor_at_cursor`, `clear_selection`, `has_selection`, `selection_range`. **Status: ACTIVE (complete)**

## Risks and Open Questions

1. **Cursor movement without selection clear**: The current `set_cursor` method clears selection. We need `move_cursor_preserving_selection` (Step 6) to avoid this during drag.

2. **Dirty region granularity**: Current `DirtyLines` enum has `Single`, `FromLineToEnd`, and `None`. For selection drag, we ideally want a range. Current workaround: use `FromLineToEnd(min_affected_line)` which may over-dirty. The selection_rendering chunk will handle actual rendering; this chunk just needs to ensure affected lines are marked.

3. **No existing anchor state tracking in BufferFocusTarget**: We may need to track whether we're in a drag operation (mouse down without mouse up yet). However, since the selection anchor already exists on the buffer and we clear selection on mouse-up if anchor==cursor, we may not need additional state.

4. **Scroll during drag**: The goal explicitly states "auto-scroll during drag is a future enhancement." We clamp to visible/valid bounds only. No need to implement scroll-on-drag.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->