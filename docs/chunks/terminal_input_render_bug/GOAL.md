---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/main.rs
  - crates/terminal/tests/input_integration.rs
  - crates/terminal/tests/integration.rs
code_references:
  - ref: crates/editor/src/main.rs#EditorController::toggle_cursor_blink
    implements: "Timer-driven PTY polling to process shell output into TerminalBuffer"
  - ref: crates/editor/src/main.rs#EditorController::handle_key
    implements: "Immediate PTY polling after keyboard input for responsive echo"
  - ref: crates/editor/src/main.rs#EditorController::handle_mouse
    implements: "PTY polling after mouse input to terminal tabs"
  - ref: crates/editor/src/main.rs#EditorController::handle_scroll
    implements: "PTY polling after scroll input to terminal tabs"
  - ref: crates/terminal/tests/integration.rs#test_shell_prompt_appears
    implements: "Integration test verifying shell spawn → PTY poll → prompt visible"
  - ref: crates/terminal/tests/integration.rs#test_pty_input_output_roundtrip
    implements: "Integration test verifying write_input → PTY → poll → buffer echo"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on:
- terminal_input_encoding
- terminal_scrollback_viewport
- renderer_polymorphic_buffer
created_after:
- terminal_scrollback_viewport
- renderer_polymorphic_buffer
---

# Chunk Goal

## Minor Goal

After pressing `Cmd+Shift+T` to open a terminal tab, the terminal appears non-functional: typing characters produces no visible output, pressing Enter after a command (e.g. `ls`) has no effect, and scrolling does nothing. The terminal tab opens and is visually present, but it behaves as if input is not reaching the PTY, output is not rendering, or both.

The `terminal_tab_spawn` chunk wired the keybinding and shell spawn. The `terminal_input_encoding` chunk built `InputEncoder` and `TerminalFocusTarget` for routing keyboard input to the PTY. The `renderer_polymorphic_buffer` chunk made the renderer accept `&dyn BufferView` so terminal content renders through the same path as file tabs. The `terminal_scrollback_viewport` chunk wired scroll events. Despite all these pieces existing, the end-to-end flow from keypress → PTY → output → screen is broken.

This chunk must diagnose and fix the disconnect. Likely failure points include:
- Key events not being dispatched to `TerminalFocusTarget::handle_key` when a terminal tab is focused (routing gap between `EditorState::handle_key` and the terminal input path)
- PTY output not being polled or not triggering a re-render (polling loop not running, or render not being requested after PTY read)
- `BufferView` for `TerminalBuffer` returning empty/stale content to the renderer
- Scroll events not reaching `TerminalFocusTarget::handle_scroll`

## Success Criteria

- After pressing `Cmd+Shift+T`, the user sees a shell prompt rendered in the terminal tab
- Typing characters in the terminal tab produces visible echoed output
- Typing `ls` and pressing Enter executes the command and displays its output
- Scrolling with trackpad/mouse wheel works when there is scrollback content
- Ctrl+C interrupts a running command
- Switching to a file tab and back to the terminal tab preserves terminal state