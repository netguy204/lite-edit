---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/input_encoder.rs
  - crates/terminal/tests/input_integration.rs
code_references:
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_special_key
    implements: "Alt+Backspace encoding - returns ESC+DEL when option modifier is active"
  - ref: crates/terminal/src/input_encoder.rs#tests::test_encode_alt_backspace
    implements: "Unit test verifying Alt+Backspace encodes to \\x1b\\x7f"
  - ref: crates/terminal/tests/input_integration.rs#test_alt_backspace_deletes_word
    implements: "Integration test verifying Alt+Backspace deletes word in shell context"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- scroll_bottom_deadzone_v3
- terminal_styling_fidelity
---

# Chunk Goal

## Minor Goal

Alt+Backspace (Option+Delete) does not perform backward-word-delete when a terminal tab is focused. In standard terminal emulators, Alt+Backspace sends `\x1b\x7f` (ESC followed by DEL) to the PTY, which readline, zsh line editor, and other line-editing libraries interpret as "delete word backward." This is a fundamental terminal editing shortcut.

The existing `delete_backward_word` chunk implemented Alt+Backspace for the editor's `TextBuffer` (file editing), but the `terminal_input_encoding` chunk's `InputEncoder` does not handle this key combination for terminal contexts. When a terminal tab is focused, Alt+Backspace is either silently dropped or encoded incorrectly — the escape sequence `\x1b\x7f` never reaches the PTY.

Context: Alt+Arrow navigation already works correctly in terminal readline contexts (confirming that Alt/Option modifier detection works), and Ctrl+A works as expected. The gap is specifically in the Alt+Backspace → `\x1b\x7f` encoding path in `InputEncoder`.

## Success Criteria

- Alt+Backspace in a terminal tab sends `\x1b\x7f` (ESC + DEL) to the PTY
- Bash/zsh readline deletes the word before the cursor when Alt+Backspace is pressed
- The fix does not interfere with Alt+Backspace behavior in file-editing tabs (which should continue to use the TextBuffer `delete_backward_word` path)
- Other Alt+key combinations that already work (Alt+Arrow, Alt+F, Alt+B) continue to function correctly