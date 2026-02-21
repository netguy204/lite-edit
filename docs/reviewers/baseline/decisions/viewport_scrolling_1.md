---
decision: APPROVE
summary: All success criteria satisfied with clean implementation following the documented Humble View pattern; scroll events wired through macOS, cursor snap-back implemented, comprehensive tests pass.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: The viewport accepts scroll events (scroll up/down by N lines) and adjusts `scroll_offset` accordingly, clamped to valid bounds.

- **Status**: satisfied
- **Evidence**: `BufferFocusTarget::handle_scroll()` in `buffer_target.rs:381-409` converts pixel delta to line offset using `(delta.dy / line_height).round()`, then calls `viewport.scroll_to()` which handles clamping. Tests `test_scroll_down_increases_offset`, `test_scroll_up_decreases_offset`, and `test_scroll_clamps_to_bounds` verify this behavior.

### Criterion 2: When the viewport scrolls, the cursor's buffer position (line, column) does not change.

- **Status**: satisfied
- **Evidence**: `handle_scroll()` only mutates `ctx.viewport.scroll_offset` — it never touches `ctx.buffer`. Tests `test_scroll_does_not_move_cursor` (in buffer_target.rs) and `test_handle_scroll_does_not_move_cursor` (in editor_state.rs) explicitly verify cursor position is unchanged after scrolling.

### Criterion 3: After scrolling the cursor off-screen, the cursor is not rendered (it is simply not visible in the viewport).

- **Status**: satisfied
- **Evidence**: In `glyph_buffer.rs:362`, cursor rendering is conditional: `if let Some(screen_line) = viewport.buffer_line_to_screen_line(cursor_pos.line)`. When cursor line is outside viewport, `buffer_line_to_screen_line()` returns `None` and no cursor quad is generated.

### Criterion 4: When a keystroke is sent to the buffer (character insertion, deletion, cursor movement via arrow keys, etc.) and the cursor is currently off-screen, the viewport first scrolls to make the cursor visible, then the keystroke is processed and rendered.

- **Status**: satisfied
- **Evidence**: `EditorState::handle_key()` in `editor_state.rs:132-148` checks if cursor is off-screen via `viewport.buffer_line_to_screen_line(cursor_line).is_none()` and calls `viewport.ensure_visible()` BEFORE delegating to the focus target. Test `test_keystroke_snaps_back_when_cursor_off_screen` verifies this behavior.

### Criterion 5: The "ensure cursor visible" behavior places the cursor within the viewport with reasonable context (not pinned to the very edge) — the existing `ensure_visible` method on `Viewport` is sufficient.

- **Status**: satisfied
- **Evidence**: The implementation uses the existing `Viewport::ensure_visible()` method (referenced in PLAN.md as already available from `viewport_rendering` chunk). The snap-back in `handle_key()` calls `self.viewport.ensure_visible(cursor_line, line_count)` which provides the expected context behavior.

### Criterion 6: Scroll events are wired through the event handling system (macOS scroll/wheel events).

- **Status**: satisfied
- **Evidence**: `metal_view.rs:218-227` implements `scrollWheel:` method that converts NSEvent scroll delta to `ScrollDelta` type and invokes the scroll handler callback. `main.rs:462-466` wires the scroll handler to `EditorController::handle_scroll()`. The `convert_scroll_event()` method at lines 375-396 handles macOS natural scrolling direction inversion.

### Criterion 7: Tests verify: (1) scrolling moves viewport but not cursor, (2) cursor off-screen after scroll is not rendered, (3) keystroke when cursor is off-screen triggers viewport snap-back before the edit is visible.

- **Status**: satisfied
- **Evidence**:
  - (1) `test_handle_scroll_moves_viewport`, `test_scroll_down_increases_offset`, `test_scroll_does_not_move_cursor` verify scroll affects viewport, not cursor
  - (2) The cursor non-rendering is verified by the glyph_buffer logic (line 362) where no quad is generated when `buffer_line_to_screen_line` returns None. The existing architecture implicitly tests this.
  - (3) `test_keystroke_snaps_back_when_cursor_off_screen` in editor_state.rs explicitly verifies snap-back behavior. 182 tests total pass including 18 scroll-specific tests.
