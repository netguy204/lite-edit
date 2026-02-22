---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/input_encoder.rs
  - crates/terminal/src/terminal_target.rs
  - crates/terminal/src/terminal_buffer.rs
  - crates/terminal/src/lib.rs
  - crates/editor/src/input.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/workspace.rs
  - crates/terminal/tests/input_integration.rs
code_references:
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder
    implements: "Stateless encoder for terminal escape sequences"
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_key
    implements: "Keyboard event to escape sequence encoding"
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_char
    implements: "Printable character encoding with modifier handling"
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_ctrl_char
    implements: "Control character encoding (Ctrl-C, Ctrl-D, etc.)"
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_arrow
    implements: "Arrow key encoding with APP_CURSOR mode awareness"
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_paste
    implements: "Bracketed paste encoding"
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_mouse
    implements: "Mouse event encoding (SGR and legacy modes)"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget
    implements: "Focus target routing keyboard/mouse input to PTY"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::handle_key
    implements: "Keyboard event dispatch to PTY via encoder"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::handle_mouse
    implements: "Mouse event dispatch when mouse mode is active"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::write_paste
    implements: "Clipboard paste to PTY with bracketed paste support"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::term_mode
    implements: "Terminal mode flags accessor for input encoding"
  - ref: crates/input/src/lib.rs#Key
    implements: "Extended Key enum with F1-F12 and Insert variants"
  - ref: crates/terminal/src/lib.rs
    implements: "Module exports for InputEncoder and TerminalFocusTarget"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_emulator
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Build the input encoding layer that translates macOS keyboard and mouse events into terminal escape sequences and writes them to the PTY stdin. This is what makes the terminal interactive — without it, the terminal can display output but the user can't type commands, navigate in Vim, or interact with TUI applications.

`alacritty_terminal` tracks which terminal modes are active (APP_CURSOR, BRACKETED_PASTE, SGR_MOUSE, KITTY_KEYBOARD, etc.) but does NOT encode input — that's the frontend's responsibility. This chunk builds that frontend layer.

**Keyboard encoding:**
- Printable ASCII: write directly to PTY
- Control characters: Ctrl-C → `0x03`, Ctrl-D → `0x04`, Ctrl-Z → `0x1A`, etc.
- Arrow keys: `\x1b[A/B/C/D` (normal mode) or `\x1bOA/B/C/D` (APP_CURSOR mode)
- Function keys: F1-F12 → appropriate escape sequences
- Home/End/PageUp/PageDown/Insert/Delete: mode-dependent escape sequences
- Tab, Enter, Backspace, Escape: correct byte sequences
- Modifier combos: Shift+Arrow, Alt+key, Ctrl+Arrow — encoded per xterm conventions
- Bracketed paste: when mode is active, wrap pasted text in `\x1b[200~` ... `\x1b[201~`

**Mouse encoding (when TUI apps request it via terminal modes):**
- Click reporting: encode button + position per mouse mode (X10, normal, SGR)
- Motion reporting: encode mouse movement when MOUSE_MOTION mode is active
- Drag reporting: encode drag events when MOUSE_DRAG mode is active
- SGR encoding: `\x1b[<button;x;y;M/m` format when SGR_MOUSE is active

**Input dispatch:**
- When a terminal tab is focused, keyboard events route to the encoding layer → PTY stdin
- When a file tab is focused, keyboard events route to TextBuffer mutations (existing behavior)
- The tab knows its buffer type via `BufferView::is_editable()` and dispatches accordingly

## Success Criteria

- User can type commands in a terminal tab and see them echoed (basic shell interaction works)
- Ctrl-C sends SIGINT (interrupts running commands)
- Ctrl-D sends EOF (exits shells, closes programs expecting input)
- Arrow keys work for command-line editing (readline/zsh line editor)
- Arrow keys work in Vim (navigate in normal mode) — verifies APP_CURSOR mode encoding
- Tab completion works in shell
- Backspace and Delete work correctly
- Copy/paste works: Cmd+V pastes text (with bracketed paste encoding if mode is active)
- Function keys work in programs that use them (e.g., F1 for help in various TUI apps)
- Mouse clicks work in TUI apps that enable mouse reporting (e.g., htop, less with mouse mode)
- Input goes to PTY when terminal tab is focused; input goes to TextBuffer when file tab is focused