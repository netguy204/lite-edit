---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
  - text_selection_model
created_after: ["editable_buffer", "glyph_rendering", "metal_surface", "viewport_rendering"]
---

# Shift+Arrow Key Selection

## Minor Goal

Enable text selection via Shift+arrow keys, the standard keyboard-driven selection mechanism in virtually every text editor. When the user holds Shift and presses an arrow key, a selection anchor is placed (if not already set) and the cursor extends the selection in the arrow direction. The selection persists after Shift is released — it remains visible and active until dismissed by a plain click, a non-shift cursor movement, or a new selection.

This builds on the text selection model chunk which provides the anchor/cursor selection API on TextBuffer. The key insight is that the existing `move_*` methods clear selection unconditionally — this chunk needs Shift-modified movement variants that preserve and extend the selection instead.

## Success Criteria

- **Shift+Arrow commands**: Add selection-extending variants to the `Command` enum in `buffer_target.rs`:
  - `SelectLeft` — extend selection one character left
  - `SelectRight` — extend selection one character right
  - `SelectUp` — extend selection one line up
  - `SelectDown` — extend selection one line down
  - `SelectToLineStart` — extend selection to beginning of line (Shift+Home, Shift+Cmd+Left)
  - `SelectToLineEnd` — extend selection to end of line (Shift+End, Shift+Cmd+Right)
  - `SelectToBufferStart` — extend selection to buffer start (Shift+Cmd+Up)
  - `SelectToBufferEnd` — extend selection to buffer end (Shift+Cmd+Down)

- **Key bindings in `resolve_command`**: When Shift is held alongside an arrow key or movement chord, resolve to the `Select*` variant instead of the `Move*` variant:
  - Shift+Left → `SelectLeft`
  - Shift+Right → `SelectRight`
  - Shift+Up → `SelectUp`
  - Shift+Down → `SelectDown`
  - Shift+Home / Shift+Cmd+Left → `SelectToLineStart`
  - Shift+End / Shift+Cmd+Right → `SelectToLineEnd`
  - Shift+Cmd+Up → `SelectToBufferStart`
  - Shift+Cmd+Down → `SelectToBufferEnd`

- **Selection extension logic in `execute_command`**: Each `Select*` command should:
  1. If no selection anchor is set, set the anchor at the current cursor position (`buffer.set_selection_anchor_at_cursor()`)
  2. Move the cursor using the corresponding `move_*` method (which, per the selection model, clears the selection) — **but** the anchor must be preserved across the move. This means either:
     - The `Select*` execution saves the anchor, calls `move_*`, then restores the anchor
     - Or add `move_*_preserving_selection` variants on TextBuffer that don't clear the anchor
     - Or add a flag/parameter to movement methods to skip selection clearing
  3. Mark affected lines dirty (the old and new selection extents)

- **Selection persists after Shift release**: The selection remains active when Shift is released. It is only cleared by:
  - A non-Shift cursor movement (plain arrow key, Home/End without Shift) — already handled by the selection model's "cursor movement clears selection" behavior
  - A mouse click — already handled by the mouse_click_cursor chunk setting cursor position
  - A mutation (typing replaces selection, per selection model)
  - Explicit clear (e.g., Escape, if wired)

- **Extending an existing selection**: If a selection is already active (e.g., from a previous Shift+Arrow or mouse drag), pressing Shift+Arrow extends it further — the anchor stays where it was originally placed, and only the cursor moves.

- **Shift+Arrow with Ctrl modifiers**: Shift+Ctrl+A should extend selection to line start. Shift+Ctrl+E should extend selection to line end. These combine the Shift selection behavior with the Emacs-style line navigation.

- **Unit tests**:
  - Shift+Right from no selection: creates selection of 1 character
  - Shift+Right×3: selects 3 characters from the starting position
  - Shift+Left after Shift+Right: shrinks the selection
  - Shift+Down: extends selection to next line
  - Plain Right after Shift+Right×3: clears selection and moves cursor
  - Shift+Home: selects from cursor to line start
  - Shift+End: selects from cursor to line end
  - Selection persists when no keys are pressed (shift release doesn't clear)
  - Existing mouse-drag selection can be extended with Shift+Arrow
