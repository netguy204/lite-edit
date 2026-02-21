# Implementation Plan

## Approach

This chunk establishes the mouse event pipeline from macOS through to cursor positioning. The architecture follows the existing pattern: MetalView handles NSEvent conversion (like it does for keyboard), EditorController routes to EditorState, and BufferFocusTarget interprets the event.

Key design choices:
1. **Parallel to key handling**: Just as `MetalView` has `set_key_handler`, we add `set_mouse_handler`. `EditorController` and `EditorState` already have `handle_key`; we add `handle_mouse` in parallel.
2. **Font metrics in EditorContext**: The pixel-to-position conversion needs `line_height` and `char_width` (advance width). We augment `EditorContext` with a `FontMetrics` struct to make these available to focus targets.
3. **Coordinate flipping**: macOS views use bottom-left origin; our buffer uses top-left. The conversion flips y: `buffer_y = view_height - event_y`.
4. **Scale factor handling**: NSEvent provides coordinates in points. We convert to pixels using the view's scale factor for consistent math.

The existing input types (`MouseEvent`, `MouseEventKind`) are already defined in `input.rs`.

Testing follows the humble view architecture: the pixel-to-position logic is pure math that can be unit tested without platform dependencies. We test the conversion function with known font metrics and edge cases.

## Sequence

### Step 1: Add `char_width` to EditorContext

Create a `FontMetrics` struct in `context.rs` (or use a simple tuple) to hold `line_height` and `char_width`. Update `EditorContext` to accept these values at construction time.

Location: `crates/editor/src/context.rs`

Rationale: Focus targets need font metrics to convert pixel coordinates to buffer positions. EditorContext is the state they have access to.

### Step 2: Thread font metrics through EditorState

Update `EditorState::handle_key` and the future `handle_mouse` to pass font metrics when constructing `EditorContext`. The metrics come from the Renderer's font.

Location: `crates/editor/src/editor_state.rs`

This requires `EditorState` to store font metrics (or receive them as parameters).

### Step 3: Add `handle_mouse` to EditorState

Add a `handle_mouse(&mut self, event: MouseEvent, font_metrics: FontMetrics, view_height: f32)` method that creates an `EditorContext` and calls `self.focus_target.handle_mouse(event, &mut ctx)`.

Location: `crates/editor/src/editor_state.rs`

### Step 4: Implement pixel-to-position conversion in BufferFocusTarget

Implement `handle_mouse` in `BufferFocusTarget`:

1. On `MouseEventKind::Down`:
   - Flip y-coordinate: `flipped_y = view_height - event.position.1`
   - Compute line: `line = (flipped_y / line_height) + scroll_offset`, clamp to `[0, line_count - 1]`
   - Compute col: `col = event.position.0 / char_width`, clamp to `[0, line_len]`
   - Call `buffer.set_cursor(Position::new(line, col))`
   - Mark cursor line dirty

The math must handle Retina (scale factor) if coordinates are in points.

Location: `crates/editor/src/buffer_target.rs`

### Step 5: Unit test pixel-to-position logic

Write tests for the conversion logic:
- Click on first character → col 0
- Click on last character of line → correct col
- Click past end of line → clamp to line length
- Click below last line → clamp to last line
- Click with scroll offset → correct buffer line
- Edge case: empty buffer

Location: `crates/editor/src/buffer_target.rs` (in `#[cfg(test)]` module)

Per testing philosophy: test the behavior, not the plumbing.

### Step 6: Override `mouseDown:` in MetalView

Add a `mouseDown:` override in MetalView that:
1. Extracts position from NSEvent (`locationInWindow`)
2. Converts to view coordinates (`convertPoint:fromView:`)
3. Extracts modifier flags
4. Constructs a `MouseEvent` with `MouseEventKind::Down`
5. Calls the mouse handler callback

Location: `crates/editor/src/metal_view.rs`

Also add a `set_mouse_handler` method parallel to `set_key_handler`.

### Step 7: Add `handle_mouse` to EditorController

Add a `handle_mouse(&mut self, event: MouseEvent)` method that:
1. Records mouse event time (like keystroke for cursor reset)
2. Forwards to `self.state.handle_mouse(event, font_metrics, view_height)`
3. Calls `render_if_dirty()`

Location: `crates/editor/src/main.rs`

### Step 8: Wire mouse handler in `setup_window`

In `AppDelegate::setup_window`, after setting up the key handler:
1. Clone controller for mouse handler closure
2. Call `metal_view.set_mouse_handler(...)` with a closure that calls `controller.borrow_mut().handle_mouse(event)`

Location: `crates/editor/src/main.rs`

### Step 9: Integration test (manual)

Manually verify:
- Click positions cursor at expected location
- Click on different lines works
- Click past end of line clamps correctly
- Click below last line clamps to last line
- Scrolled viewport: click targets correct buffer line

This cannot be automated (requires macOS GUI) per testing philosophy.

## Dependencies

All dependencies are satisfied:
- `editable_buffer` chunk: Provides TextBuffer, cursor movement, `set_cursor`
- `glyph_rendering` chunk: Provides Font and FontMetrics
- `metal_surface` chunk: Provides MetalView infrastructure
- `viewport_rendering` chunk: Provides Viewport with scroll_offset

The `MouseEvent` and `MouseEventKind` types are already defined in `input.rs`.

## Risks and Open Questions

1. **Coordinate system**: Need to verify whether `NSEvent.locationInWindow` returns points or pixels. If points, multiply by scale factor before pixel math.

2. **Click position rounding**: Should clicking at x=12.7 pixels (when char_width=10) target column 1 or 2? The plan uses truncation (`floor`). Alternatively, use `round` for nearest-character behavior. Start with truncation; adjust if it feels wrong.

3. **Tab characters**: The current approach assumes monospace chars. Tab characters may have different visual widths. For now, assume tabs render as single characters (the current behavior). This is a known limitation.

4. **View bounds**: If user clicks outside the actual text area (e.g., in padding), we may compute negative or very large positions. Clamping handles this, but verify behavior.

## Deviations

<!-- Populated during implementation -->