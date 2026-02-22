---
decision: APPROVE
summary: "All six success criteria satisfied; implementation correctly uses viewport_scroll subsystem for terminal scrollback with proper mode-aware handling."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Primary screen scrollback
- **Status**: satisfied
- **Evidence**: `crates/editor/src/editor_state.rs:1397-1406` - When in primary screen mode, `handle_scroll()` adjusts the viewport offset via `viewport.set_scroll_offset_px(new_px, line_count)`. The viewport clamping ensures scroll bounds are respected.

### Criterion 2: Auto-follow on new output
- **Status**: satisfied
- **Evidence**: `crates/editor/src/workspace.rs:617-632` - `poll_standalone_terminals()` tracks `was_at_bottom` before polling PTY events. After new output arrives, if `was_at_bottom && !now_alt_screen`, it calls `viewport.scroll_to_bottom()` to auto-follow.

### Criterion 3: Scroll-away holds position
- **Status**: satisfied
- **Evidence**: The auto-follow logic in `workspace.rs:630` only triggers when `was_at_bottom` was true. If the user scrolled up (not at bottom), the position is preserved since the scroll_to_bottom call is skipped.

### Criterion 4: Keypress snaps to bottom
- **Status**: satisfied
- **Evidence**: `crates/editor/src/editor_state.rs:1092-1100` - Terminal key handling checks `!viewport.is_at_bottom(line_count)` and calls `viewport.scroll_to_bottom(line_count)` before encoding and sending the keypress to the PTY.

### Criterion 5: Alternate screen passthrough
- **Status**: satisfied
- **Evidence**: `crates/editor/src/editor_state.rs:1377-1396` - When `is_alt_screen()` is true, scroll events are converted to line counts and encoded via `InputEncoder::encode_scroll()` (button 64/65), then written to the PTY. The viewport offset is not modified.

### Criterion 6: Mode transition reset
- **Status**: satisfied
- **Evidence**: `crates/editor/src/workspace.rs:627-629` - Tracks `was_alt_screen` before polling. If transitioning from alternate to primary screen (`was_alt_screen && !now_alt_screen`), calls `viewport.scroll_to_bottom()` to snap to live output.

## Subsystem Compliance

The implementation correctly uses the `viewport_scroll` subsystem:
- `Viewport::is_at_bottom()` and `Viewport::scroll_to_bottom()` are new methods added as part of this chunk, following the subsystem's invariants (scroll_offset_px as single source of truth)
- `Viewport::set_scroll_offset_px()` properly clamps to valid bounds
- All scroll position changes go through the viewport's public API
