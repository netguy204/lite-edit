---
status: ACTIVE
advances_trunk_goal: "Required Properties: Standard text editor interaction patterns"
proposed_chunks:
  - prompt: "Set the mouse cursor to an I-beam (text cursor) when the mouse is over the editable area of the window. Override resetCursorRects on MetalView to add an IBeam cursor rect covering the full view bounds. This is a macOS NSView API — call addCursorRect:cursor: with NSCursor.iBeam in the resetCursorRects override."
    chunk_directory: ibeam_cursor
    depends_on: []
  - prompt: "Add Alt+Backspace (Option+Delete) to delete backward by word. A word boundary is defined by character class: if the cursor is on non-whitespace, the word extends backward through contiguous non-whitespace; if on whitespace, it extends backward through contiguous whitespace. Add a delete_backward_word method to TextBuffer that scans backward from cursor to find the word start using this class-based rule, deletes the range, and returns DirtyLines. Wire through Command enum and resolve_command in BufferFocusTarget (Option+Backspace maps to Key::Backspace with option modifier)."
    chunk_directory: delete_backward_word
    depends_on: []
  - prompt: "Add Cmd+Backspace to delete from cursor to the beginning of the current line. Add a delete_to_line_start method to TextBuffer that removes characters from cursor back to column 0 of the current line and returns DirtyLines. Wire through Command enum and resolve_command in BufferFocusTarget (Cmd+Backspace maps to Key::Backspace with command modifier)."
    chunk_directory: delete_to_line_start
    depends_on: []
created_after:
  - editor_qol_interactions
---

## Advances Trunk Goal

This narrative advances standard text editor interaction patterns — specifically, visual cursor feedback (I-beam cursor over editable area) and common deletion shortcuts (Alt+Backspace for word deletion, Cmd+Backspace for line-start deletion) that macOS users expect in any text editor.

## Driving Ambition

lite-edit is missing several polish interactions that every macOS text editor provides. The mouse cursor remains the default arrow pointer when hovering over the editable text area — it should be an I-beam to signal editability. Alt+Backspace (Option+Delete) should delete from the cursor back to the start of the current word, and Cmd+Backspace should delete from the cursor back to the start of the current line. These are standard macOS text editing conventions and their absence creates friction for users with muscle memory from other editors.

## Chunks

1. **I-beam cursor over editable area** — Override `resetCursorRects` on MetalView to set an I-beam cursor when the mouse is over the view. Pure macOS view-layer change, no buffer logic needed.

2. **Alt+Backspace word deletion** — Add `delete_backward_word` to TextBuffer using a character-class word boundary rule (non-whitespace vs whitespace), then wire through the command resolution pipeline.

3. **Cmd+Backspace delete to line start** — Add `delete_to_line_start` to TextBuffer, wire through command resolution.

## Completion Criteria

When complete:
- The mouse cursor displays as an I-beam when hovering over the editor's text area
- Alt+Backspace deletes from the cursor back to the word boundary (non-whitespace eats non-whitespace, whitespace eats whitespace)
- Cmd+Backspace deletes from the cursor to the beginning of the current line
- All three behaviors match standard macOS text editor conventions
