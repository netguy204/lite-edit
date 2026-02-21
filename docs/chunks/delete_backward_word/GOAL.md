---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/src/text_buffer.rs
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_backward_word
    implements: "Character-class based backward word deletion logic with gap buffer integration"
  - ref: crates/editor/src/buffer_target.rs#Command::DeleteBackwardWord
    implements: "Command enum variant for Alt+Backspace action"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Key binding mapping Option+Backspace to DeleteBackwardWord command"
  - ref: crates/editor/src/buffer_target.rs#execute_command
    implements: "Command execution wiring for DeleteBackwardWord"
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

Add Alt+Backspace (Option+Delete) to delete backward by one word. This is a standard macOS text editing shortcut that users rely on for efficient editing. The word boundary rule is character-class based:

- **Non-whitespace class**: If the character immediately before the cursor is non-whitespace, delete backward through contiguous non-whitespace characters until hitting whitespace or the start of the line.
- **Whitespace class**: If the character immediately before the cursor is whitespace, delete backward through contiguous whitespace characters until hitting non-whitespace or the start of the line.

This requires:
1. A `delete_backward_word` method on `TextBuffer` (in the `buffer` crate) that implements the scan-and-delete logic using the gap buffer, returning `DirtyLines`.
2. A `DeleteBackwardWord` variant in the `Command` enum in `buffer_target.rs`.
3. A match arm in `resolve_command` mapping `Key::Backspace` with `option: true` to `DeleteBackwardWord`.
4. Execution wiring in `execute_command` to call the new buffer method.

If a selection is active when Alt+Backspace is pressed, the selection should be deleted instead (consistent with existing delete behavior).

## Success Criteria

- Alt+Backspace with cursor after `"hello world"` (cursor at col 11) deletes `"world"`, leaving `"hello "` with cursor at col 6
- Alt+Backspace with cursor after `"hello   "` (cursor at col 8, on whitespace) deletes the trailing spaces, leaving `"hello"` with cursor at col 5
- Alt+Backspace at the start of a line (col 0) is a no-op
- Alt+Backspace with an active selection deletes the selection (does not perform word deletion)
- Alt+Backspace works correctly at word boundaries mid-line (e.g., `"one two three"` with cursor at col 7 deletes `"two"`)
- The method returns appropriate `DirtyLines` for the affected line(s)
- Existing Backspace behavior (plain, no modifiers) is unchanged
