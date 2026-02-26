---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/confirm_dialog.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogContext::CloseActiveTerminal
    implements: "Terminal close guard context variant"
  - ref: crates/editor/src/editor_state.rs#EditorState::is_terminal_with_active_process
    implements: "Process liveness detection"
  - ref: crates/editor/src/editor_state.rs#EditorState::show_terminal_close_confirm
    implements: "Terminal-specific confirmation dialog"
  - ref: crates/editor/src/editor_state.rs#EditorState::kill_terminal_and_close_tab
    implements: "Kill process and close tab on confirmation"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- fallback_glyph_metrics
---

# Chunk Goal

## Minor Goal

When the user presses Cmd+W to close a terminal tab, display a confirmation dialog if the terminal's PTY is attached to a running process. If the TTY is idle (no process attached or the process has already exited), close the tab immediately without prompting.

Currently, `close_tab()` only guards against closing tabs with unsaved buffer changes (dirty files). Terminal tabs with active processes (e.g., a running build, SSH session, or interactive program) are closed without warning, which can kill the process and lose work.

This chunk extends the existing `ConfirmDialog` infrastructure with a new `ConfirmDialogContext` variant for closing active terminal tabs, and adds process-liveness detection to the close-tab flow.

## Success Criteria

- Pressing Cmd+W on a terminal tab with a running process shows the confirmation dialog before closing
- Pressing Cmd+W on a terminal tab with no attached process (idle/exited) closes immediately
- The confirmation dialog uses appropriate wording (e.g., "Kill running process?") distinct from the dirty-file dialog
- A new `ConfirmDialogContext` variant (e.g., `CloseActiveTerminal`) handles the terminal-specific confirmation flow
- Process liveness is determined via existing `TerminalBuffer::try_wait()` / `process_id()` methods
- Mouse clicks on the terminal tab close button follow the same guard logic
- Confirming the dialog kills the process and closes the tab
- Cancelling the dialog returns focus to the terminal