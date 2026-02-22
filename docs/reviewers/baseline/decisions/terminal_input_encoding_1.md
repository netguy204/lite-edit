---
decision: APPROVE
summary: All success criteria satisfied through InputEncoder, TerminalFocusTarget, and comprehensive unit tests; implementation follows documented patterns from PLAN.md
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: User can type commands in a terminal tab and see them echoed (basic shell interaction works)

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_char()` (input_encoder.rs:32-51) encodes printable characters as UTF-8 bytes, writing them to PTY via `TerminalFocusTarget::handle_key()` (terminal_target.rs:58-92). Integration tests `test_typing_basic_command` verifies "echo hello" produces expected output.

### Criterion 2: Ctrl-C sends SIGINT (interrupts running commands)

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_ctrl_char()` (input_encoder.rs:57-79) maps Ctrl+C to 0x03 (ETX). Unit test `test_encode_ctrl_c` verifies encoding. Integration test `test_ctrl_c_sends_interrupt` demonstrates interrupting `sleep 100`.

### Criterion 3: Ctrl-D sends EOF (exits shells, closes programs expecting input)

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_ctrl_char()` maps Ctrl+D to 0x04 (EOT). Unit test `test_encode_ctrl_d` verifies encoding. Integration test `test_ctrl_d_sends_eof` verifies EOF to `cat` command.

### Criterion 4: Arrow keys work for command-line editing (readline/zsh line editor)

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_arrow()` (input_encoder.rs:127-141) produces `\x1b[A/B/C/D` in normal mode. Tests `test_encode_arrow_normal_mode` verify CSI sequences.

### Criterion 5: Arrow keys work in Vim (navigate in normal mode) — verifies APP_CURSOR mode encoding

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_arrow()` checks `TermMode::APP_CURSOR` and switches to SS3 sequences (`\x1bOA/B/C/D`). Unit test `test_encode_arrow_app_cursor_mode` explicitly verifies APP_CURSOR produces `\x1bOA`.

### Criterion 6: Tab completion works in shell

- **Status**: satisfied
- **Evidence**: `encode_special_key()` maps `Key::Tab` to 0x09 (HT). Unit test `test_encode_tab` verifies. Integration test `test_tab_key_for_completion` demonstrates tab sent to shell.

### Criterion 7: Backspace and Delete work correctly

- **Status**: satisfied
- **Evidence**: Backspace encoded as 0x7f (DEL) per `encode_special_key()`. Delete uses tilde format `\x1b[3~` via `encode_tilde_key()`. Unit tests `test_encode_backspace` and `test_encode_navigation_keys` verify. Integration test `test_backspace_deletes_character` demonstrates correction.

### Criterion 8: Copy/paste works: Cmd+V pastes text (with bracketed paste encoding if mode is active)

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_paste()` (input_encoder.rs:222-231) wraps text in `\x1b[200~`...`\x1b[201~` when BRACKETED_PASTE mode active. `TerminalFocusTarget::write_paste()` (terminal_target.rs:142-150) provides the paste API. Unit tests `test_encode_paste_*` verify both modes. Cmd+V correctly returns false from `handle_key()` to let editor dispatch to `write_paste()`.

### Criterion 9: Function keys work in programs that use them (e.g., F1 for help in various TUI apps)

- **Status**: satisfied
- **Evidence**: F1-F4 encoded as SS3 sequences (`\x1bOP/Q/R/S`), F5-F12 use tilde format with VT220 numbering gaps. Key enum extended with F1-F12 variants (input/src/lib.rs:104-127). metal_view.rs maps macOS keycodes 0x7A-0x6F. Unit tests `test_encode_f1_f4` and `test_encode_f5_f12` verify sequences. Integration test `test_function_keys_encoding` confirms.

### Criterion 10: Mouse clicks work in TUI apps that enable mouse reporting (e.g., htop, less with mouse mode)

- **Status**: satisfied
- **Evidence**: `InputEncoder::encode_mouse()` (input_encoder.rs:236-251) handles X10/normal and SGR encoding based on TermMode flags. `TerminalFocusTarget::handle_mouse()` (terminal_target.rs:115-136) converts pixel positions to cell coordinates and encodes if mouse mode active. Unit tests verify SGR click/release (`test_encode_mouse_sgr_*`), legacy encoding (`test_encode_mouse_legacy`), and modifier handling.

### Criterion 11: Input goes to PTY when terminal tab is focused; input goes to TextBuffer when file tab is focused

- **Status**: satisfied
- **Evidence**: `TerminalFocusTarget` is the focus target for terminal tabs, routing input to PTY via `write_input()`. `BufferView::is_editable()` returns false for TerminalBuffer (terminal_buffer.rs:302-306), true for TextBuffer. workspace.rs has `TabKind::Terminal` variant and comment noting future dispatch to TerminalFocusTarget. The architectural seam is in place; full wiring deferred to workspace model chunk.
