---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
  - crates/editor/src/buffer_target.rs
  - crates/buffer/src/text_buffer.rs
code_references: []
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- mouse_click_cursor
- text_selection_model
created_after:
- editable_buffer
- glyph_rendering
- metal_surface
- viewport_rendering
---
# Mouse Drag Selection

## Minor Goal

Enable text selection via mouse drag. When the user presses the mouse button and drags, text between the press point and the current drag position should be selected. This builds on the mouse click cursor positioning (which established the mouse event pipeline and pixel-to-position conversion) and the text selection model (which provides the anchor/cursor selection API on TextBuffer).

## Success Criteria

- **MetalView forwards drag and mouse-up events**: Override `mouseDragged:` and `mouseUp:` in MetalView. Convert each to a `MouseEvent` with `MouseEventKind::Moved` (drag) or `MouseEventKind::Up` and deliver through the mouse handler callback. The existing `mouseDown:` forwarding from the mouse_click_cursor chunk is already in place.

- **BufferFocusTarget handles the full mouse lifecycle**:
  - `MouseEventKind::Down` — convert pixel position to buffer position, set cursor there (existing behavior from mouse_click_cursor chunk), **and** set the selection anchor at the same position via `buffer.set_selection_anchor_at_cursor()`.
  - `MouseEventKind::Moved` (drag) — convert pixel position to buffer position, move cursor there (extending the selection from the anchor). Mark affected lines dirty.
  - `MouseEventKind::Up` — finalize the selection. If anchor equals cursor (click without drag), clear the selection. Otherwise, leave the selection active for subsequent copy/replace operations.

- **Drag-to-select visual feedback**: As the mouse drags, the cursor moves and the selection range updates. The dirty region tracking must cover both the old and new selection extents so the renderer can update the highlight. (Actual rendering of the highlight is handled by the selection_rendering chunk — this chunk ensures the selection model is correctly updated during drag.)

- **Edge cases**:
  - Dragging past the end of a line clamps the column to line length
  - Dragging below the last line clamps to the last line
  - Dragging above the first visible line clamps to line 0 (auto-scroll during drag is a future enhancement)
  - A click without drag (mouse down then immediate mouse up at same position) clears any existing selection

- **Unit tests**: Test the mouse event sequence (down → moved → moved → up) through BufferFocusTarget and verify the selection state at each step. Test click-without-drag clears selection.
