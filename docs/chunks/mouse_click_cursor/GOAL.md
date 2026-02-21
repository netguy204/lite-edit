---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/context.rs
  - crates/editor/src/main.rs
code_references: []
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- editable_buffer
- glyph_rendering
- metal_surface
- viewport_rendering
---
# Mouse Click Cursor Positioning

## Minor Goal

Enable cursor positioning via mouse click, the most basic mouse interaction a text editor needs. Currently `handle_mouse` in `BufferFocusTarget` is a stub, and `MetalView` doesn't forward mouse events at all.

This chunk plumbs mouse-down events from macOS through the view layer to the buffer, converting pixel coordinates to (line, column) positions using font metrics and viewport scroll offset. It establishes the mouse event pipeline that the subsequent mouse-drag-selection chunk will build on.

## Success Criteria

- **MetalView forwards mouse-down events**: Override `mouseDown:` in the `MetalView` NSView subclass. Convert the NSEvent to a `MouseEvent` struct (already defined in `input.rs`) with `MouseEventKind::Down` and pixel position. Deliver it through a new `mouse_handler` callback (analogous to the existing `key_handler`).

- **EditorController routes mouse events**: Add a `handle_mouse` method on `EditorController` that forwards `MouseEvent` to `EditorState`, similar to how `handle_key` works. Wire the MetalView mouse handler callback in `setup_window`.

- **EditorState forwards mouse events to focus target**: Add a `handle_mouse` method that creates an `EditorContext` and calls `self.focus_target.handle_mouse(event, &mut ctx)`.

- **BufferFocusTarget.handle_mouse positions cursor on click**: On `MouseEventKind::Down`, convert the pixel position `(x, y)` to a buffer `(line, col)`:
  - `line = (y / line_height) + viewport.scroll_offset`, clamped to valid range
  - `col = (x / char_width)`, clamped to the length of the target line (using monospace font metrics from `EditorContext` or passed as parameters)
  - Call `buffer.set_cursor(Position::new(line, col))` and mark the cursor line dirty.

- **Font metrics are accessible**: The pixel-to-position conversion needs `line_height` and `char_width` (advance width). These are available from `Viewport::line_height()` and must be augmented with char width. Either add char width to `Viewport`, pass it through `EditorContext`, or make font metrics available to the focus target.

- **Coordinate system is correct**: macOS uses bottom-left origin for view coordinates. The conversion must flip the y-axis: `buffer_y = view_height - event_y`. Account for the scale factor (Retina) if mouse coordinates are in points rather than pixels.

- **Clicking on empty space below the last line**: Clamp to the last line of the buffer. Clicking past the end of a line clamps the column to the line length.

- **Unit tests**: Test the pixel-to-position conversion logic with known font metrics, including edge cases (click on first char, last char, past end of line, below last line, with scroll offset).
