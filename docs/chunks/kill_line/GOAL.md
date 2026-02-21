---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/src/text_buffer.rs
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_to_line_end
    implements: "Delete from cursor to end of line (kill-line) behavior with Emacs C-k semantics"
  - ref: crates/editor/src/buffer_target.rs#Command::DeleteToLineEnd
    implements: "Command enum variant for kill-line operation"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Ctrl+K key binding resolution to DeleteToLineEnd command"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Execute DeleteToLineEnd command via ctx.buffer.delete_to_line_end()"
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- editable_buffer
- glyph_rendering
- metal_surface
- viewport_rendering
---
# Ctrl+K Kill Line

## Minor Goal

Add Ctrl+K to delete from the cursor position to the end of the current line. This is a standard editor shortcut (originating from Emacs `C-k`) that provides a fast way to clear the remainder of a line without repeatedly pressing Delete. The cursor stays in place; the text from the cursor to the end of the line is removed.

## Success Criteria

- **`delete_to_line_end` method on TextBuffer**: Add a method that:
  1. Determines how many characters exist from the cursor column to the end of the current line
  2. If the cursor is already at the end of the line, deletes the newline character (joining with the next line), similar to `delete_forward` at line end — this matches Emacs `kill-line` behavior
  3. Otherwise, removes all characters from the cursor to the end of the line
  4. Returns `DirtyLines::Single(cursor_line)` for within-line deletion, or `DirtyLines::FromLineToEnd(cursor_line)` if a newline was deleted (lines shift up)
  5. Does not move the cursor (cursor stays at same line/column)

- **`DeleteToLineEnd` command**: Add `DeleteToLineEnd` to the `Command` enum in `buffer_target.rs`.

- **Key binding**: Map `Key::Char('k')` with `mods.control && !mods.command` to `DeleteToLineEnd` in `resolve_command`.

- **Execute command**: In `execute_command`, call `ctx.buffer.delete_to_line_end()`, mark the result dirty, and ensure cursor visibility.

- **Interaction with selection**: If a selection is active when Ctrl+K is pressed, the selection should be cleared first (or ignored), then the kill-line operation proceeds from the cursor position. Kill-line operates on the cursor position, not on the selection. (Alternatively, delete the selection first — but standard Emacs behavior ignores the mark on `C-k`, so clearing the selection and operating from cursor is more consistent.)

- **Unit tests**:
  - Kill from middle of line: `"hello world"` with cursor at col 5 → `"hello"`
  - Kill from start of line: `"hello"` with cursor at col 0 → `""` (empty line)
  - Kill at end of line (joins next line): `"hello\nworld"` with cursor at col 5 on line 0 → `"helloworld"` (one line)
  - Kill on empty line: joins with next line
  - Kill at end of buffer: no-op
  - Cursor position unchanged after kill (same line, same column)
  - Dirty lines are correct for each case
