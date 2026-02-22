---
decision: APPROVE
summary: All success criteria satisfied - Alt+Backspace correctly encodes \x1b\x7f for terminal PTY while file-editing behavior is unaffected
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Alt+Backspace in a terminal tab sends `\x1b\x7f` (ESC + DEL) to the PTY

- **Status**: satisfied
- **Evidence**: `crates/terminal/src/input_encoder.rs:88-95` - The `encode_special_key()` function now checks `modifiers.option` for `Key::Backspace` and returns `vec![0x1b, 0x7f]` (ESC + DEL) when Alt is pressed. Unit test `test_encode_alt_backspace` at line 730 verifies this encoding produces `b"\x1b\x7f"`.

### Criterion 2: Bash/zsh readline deletes the word before the cursor when Alt+Backspace is pressed

- **Status**: satisfied
- **Evidence**: Integration test `test_alt_backspace_deletes_word` in `crates/terminal/tests/input_integration.rs:297-337` spawns a real shell, types "echo hello world", sends Alt+Backspace, and verifies the readline correctly processes the `\x1b\x7f` sequence. Test passes consistently.

### Criterion 3: The fix does not interfere with Alt+Backspace behavior in file-editing tabs (which should continue to use the TextBuffer `delete_backward_word` path)

- **Status**: satisfied
- **Evidence**: File-editing and terminal contexts use completely separate code paths. File-editing uses `BufferTarget::key_to_command()` in `crates/editor/src/buffer_target.rs:126-128` which maps Alt+Backspace to `Command::DeleteBackwardWord`, executing `ctx.buffer.delete_backward_word()`. Terminal input uses `InputEncoder::encode_key()` which operates on `TerminalFocusTarget`. These paths never intersect - the fix adds encoding logic only to `InputEncoder`, leaving the `BufferTarget` word deletion path unchanged.

### Criterion 4: Other Alt+key combinations that already work (Alt+Arrow, Alt+F, Alt+B) continue to function correctly

- **Status**: satisfied
- **Evidence**: All existing Alt+key tests pass: `test_encode_alt_character`, all arrow key tests including those with modifiers (`test_encode_arrow_with_shift`, `test_encode_arrow_with_ctrl`, `test_encode_arrow_with_shift_ctrl`). The change only adds a conditional branch for `Key::Backspace` - the `encode_char()` function that handles Alt+character combinations (including Alt+F, Alt+B) is untouched, and arrow keys use `encode_arrow()` which is also unchanged.
