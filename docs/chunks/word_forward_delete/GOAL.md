---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/src/text_buffer.rs
- crates/editor/src/buffer_target.rs
- crates/editor/src/metal_view.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_forward_word
    implements: "Alt+D forward word deletion using word_boundary_right for boundary computation"
  - ref: crates/editor/src/buffer_target.rs#Command::DeleteForwardWord
    implements: "DeleteForwardWord variant in Command enum for Alt+D"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Key binding mapping Option+'d' to DeleteForwardWord command"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Command execution dispatching DeleteForwardWord to buffer method"
  - ref: crates/editor/src/metal_view.rs#MetalView::convert_key
    implements: "Option modifier handling using charactersIgnoringModifiers for base key character"
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

Add Alt+D forward word deletion, the forward complement to the existing Alt+Backspace
(`delete_backward_word`). Where Alt+Backspace removes the word behind the cursor,
Alt+D removes the word ahead of it — completing the symmetric pair of word-deletion
gestures that Emacs and macOS terminal users expect.

The behaviour follows `docs/trunk/SPEC.md#word-model`: delete forward through the
contiguous run of the same character class as the character at the cursor position
(non-whitespace eats non-whitespace; whitespace eats whitespace), stopping at the line
boundary. This is a single-run deletion — it does not skip whitespace to reach a word,
unlike the navigation special case in `move_word_right`.

This chunk builds on `word_boundary_right` from `word_boundary_primitives`.

## Success Criteria

- `TextBuffer::delete_forward_word` exists and deletes from `cursor.col` to
  `word_boundary_right(chars, cursor.col)` on the current line.
- The character class is determined by `chars[cursor.col]` (the character at the
  cursor, not before it), consistent with the forward-direction rule in
  `docs/trunk/SPEC.md#word-model`.
- If the cursor is at the end of the line (`cursor.col >= line_len`), the method is a
  no-op and returns `DirtyLines::None`. It does not delete the newline or join lines.
- If there is an active selection, the method deletes the selection instead and returns
  (consistent with all other deletion operations).
- The method carries a `// Spec: docs/trunk/SPEC.md#word-model` comment and calls
  `word_boundary_right` rather than reimplementing scan logic.
- `DeleteForwardWord` exists in the `Command` enum in `buffer_target.rs`.
- `convert_key` in `metal_view.rs` uses `event.charactersIgnoringModifiers()` when the
  Option modifier is held (mirroring the existing Control modifier handling), so that
  Option+D produces `Key::Char('d')` with `mods.option=true` rather than the macOS-composed
  character `'ð'` (eth). Without this, the InsertChar arm fires instead of DeleteForwardWord.
- `resolve_command` maps `Option+'d'` → `DeleteForwardWord`, checked before any plain
  `Key::Char('d')` arm.
- `execute_command` calls `ctx.buffer.delete_forward_word()`, marks dirty, and ensures
  cursor visible.
- Unit tests cover: cursor mid-word on non-whitespace (deletes to word end), cursor on
  whitespace between words (deletes whitespace run only), cursor at line end (no-op),
  cursor at line start (deletes first run), active selection (deletes selection not
  word), line containing only whitespace.

