---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/input.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/buffer_target.rs
  - crates/buffer/src/text_buffer.rs
code_references:
  - ref: crates/editor/src/input.rs#MouseEvent
    implements: "click_count field for double-click detection"
  - ref: crates/editor/src/metal_view.rs#MetalView::convert_mouse_event
    implements: "Extract NSEvent.clickCount and populate MouseEvent.click_count"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::handle_mouse
    implements: "Double-click word selection dispatch (click_count == 2 handling)"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::select_word_at
    implements: "Word selection using word boundary helpers"
narrative: word_nav
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- word_boundary_primitives
- word_jump_navigation
created_after:
- delete_backward_word
- file_picker
- fuzzy_file_matcher
- selector_rendering
- selector_widget
---

# Chunk Goal

## Minor Goal

Add double-click word selection: when the user double-clicks anywhere on a word, the
entire word is selected instantly. This is among the most fundamental text-editor
interactions on macOS — used constantly for selecting identifiers, copying words, and
beginning drag-to-extend selections — and its absence is immediately noticeable.

The selection follows `docs/trunk/SPEC.md#word-model`: the word boundaries are found
by calling `word_boundary_left` and `word_boundary_right` on the character class at
the click position. Double-clicking whitespace selects the whitespace run (consistent
with the character-class model rather than silently doing nothing).

This chunk depends on `word_jump_navigation` because that chunk establishes the pattern
for exposing `word_boundary_left` / `word_boundary_right` to call sites outside
`TextBuffer` (specifically `buffer_target.rs`'s mouse handler); the visibility
decision made there should be reused here rather than solved a second time.

## Success Criteria

- `MouseEvent` or `MouseEventKind` carries a `click_count: u32` (or equivalent)
  populated from `NSEvent.clickCount` in `metal_view.rs`. Single clicks continue to
  behave exactly as before (`click_count == 1`).
- In `buffer_target.rs`'s `handle_mouse`, a `Down` event with `click_count == 2`:
  1. Converts pixel position to a buffer `Position` via `pixel_to_buffer_position`.
  2. Gets the current line's characters.
  3. Calls `word_boundary_left(chars, col + 1)` for the selection start and
     `word_boundary_right(chars, col)` for the selection end, where both use the
     character class of `chars[col]`.
  4. Sets the selection anchor at the word start and the cursor at the word end via
     `set_selection_anchor` + `move_cursor_preserving_selection`.
  5. Marks the cursor line dirty.
- Double-clicking on whitespace selects the contiguous whitespace run.
- Double-clicking on an empty line is a no-op (no panic, no selection).
- Double-clicking at or past the end of a line selects the last run on that line (or
  is a no-op if the line is empty).
- A `// Spec: docs/trunk/SPEC.md#word-model` comment is present at the word boundary
  call site in `buffer_target.rs`.
- Integration tests in `buffer_target.rs` cover: double-click mid-word selects full
  word, double-click at word start selects word, double-click on whitespace selects
  whitespace run, double-click on empty line is a no-op.
- All existing single-click and drag-selection tests continue to pass.

