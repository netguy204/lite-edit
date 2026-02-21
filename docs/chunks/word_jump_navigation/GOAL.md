---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/src/text_buffer.rs
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::move_word_right
    implements: "Word-jump cursor movement to right edge of current/next word"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::move_word_left
    implements: "Word-jump cursor movement to left edge of current/previous word"
  - ref: crates/editor/src/buffer_target.rs#Command::MoveWordLeft
    implements: "Command enum variant for Option+Left word navigation"
  - ref: crates/editor/src/buffer_target.rs#Command::MoveWordRight
    implements: "Command enum variant for Option+Right word navigation"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Key binding mapping for Option+Left/Right to word movement commands"
  - ref: crates/editor/src/buffer_target.rs#execute_command
    implements: "Command execution handlers for MoveWordLeft/MoveWordRight"
narrative: word_nav
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- word_boundary_primitives
created_after:
- delete_backward_word
- file_picker
- fuzzy_file_matcher
- selector_rendering
- selector_widget
---

# Chunk Goal

## Minor Goal

Add Alt+Left and Alt+Right word-jump cursor movement, giving users the ability to
navigate by whole words rather than one character at a time.

This is the most frequently used word-oriented editing gesture on macOS. Without it,
moving across identifiers, arguments, or prose requires holding an arrow key or
clicking. The feature builds directly on the `word_boundary_left` and
`word_boundary_right` helpers from `word_boundary_primitives` and wires into the
existing `Command` dispatch pipeline with minimal new surface area.

Word navigation behaviour follows `docs/trunk/SPEC.md#word-model`: movement stays
within the current line, the character class under the cursor determines which run to
act on, and the navigation special case applies — if the cursor is already at the near
edge of the current run (or the run is whitespace), the jump continues to the far edge
of the adjacent non-whitespace run in the pressed direction.

## Success Criteria

- `TextBuffer::move_word_right` exists and moves the cursor to the right edge of the
  non-whitespace run at the cursor, or — if the cursor is on whitespace — past the
  whitespace and to the right edge of the following non-whitespace run. Stops at line
  end. Clears any active selection.
- `TextBuffer::move_word_left` exists and moves the cursor to the left edge of the
  non-whitespace run at the cursor, or — if the cursor is on whitespace or at the
  leftmost column of a word — past the whitespace and to the left edge of the preceding
  non-whitespace run. Stops at column 0. Clears any active selection.
- Both methods carry a `// Spec: docs/trunk/SPEC.md#word-model` comment and call
  `word_boundary_left` / `word_boundary_right` rather than reimplementing scan logic.
- `MoveWordLeft` and `MoveWordRight` variants exist in the `Command` enum in
  `buffer_target.rs`.
- `resolve_command` maps `Option+Left` → `MoveWordLeft` and `Option+Right` →
  `MoveWordRight`, checked before the plain `Left` / `Right` arms.
- `execute_command` calls the new buffer methods then `mark_cursor_dirty` +
  `ensure_cursor_visible`.
- Unit tests in `text_buffer.rs` cover: cursor mid-word (lands at word end / start),
  cursor at word start (right: stays at same word end; left: jumps to preceding word
  start), cursor at word end (right: jumps to next word end; left: stays at same word
  start), cursor on whitespace between words, cursor at line start, cursor at line end,
  empty line, single-character word.

