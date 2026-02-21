---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
  - crates/buffer/src/text_buffer.rs
  - crates/buffer/src/types.rs
code_references: []
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
# Text Selection Model

## Minor Goal

Add a selection model to `TextBuffer` so the editor can track, query, and act on selected text. This is the foundational data model that mouse drag selection, Cmd+A select-all, Cmd+C copy, Cmd+V paste (replacing selection), and selection rendering all depend on. Without this, the editor has no concept of "selected text."

The model uses an anchor-cursor approach: an optional anchor position marks where selection started, and the current cursor position marks where it ends. The range between them (in either direction) is the selection. When no anchor is set, there is no selection.

## Success Criteria

- **Selection anchor on TextBuffer**: Add an `Option<Position>` field `selection_anchor` to `TextBuffer`. When `Some`, the selection spans from anchor to cursor (the anchor may come before or after the cursor — both directions are valid).

- **Selection API methods on TextBuffer**:
  - `set_selection_anchor(pos: Position)` — sets the anchor, clamped to valid bounds
  - `set_selection_anchor_at_cursor()` — sets the anchor to the current cursor position (convenience for starting a selection)
  - `clear_selection()` — clears the anchor (no selection)
  - `has_selection() -> bool` — returns true if anchor is set and differs from cursor
  - `selection_range() -> Option<(Position, Position)>` — returns `(start, end)` in document order (start <= end), regardless of which direction the selection was made
  - `selected_text() -> Option<String>` — returns the text within the selection range
  - `select_all()` — sets anchor to buffer start and cursor to buffer end

- **Mutations delete selection first**: When a selection is active, the following operations should delete the selected text before performing their action:
  - `insert_char` — delete selection, then insert the character at the resulting cursor
  - `insert_newline` — delete selection, then insert newline
  - `insert_str` — delete selection, then insert string
  - `delete_backward` — if selection active, delete selection (don't delete an additional character)
  - `delete_forward` — if selection active, delete selection (don't delete an additional character)

- **Delete selection helper**: Add a `delete_selection() -> DirtyLines` method that removes the text between anchor and cursor, places the cursor at the start of the former selection, clears the anchor, and returns the appropriate dirty lines.

- **Cursor movement clears selection**: All `move_*` methods (`move_left`, `move_right`, `move_up`, `move_down`, `move_to_line_start`, `move_to_line_end`, `move_to_buffer_start`, `move_to_buffer_end`) should clear the selection when called. (Shift+movement for extending selection is a future concern.)

- **`set_cursor` clears selection**: Calling `set_cursor` clears any active selection.

- **Unit tests**: Comprehensive tests covering:
  - Setting and clearing selection anchor
  - `selection_range` returns correct ordered range for forward and backward selections
  - `selected_text` returns correct content for single-line and multi-line selections
  - `insert_char` with active selection replaces the selection
  - `delete_backward` with active selection deletes only the selection
  - `select_all` selects the entire buffer
  - Cursor movement clears selection
  - Edge cases: selection at buffer boundaries, empty selection (anchor == cursor)
