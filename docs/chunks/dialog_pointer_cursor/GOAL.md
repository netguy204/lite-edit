---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/drain_loop.rs
code_references:
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::update_cursor_regions
    implements: "Pointer cursor regions for confirm dialog buttons"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- dragdrop_file_paste
- vsplit_scroll
- workspace_initial_terminal
- workspace_session_persistence
---

# Chunk Goal

## Minor Goal

When a yes/no confirmation dialog is visible, hovering over either button should
display a pointer cursor (hand) instead of the default I-beam cursor. This
matches the cursor behavior already applied to other clickable regions (the left
rail, tab bar, and selector overlay items) and gives users a clear visual signal
that the buttons are interactive.

The fix lives in `update_cursor_regions` in `drain_loop.rs`. When
`EditorFocus::ConfirmDialog` is active, compute the confirm dialog geometry and
register pointer-cursor `CursorRect` regions for each button.

## Success Criteria

- Hovering over the Cancel or Confirm button in the yes/no dialog displays the
  system pointer cursor.
- Hovering over the dialog panel background (outside both buttons) does not show
  the pointer cursor.
- Moving the mouse off the dialog (back to the text buffer) restores the I-beam
  cursor as before.
- No change to existing cursor behavior for the rail, tab bar, selector overlay,
  or buffer content area.