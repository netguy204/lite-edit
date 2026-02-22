---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::overlay_cursor_visible
    implements: "Overlay cursor visibility state for blink animation"
  - ref: crates/editor/src/editor_state.rs#EditorState::last_overlay_keystroke
    implements: "Keystroke timestamp tracking for overlay cursor blink reset"
  - ref: crates/editor/src/editor_state.rs#EditorState::toggle_cursor_blink
    implements: "Focus-aware cursor blink toggle - only toggles cursor for focused area"
  - ref: crates/editor/src/main.rs#EditorController::render_if_dirty
    implements: "Focus-aware cursor visibility passing to renderer"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- file_search_path_matching
---

# Chunk Goal

## Minor Goal

The blinking cursor should only appear in the currently focused buffer. When an overlay like the file picker is active, the cursor in the main edit buffer should stop blinking (render as static or hidden), and only the cursor in the picker's mini buffer should blink. This ensures clear visual feedback about which buffer has input focus.

## Success Criteria

- When the file picker (or any overlay) is open, the main edit buffer's cursor does not blink
- The mini buffer cursor blinks when the mini buffer has focus
- When the overlay is dismissed, the main edit buffer's cursor resumes blinking
- No visual glitches during focus transitions (no double-blink frame, no missing cursor)