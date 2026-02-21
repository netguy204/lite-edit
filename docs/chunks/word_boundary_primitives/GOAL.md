---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/buffer/src/text_buffer.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#word_boundary_left
    implements: "Backward word boundary scanning using whitespace classification"
  - ref: crates/buffer/src/text_buffer.rs#word_boundary_right
    implements: "Forward word boundary scanning using whitespace classification"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_backward_word
    implements: "Word deletion using word_boundary_left primitive"
narrative: word_nav
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- delete_backward_word
- file_picker
- fuzzy_file_matcher
- selector_rendering
- selector_widget
---

# Chunk Goal

## Minor Goal

Extract word boundary scanning out of `delete_backward_word`'s inline loop and into
two private helper functions in `crates/buffer/src/text_buffer.rs`, establishing the
shared implementation foundation for all word-oriented features that follow.

`delete_backward_word` currently hard-codes a character-class scan loop directly inside
the method body. The upcoming `move_word_left`, `move_word_right`, `delete_forward_word`,
and double-click selection all require the same scanning primitive. Without extraction,
each feature would duplicate the same ~15-line scan, and a future change to the word
definition (e.g. treating `_` as a word separator) would require finding and updating
five separate places.

This chunk makes the word model defined in `docs/trunk/SPEC.md#word-model` concrete
in code: two small, pure, independently testable functions that every subsequent
word-oriented chunk calls.

## Success Criteria

- `fn word_boundary_left(chars: &[char], col: usize) -> usize` exists as a private
  function in `text_buffer.rs`. Given `chars` and a cursor column `col`, it returns
  the start column of the contiguous run containing `chars[col - 1]`, using
  `char::is_whitespace()` as the sole classifier. Returns `col` unchanged when
  `col == 0`.
- `fn word_boundary_right(chars: &[char], col: usize) -> usize` exists as a private
  function in `text_buffer.rs`. Given `chars` and `col`, it returns the first column
  past the end of the contiguous run starting at `chars[col]`. Returns `col` unchanged
  when `col >= chars.len()`.
- Both helpers carry a `// Spec: docs/trunk/SPEC.md#word-model` comment.
- `delete_backward_word` produces identical behaviour to before, now implemented by
  calling `word_boundary_left` instead of its former inline scan loop.
- Direct unit tests for each helper cover: empty slice, single-character run,
  full-line run (all one class), a non-whitespace run surrounded by whitespace,
  a whitespace run surrounded by non-whitespace, `col` at 0, `col` at `chars.len()`,
  and `col` in the middle of a run.
- All existing `delete_backward_word` tests continue to pass without modification.