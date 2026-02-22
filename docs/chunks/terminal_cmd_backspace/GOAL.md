---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/input_encoder.rs
  - crates/terminal/tests/input_integration.rs
code_references:
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_special_key
    implements: "Cmd+Backspace encoding to \\x15 (Ctrl+U) for kill-line-backward"
  - ref: crates/terminal/src/input_encoder.rs#tests::test_encode_cmd_backspace
    implements: "Unit test verifying Cmd+Backspace encodes to \\x15"
  - ref: crates/terminal/tests/input_integration.rs#test_cmd_backspace_deletes_to_line_start
    implements: "Integration test verifying Cmd+Backspace clears line in shell context"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- terminal_alt_backspace
- terminal_background_box_drawing
- terminal_clipboard_selection
- terminal_tab_initial_render
---

# Chunk Goal

## Minor Goal

Cmd+Backspace does not delete to the start of the line when a terminal tab is focused. In standard macOS terminal emulators, Cmd+Backspace sends `\x15` (Ctrl+U / NAK) to the PTY, which readline, zsh line editor, and other line-editing libraries interpret as "kill line backward" (delete from cursor to start of line). This is a standard macOS terminal editing shortcut.

The existing `delete_to_line_start` chunk implemented Cmd+Backspace for the editor's `TextBuffer` (file editing), and the `terminal_alt_backspace` chunk established the pattern for encoding modifier+Backspace combinations in `InputEncoder` for terminal contexts. However, Cmd+Backspace in a terminal tab is either silently dropped or not encoded — the `\x15` byte never reaches the PTY.

This follows the same pattern as `terminal_alt_backspace`: add Cmd+Backspace encoding to `InputEncoder` so that the terminal tab matches expected macOS terminal behavior.

## Success Criteria

- Cmd+Backspace in a terminal tab sends `\x15` (Ctrl+U) to the PTY
- Unit test verifying Cmd+Backspace encodes to `\x15`
- Integration test verifying Cmd+Backspace deletes to line start in a shell context



