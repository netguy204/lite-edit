---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/confirm_dialog.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/selector_overlay.rs
- crates/editor/src/renderer.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmButton
    implements: "Enum representing Cancel/Abandon button selection states"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmOutcome
    implements: "Outcome enum for dialog key handling (Cancelled/Confirmed/Pending)"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialog
    implements: "Pure state struct for the confirmation dialog widget (Humble View Architecture)"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialog::handle_key
    implements: "Keyboard navigation handling (Tab/Left/Right toggle, Enter confirm, Escape cancel)"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogGeometry
    implements: "Computed geometry for dialog overlay positioning"
  - ref: crates/editor/src/confirm_dialog.rs#calculate_confirm_dialog_geometry
    implements: "Pure function to calculate dialog overlay positioning"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogGlyphBuffer
    implements: "Metal GPU buffer management for dialog rendering"
  - ref: crates/editor/src/editor_state.rs#EditorFocus::ConfirmDialog
    implements: "Focus variant for routing keyboard input to the dialog"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key_confirm_dialog
    implements: "Key event handling when confirm dialog is focused"
  - ref: crates/editor/src/editor_state.rs#EditorState::show_confirm_dialog
    implements: "Triggers confirmation dialog for dirty tab close"
  - ref: crates/editor/src/editor_state.rs#EditorState::close_confirm_dialog
    implements: "Closes dialog and returns focus to buffer"
  - ref: crates/editor/src/editor_state.rs#EditorState::force_close_tab
    implements: "Closes tab unconditionally (bypasses dirty check)"
  - ref: crates/editor/src/renderer.rs#Renderer::draw_confirm_dialog
    implements: "Metal rendering of confirm dialog overlay"
  - ref: crates/editor/src/renderer.rs#Renderer::render_with_confirm_dialog
    implements: "Render entry point when confirm dialog is active"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- viewport_emacs_navigation
- pane_scroll_isolation
---

# Chunk Goal

## Minor Goal

When a tab has unsaved changes (dirty bit set), it currently cannot be closed — the close action silently refuses. Users have no way to abandon changes without saving first.

This chunk adds a confirmation dialog that appears when closing a dirty tab. The user can choose to abandon changes (closing the tab without saving) or cancel (keeping the tab open). This removes a hard blocker in the editing workflow where the only option for an unwanted dirty tab is to save it.

The dialog is rendered in-engine as a Metal overlay (not a native macOS NSAlert), consistent with the existing selector overlay aesthetic.

## Design Decisions

- **In-engine overlay rendering** — The dialog uses the same Metal glyph rendering pipeline as the selector overlay (dark panel, Catppuccin Mocha colors). Native macOS alerts were rejected to maintain visual consistency.
- **Dedicated ConfirmDialog widget** — A new `ConfirmDialog` struct following the Humble View Architecture (pure state, no platform dependencies), separate from `SelectorWidget`. The selector is for filterable lists; this is a binary choice with no query input.
- **EditorFocus::ConfirmDialog variant** — New focus variant routes keyboard input to the dialog. Enter confirms the selected button, Escape always cancels, Tab/Left/Right toggle between buttons.
- **Default selection is Cancel** — Safe default; user must deliberately navigate to "Abandon" to confirm.
- **Just close, dirty flag irrelevant** — When the user confirms, the tab is removed entirely. No need to clear the dirty flag first since the tab is destroyed.
- **Keyboard only** — Mouse click on dialog buttons is out of scope for this chunk.
- **pending_close tracking** — `EditorState` gains a `pending_close: Option<(PaneId, usize)>` field to remember which tab triggered the dialog while it's displayed.

## Success Criteria

- Attempting to close a dirty tab shows a centered overlay dialog with "Abandon unsaved changes?" prompt and two buttons: "Cancel" (selected by default) and "Abandon"
- Pressing Escape or Enter-on-Cancel dismisses the dialog and leaves the tab open and dirty
- Pressing Tab to select "Abandon" then Enter closes the tab without saving
- Non-dirty tabs close immediately with no dialog (existing behavior preserved)
- The dialog blocks other interactions (file picker, find, etc.) while open
- All existing tests pass, plus new unit tests for:
  - ConfirmDialog widget key handling (default selection, toggle, outcomes)
  - Confirm dialog geometry calculation
  - EditorState integration (dialog opens on dirty close, confirm closes tab, cancel keeps tab)
- `cargo clippy -p lite-edit -- -D warnings` passes

## Rejected Ideas

### Extend SelectorWidget for yes/no

Reuse the existing `SelectorWidget` with two items ("Abandon changes" / "Cancel").

Rejected because: The selector is for filterable lists with a query input. Using it for a binary choice would show a search box for a yes/no question — a visual and conceptual mismatch.

### Inline state on EditorState (no widget)

Add a `pending_close_tab: Option<(PaneId, usize)>` field and render an overlay when `Some`. Handle Y/N keys directly in the main key handler.

Rejected because: Not reusable for future yes/no modals (quit confirmation, reload-from-disk, etc.). A dedicated widget creates the right abstraction.

### Native macOS NSAlert

Use the system alert sheet/panel for the confirmation.

Rejected because: Breaks visual consistency with the editor's in-engine overlay aesthetic. The selector overlay and find strip are both Metal-rendered; this dialog should match.