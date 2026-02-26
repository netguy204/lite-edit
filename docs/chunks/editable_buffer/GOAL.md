---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/input.rs
- crates/editor/src/focus.rs
- crates/editor/src/context.rs
- crates/editor/src/buffer_target.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/metal_view.rs
- crates/editor/src/main.rs
- crates/editor/tests/typing_test.rs
code_references:
  - ref: crates/input/src/lib.rs#KeyEvent
    implements: "Keyboard input event type with key and modifier fields"
  - ref: crates/input/src/lib.rs#Key
    implements: "Key enum mapping keyboard keys (chars, arrows, backspace, delete, etc.)"
  - ref: crates/input/src/lib.rs#Modifiers
    implements: "Modifier key state (shift, command, option, control)"
  - ref: crates/editor/src/focus.rs#FocusTarget
    implements: "FocusTarget trait with handle_key, handle_scroll, handle_mouse"
  - ref: crates/editor/src/focus.rs#Handled
    implements: "Result enum for focus target key handling"
  - ref: crates/editor/src/context.rs#EditorContext
    implements: "Mutable context providing access to buffer, viewport, dirty region, and font metrics"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget
    implements: "Buffer focus target handling editing commands (insert, delete, cursor movement)"
  - ref: crates/editor/src/buffer_target.rs#Command
    implements: "Editing command enum (InsertChar, DeleteBackward, MoveLeft, etc.)"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Stateless chord resolution: (modifiers, key) → Option<Command>"
  - ref: crates/editor/src/editor_state.rs#EditorState
    implements: "Consolidated mutable state with cursor blink and dirty region tracking"
  - ref: crates/editor/src/metal_view.rs#MetalView
    implements: "NSView key event forwarding (keyDown, acceptsFirstResponder, modifier capture)"
  - ref: crates/editor/src/main.rs#AppDelegate
    implements: "Application setup, window creation, and cursor blink NSTimer"
narrative: null
investigation: editor_core_architecture
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- viewport_rendering
created_after: []
---

# Main Loop + Input Events + Editable Buffer

## Minor Goal

Wire up keyboard input to the buffer and renderer, creating lite-edit's first interactive editing experience. This is the capstone chunk that brings together every architectural pattern established in the investigation: the drain-all-then-render main loop, the `FocusTarget` trait, the buffer focus target with stateless chord resolution, and dirty region tracking.

After this chunk, lite-edit is a functional (if minimal) text editor: you can type characters, delete them, move the cursor, and see the results rendered in real-time via Metal. This is the "small, fast core" described in GOAL.md — the input→render critical path with nothing else.

The drain-all-then-render pattern (from H4 findings) is the architectural centerpiece: each NSRunLoop iteration drains all pending events, forwards each to the active focus target which mutates the buffer and accumulates dirty regions, then renders once. This ensures latency fairness — no event is penalized by intermediate renders of events ahead of it in the batch.

## Success Criteria

- **FocusTarget trait** is defined:
  ```rust
  trait FocusTarget {
      fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled;
      fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext);
      fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext);
  }
  ```
  Where `EditorContext` provides access to the buffer, viewport, and dirty region accumulator.

- **Buffer focus target** implements `FocusTarget` and handles:
  - Printable characters → `InsertChar` (insert at cursor)
  - Backspace → `DeleteBackward`
  - Delete → `DeleteForward`
  - Arrow keys → cursor movement (left, right, up, down)
  - Cmd-A → select all (selection state not required for this chunk — just move cursor to start/end as a placeholder, or implement basic selection if natural)
  - Enter → insert newline
  - Each handler mutates the buffer and marks the appropriate dirty region.

- **Drain-all-then-render main loop** is implemented:
  1. Drain all pending NSEvents from the run loop, forwarding each to the active focus target.
  2. Render once if `dirty_region != DirtyRegion::None`.
  3. Sleep until the next event or timer.
  - Multiple keystrokes arriving between renders are batched correctly: all are processed, then one render occurs.

- **NSView key event forwarding** is wired up: `keyDown:` events on the editor view are converted to `KeyEvent` structs and delivered to the focus target. Modifier flags (Shift, Command, etc.) are captured correctly.

- **Cursor blink** works: an NSTimer toggles cursor visibility every ~500ms. The timer resets on any keystroke (cursor stays solid while typing). Cursor blink dirties only the cursor's line.

- **Typing test**: launching the app, typing a paragraph of text, using backspace to correct mistakes, and using arrow keys to navigate produces correct, responsive visual output with no visual glitches.

- **Viewport scrolling via cursor**: when the cursor moves past the bottom or top of the viewport (via arrow keys or newline insertion), the viewport scrolls to keep the cursor visible. This is automatic viewport adjustment, not manual scroll — trackpad/mouse scroll is a future concern.

- **No perceptible latency** during sustained typing at normal speed (~60-80 WPM). Formal latency measurement is a future concern, but interactive feel should be snappy.
