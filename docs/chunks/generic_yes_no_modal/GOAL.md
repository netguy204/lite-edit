---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/confirm_dialog.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogContext
    implements: "Context enum for routing confirm dialog outcomes to the correct handler"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialog::with_labels
    implements: "Custom button label constructor for parameterized dialogs"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogGeometry::is_cancel_button
    implements: "Hit testing for cancel button mouse clicks"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogGeometry::is_confirm_button
    implements: "Hit testing for confirm button mouse clicks"
  - ref: crates/editor/src/confirm_dialog.rs#calculate_confirm_dialog_geometry
    implements: "Geometry calculation accepting dialog reference for dynamic labels"
  - ref: crates/editor/src/editor_state.rs#EditorState::confirm_context
    implements: "Replaced pending_close with typed context field"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_confirm_dialog_confirmed
    implements: "Context-based outcome routing (dispatch on ConfirmDialogContext variant)"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_confirm_dialog
    implements: "Mouse click handling for confirm dialog buttons"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- dirty_tab_close_confirm
created_after:
- dirty_tab_close_confirm
---

# Chunk Goal

## Minor Goal

The `dirty_tab_close_confirm` chunk introduces a `ConfirmDialog` widget and a hardcoded integration for closing dirty tabs. This future chunk generalizes that into a reusable yes/no modal system.

Currently the `EditorState` has a `pending_close: Option<(PaneId, usize)>` field that tightly couples the dialog to the tab-close use case. This chunk replaces that with a context/action enum so the same dialog infrastructure supports multiple use cases: quit confirmation with dirty tabs, reload-from-disk prompts, and other binary decisions.

## Success Criteria

- Replace `pending_close` with a `ConfirmDialogContext` enum that routes outcomes to the correct handler
- Support at minimum: close-dirty-tab, quit-with-dirty-tabs contexts
- Mouse click support for dialog buttons (hit testing the button rects)
- All existing confirm dialog tests still pass
- New tests for mouse interaction and context routing