---
decision: APPROVE
summary: All success criteria satisfied with thorough implementation and comprehensive unit tests for pixel-to-position conversion.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: **MetalView forwards mouse-down events**: Override `mouseDown:` in the `MetalView` NSView subclass. Convert the NSEvent to a `MouseEvent` struct (already defined in `input.rs`) with `MouseEventKind::Down` and pixel position. Deliver it through a new `mouse_handler` callback (analogous to the existing `key_handler`).

- **Status**: satisfied
- **Evidence**: `metal_view.rs:175-185` implements `__mouse_down` that calls `convert_mouse_event` and invokes the mouse handler. `set_mouse_handler` method at line 267-269 mirrors `set_key_handler`. The `mouse_handler` field is stored in `MetalViewIvars` (line 49).

### Criterion 2: **EditorController routes mouse events**: Add a `handle_mouse` method on `EditorController` that forwards `MouseEvent` to `EditorState`, similar to how `handle_key` works. Wire the MetalView mouse handler callback in `setup_window`.

- **Status**: satisfied
- **Evidence**: `main.rs:192-195` implements `EditorController::handle_mouse` that forwards to `state.handle_mouse(event)` and calls `render_if_dirty()`. Mouse handler is wired at `main.rs:423-426` in `setup_window` using `metal_view.set_mouse_handler`.

### Criterion 3: **EditorState forwards mouse events to focus target**: Add a `handle_mouse` method that creates an `EditorContext` and calls `self.focus_target.handle_mouse(event, &mut ctx)`.

- **Status**: satisfied
- **Evidence**: `editor_state.rs:125-150` implements `handle_mouse` that creates `EditorContext` with font_metrics and view_height, and calls `self.focus_target.handle_mouse(event, &mut ctx)`. Also resets cursor blink timer and ensures cursor visibility.

### Criterion 4: **BufferFocusTarget.handle_mouse positions cursor on click**: On `MouseEventKind::Down`, convert the pixel position `(x, y)` to a buffer `(line, col)`:

- **Status**: satisfied
- **Evidence**: `buffer_target.rs:233-252` implements `handle_mouse` that on `MouseEventKind::Down` calls `pixel_to_buffer_position`, then `ctx.buffer.set_cursor(position)` and `ctx.mark_cursor_dirty()`.

### Criterion 5: `line = (y / line_height) + viewport.scroll_offset`, clamped to valid range

- **Status**: satisfied
- **Evidence**: `buffer_target.rs:294-308` computes `screen_line = (flipped_y / line_height).floor()`, then `buffer_line = scroll_offset.saturating_add(screen_line)`, and clamps to `[0, line_count - 1]`.

### Criterion 6: `col = (x / char_width)`, clamped to the length of the target line (using monospace font metrics from `EditorContext` or passed as parameters)

- **Status**: satisfied
- **Evidence**: `buffer_target.rs:312-320` computes `col = (x / char_width).floor()` and clamps to `line_len` via `col.min(line_len)`. Font metrics come from `ctx.font_metrics` through `EditorContext`.

### Criterion 7: Call `buffer.set_cursor(Position::new(line, col))` and mark the cursor line dirty.

- **Status**: satisfied
- **Evidence**: `buffer_target.rs:245-246` calls `ctx.buffer.set_cursor(position)` followed by `ctx.mark_cursor_dirty()`.

### Criterion 8: **Font metrics are accessible**: The pixel-to-position conversion needs `line_height` and `char_width` (advance width). These are available from `Viewport::line_height()` and must be augmented with char width. Either add char width to `Viewport`, pass it through `EditorContext`, or make font metrics available to the focus target.

- **Status**: satisfied
- **Evidence**: `EditorContext` at `context.rs:30-32` includes `font_metrics: FontMetrics` and `view_height: f32`. `FontMetrics` (from `font.rs`) contains `advance_width` and `line_height`. `EditorState` stores these at line 47-49 and passes them when creating `EditorContext`.

### Criterion 9: **Coordinate system is correct**: macOS uses bottom-left origin for view coordinates. The conversion must flip the y-axis: `buffer_y = view_height - event_y`. Account for the scale factor (Retina) if mouse coordinates are in points rather than pixels.

- **Status**: satisfied
- **Evidence**: `metal_view.rs:297-305` multiplies by scale factor to convert from points to pixels. `buffer_target.rs:288-289` flips y: `let flipped_y = (view_height as f64) - y`. View height is passed through EditorContext.

### Criterion 10: **Clicking on empty space below the last line**: Clamp to the last line of the buffer. Clicking past the end of a line clamps the column to the line length.

- **Status**: satisfied
- **Evidence**: `buffer_target.rs:303-308` clamps line to `line_count - 1` (or 0 for empty buffer). Line 319-320 clamps column to `line_len`. Tests at lines 836-852 and 820-834 verify these edge cases.

### Criterion 11: **Unit tests**: Test the pixel-to-position conversion logic with known font metrics, including edge cases (click on first char, last char, past end of line, below last line, with scroll offset).

- **Status**: satisfied
- **Evidence**: `buffer_target.rs:768-948` has comprehensive tests: `test_pixel_to_position_first_character`, `test_pixel_to_position_second_line`, `test_pixel_to_position_column_calculation`, `test_pixel_to_position_past_line_end`, `test_pixel_to_position_below_last_line`, `test_pixel_to_position_with_scroll_offset`, `test_pixel_to_position_empty_buffer`, `test_pixel_to_position_negative_x`, `test_pixel_to_position_fractional_coordinates`, and an integration test `test_mouse_click_positions_cursor`.
