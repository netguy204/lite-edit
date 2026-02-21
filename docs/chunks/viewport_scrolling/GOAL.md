---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
  - crates/editor/src/main.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/tests/viewport_test.rs
code_references:
  - ref: crates/editor/src/input.rs#ScrollDelta
    implements: "Scroll delta data structure for representing scroll events"
  - ref: crates/editor/src/metal_view.rs#MetalView::__scroll_wheel
    implements: "macOS scrollWheel: event handler that invokes the scroll callback"
  - ref: crates/editor/src/metal_view.rs#MetalView::convert_scroll_event
    implements: "Converts NSEvent scroll deltas to ScrollDelta with natural scrolling handling"
  - ref: crates/editor/src/metal_view.rs#MetalView::set_scroll_handler
    implements: "Sets the scroll event handler callback"
  - ref: crates/editor/src/focus.rs#FocusTarget::handle_scroll
    implements: "Trait method for focus targets to handle scroll events"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::handle_scroll
    implements: "Converts scroll delta to line offset and mutates viewport.scroll_offset"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_scroll
    implements: "Forwards scroll events to focus target via EditorContext"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key
    implements: "Ensures cursor visibility before keystroke processing (snap-back logic)"
  - ref: crates/editor/src/main.rs#EditorController::handle_scroll
    implements: "Controller method that forwards scroll to state and triggers render"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- clipboard_operations
- mouse_click_cursor
- text_selection_model
- quit_command
---

# Viewport Scrolling

## Minor Goal

Enable the viewport to be scrolled via scroll events (e.g., mouse wheel, trackpad). When the user scrolls, the viewport moves but the cursor stays at its current buffer position — even if that position is now off-screen. When the user subsequently types a keystroke that mutates the buffer (or moves the cursor), the viewport snaps back to make the cursor visible before applying the edit.

This establishes the fundamental scroll interaction model: scrolling is a viewport-only operation that never moves the cursor, while editing is a cursor-following operation that always ensures visibility.

## Success Criteria

- The viewport accepts scroll events (scroll up/down by N lines) and adjusts `scroll_offset` accordingly, clamped to valid bounds.
- When the viewport scrolls, the cursor's buffer position (line, column) does not change.
- After scrolling the cursor off-screen, the cursor is not rendered (it is simply not visible in the viewport).
- When a keystroke is sent to the buffer (character insertion, deletion, cursor movement via arrow keys, etc.) and the cursor is currently off-screen, the viewport first scrolls to make the cursor visible, then the keystroke is processed and rendered.
- The "ensure cursor visible" behavior places the cursor within the viewport with reasonable context (not pinned to the very edge) — the existing `ensure_visible` method on `Viewport` is sufficient.
- Scroll events are wired through the event handling system (macOS scroll/wheel events).
- Tests verify: (1) scrolling moves viewport but not cursor, (2) cursor off-screen after scroll is not rendered, (3) keystroke when cursor is off-screen triggers viewport snap-back before the edit is visible.