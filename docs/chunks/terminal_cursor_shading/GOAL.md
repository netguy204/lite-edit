---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/terminal_buffer.rs
- crates/editor/src/glyph_buffer.rs
- crates/terminal/tests/integration.rs
code_references:
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::feed_bytes
    implements: "Test helper for feeding raw bytes to terminal emulator"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_after_typing_one_char
    implements: "Verifies cursor advances by one after typing single character"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_after_typing_multiple_chars
    implements: "Verifies cursor position after typing multiple characters"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_after_newline
    implements: "Verifies cursor moves to new line after CRLF"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_after_carriage_return_newline
    implements: "Verifies cursor position after CRLF sequences"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_after_cursor_movement_escape
    implements: "Verifies cursor position after left/right escape sequences"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_absolute_movement
    implements: "Verifies cursor position after absolute positioning escape"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_content_and_cursor_alignment
    implements: "Verifies cursor is positioned correctly relative to content"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cells_have_no_cursor_inverse_flags
    implements: "Verifies cells don't have spurious INVERSE flags from cursor"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_after_backspace
    implements: "Verifies cursor moves left after backspace"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_shell_prompt
    implements: "Verifies cursor tracking with shell prompt simulation"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_position_comprehensive_sequence
    implements: "Comprehensive test of cursor position through multiple operations"
  - ref: crates/terminal/src/terminal_buffer.rs#tests::test_cursor_at_content_boundary
    implements: "Verifies cursor is always at content boundary after typing"
narrative: null
investigation: null
subsystems:
  - subsystem_id: "renderer"
    relationship: uses
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- emacs_line_nav
- pane_mirror_restore
---

# Chunk Goal

## Minor Goal

In the terminal pane, the cursor's shading/inversion state lags behind its
actual position by one character and can get stuck. When typing, the character
under the cursor should be rendered with inverted colors (the standard block
cursor appearance), but instead the inversion applies to the previous cursor
position. The stale shading can also persist even after the cursor has moved
away, leaving ghost inversions on characters that are no longer under the cursor.

This was observed after the recent `file_change_events` chunk landed. The cursor
position itself is correct (the cursor moves to the right place), but the
shading/inversion that visually marks the cursor cell is applied one character
behind and doesn't always clean up when the cursor moves.

Fix the terminal cursor rendering so that the inverted shading tracks the
cursor position exactly and clears correctly when the cursor moves.

## Success Criteria

- The block cursor inversion is rendered on the exact cell the cursor occupies, not one character behind
- Moving the cursor (typing, arrow keys, backspace) immediately updates the shading to the new position
- The previous cursor position loses its inversion when the cursor moves away (no ghost shading)
- Cursor rendering is correct in both the shell prompt and during TUI application use
- No regression in cursor blink behavior or editor-pane cursor rendering

