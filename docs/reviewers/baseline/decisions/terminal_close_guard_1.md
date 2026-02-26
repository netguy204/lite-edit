---
decision: APPROVE
summary: All success criteria satisfied; implementation follows established confirm dialog patterns and integrates cleanly with existing close_tab flow.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Pressing Cmd+W on a terminal tab with a running process shows the confirmation dialog before closing

- **Status**: satisfied
- **Evidence**: `close_tab()` at editor_state.rs:3076-3081 checks `is_terminal_with_active_process()` and calls `show_terminal_close_confirm()` when true.

### Criterion 2: Pressing Cmd+W on a terminal tab with no attached process (idle/exited) closes immediately

- **Status**: satisfied
- **Evidence**: The terminal check in `close_tab()` only shows the dialog if `is_terminal_with_active_process()` returns true; otherwise falls through to the immediate close logic.

### Criterion 3: The confirmation dialog uses appropriate wording (e.g., "Kill running process?") distinct from the dirty-file dialog

- **Status**: satisfied
- **Evidence**: `show_terminal_close_confirm()` at editor_state.rs:1379-1382 uses `ConfirmDialog::with_labels("Kill running process?", "Cancel", "Kill")` - distinct from the dirty-file dialog's "Abandon unsaved changes?" wording.

### Criterion 4: A new `ConfirmDialogContext` variant (e.g., `CloseActiveTerminal`) handles the terminal-specific confirmation flow

- **Status**: satisfied
- **Evidence**: `CloseActiveTerminal { pane_id, tab_idx }` variant added to `ConfirmDialogContext` enum at confirm_dialog.rs:71-79 with chunk backreference. Tests verify storage and cloning behavior.

### Criterion 5: Process liveness is determined via existing `TerminalBuffer::try_wait()` / `process_id()` methods

- **Status**: satisfied
- **Evidence**: `is_terminal_with_active_process()` at editor_state.rs:1397-1416 calls `term.try_wait().is_none()` to detect running processes.

### Criterion 6: Mouse clicks on the terminal tab close button follow the same guard logic

- **Status**: satisfied
- **Evidence**: `handle_tab_bar_click()` at editor_state.rs:3496-3497 calls `self.close_tab(tab_index)` when `is_close_button` is true, routing through the same guard logic as Cmd+W.

### Criterion 7: Confirming the dialog kills the process and closes the tab

- **Status**: satisfied
- **Evidence**: `handle_confirm_dialog_confirmed()` at editor_state.rs:1344-1346 routes `CloseActiveTerminal` to `kill_terminal_and_close_tab()`, which calls `term.kill()` then `force_close_tab()` (lines 1422-1434).

### Criterion 8: Cancelling the dialog returns focus to the terminal

- **Status**: satisfied
- **Evidence**: `close_confirm_dialog()` at editor_state.rs:1354-1359 sets `focus = EditorFocus::Buffer`, which is the correct focus mode for both file and terminal tabs in this codebase.
