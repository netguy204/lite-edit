---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/src/text_buffer.rs
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_to_line_start
    implements: "Delete from cursor to line start method in TextBuffer"
  - ref: crates/editor/src/buffer_target.rs#Command::DeleteToLineStart
    implements: "Command enum variant for Cmd+Backspace action"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Key binding mapping Cmd+Backspace to DeleteToLineStart command"
  - ref: crates/editor/src/buffer_target.rs#execute_command
    implements: "Command execution dispatch to buffer.delete_to_line_start()"
narrative: editor_ux_refinements
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- mouse_drag_selection
- shift_arrow_selection
- text_selection_rendering
- viewport_scrolling
---
# Chunk Goal

## Minor Goal

Add Cmd+Backspace to delete from the cursor to the beginning of the current line. When the cursor is mid-line this deletes everything to the left of the cursor, leaving the cursor at column 0. When the cursor is already at column 0 on a non-first line, it joins the current line with the previous line (deletes the newline), mirroring the symmetric behaviour of `delete_to_line_end` at end-of-line.

This requires:
1. A `delete_to_line_start` method on `TextBuffer` (in the `buffer` crate) that deletes from the cursor position back to column 0, or joins with the previous line when already at column 0, returning `DirtyLines`.
2. A `DeleteToLineStart` variant in the `Command` enum in `buffer_target.rs`.
3. A match arm in `resolve_command` mapping `Key::Backspace` with `command: true` to `DeleteToLineStart`.
4. Execution wiring in `execute_command` to call the new buffer method.

If a selection is active when Cmd+Backspace is pressed, the selection should be deleted instead (consistent with existing delete behavior).

## Success Criteria

- Cmd+Backspace with cursor at col 5 in `"hello world"` deletes `"hello"`, leaving `" world"` with cursor at col 0
- Cmd+Backspace with cursor at end of `"hello world"` (col 11) deletes the entire line content, leaving `""` with cursor at col 0
- Cmd+Backspace at col 0 on line 0 is a no-op (already at the very start of the buffer)
- Cmd+Backspace at col 0 on line > 0 joins the current line with the previous line: the newline at the end of the previous line is deleted, the cursor moves to `(prev_line, prev_line_len)`, and `DirtyLines::FromLineToEnd(prev_line)` is returned
- Cmd+Backspace at col 0 on an empty line (e.g. line 1 in `"first\n\nthird"`) joins it with the previous line, leaving `"first\nthird"` with cursor at `(0, 5)`
- Cmd+Backspace with an active selection deletes the selection (does not perform line-start deletion)
- The method returns `DirtyLines::Single(line)` for mid-line deletion, `DirtyLines::FromLineToEnd(prev_line)` for a line-join, and `DirtyLines::None` for the no-op case
- Existing Cmd+Left (move to line start) and plain Backspace behaviors are unchanged
