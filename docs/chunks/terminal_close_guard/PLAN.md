<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk extends the existing `ConfirmDialog` infrastructure to guard terminal tabs with running processes from accidental closure. The implementation follows the same Humble View Architecture and patterns established by `dirty_tab_close_confirm` and `generic_yes_no_modal`.

The core changes are:

1. **New `ConfirmDialogContext` variant**: Add `CloseActiveTerminal { pane_id, tab_idx }` to handle terminal-specific confirmation flow with appropriate wording.

2. **Process liveness detection**: Use `TerminalBuffer::try_wait()` to determine if the PTY has an active process before closing.

3. **Integration into `close_tab()` flow**: After the dirty-file check but before immediate close, check if the tab is a terminal with an active process and show the appropriate dialog.

4. **Confirmation handler**: When confirmed, call `TerminalBuffer::kill()` to terminate the process before closing the tab.

The existing infrastructure handles:
- Dialog widget state and keyboard navigation (`ConfirmDialog`)
- Focus routing (`EditorFocus::ConfirmDialog`)
- Rendering (`ConfirmDialogGlyphBuffer`)
- Context-based outcome dispatch (`handle_confirm_dialog_confirmed`)

This chunk only needs to:
- Add the new context variant
- Add the liveness check in `close_tab()`
- Add the outcome handler for the new variant

Testing follows TDD per `TESTING_PHILOSOPHY.md`: write failing tests first for the pure logic (process liveness detection, context variant behavior), then implement.

## Subsystem Considerations

No subsystems are directly relevant to this chunk. The renderer subsystem (`docs/subsystems/renderer`) handles GPU rendering, but this chunk only adds logic to the existing confirm dialog infrastructure without modifying rendering code.

## Sequence

### Step 1: Add `CloseActiveTerminal` variant to `ConfirmDialogContext` (TDD)

Add a new variant to the `ConfirmDialogContext` enum in `crates/editor/src/confirm_dialog.rs`.

**Tests first** (add to `#[cfg(test)]` module):
- `test_context_close_active_terminal_stores_pane_and_index`
- `test_context_close_active_terminal_is_clone`

**Implementation**:
```rust
/// Context for what triggered the confirm dialog and what action to take on confirmation.
pub enum ConfirmDialogContext {
    /// Closing a tab with unsaved changes.
    CloseDirtyTab { pane_id: PaneId, tab_idx: usize },
    /// Quitting the application with dirty tabs.
    QuitWithDirtyTabs { dirty_count: usize },
    /// Closing a terminal tab with a running process.
    // Chunk: docs/chunks/terminal_close_guard - Terminal process guard context
    CloseActiveTerminal { pane_id: PaneId, tab_idx: usize },
}
```

Location: `crates/editor/src/confirm_dialog.rs`

### Step 2: Add helper method to check terminal process liveness

Add a helper method in `editor_state.rs` to check if a tab at a given index is a terminal with an active process.

**Tests first**:
- `test_is_terminal_with_active_process_returns_false_for_file_tab`
- `test_is_terminal_with_active_process_returns_false_for_exited_terminal`
- `test_is_terminal_with_active_process_returns_true_for_running_terminal`

The method should:
1. Get the tab at the given index
2. Check if it's a terminal tab (`kind == TabKind::Terminal`)
3. Get the `TerminalBuffer` via `as_terminal_buffer_mut()`
4. Call `try_wait()` — if it returns `None`, the process is still running
5. Return `true` if running, `false` otherwise

**Implementation signature**:
```rust
/// Checks if the tab at `index` in `pane_id` is a terminal with an active process.
///
/// Returns `true` if the tab is a terminal and `try_wait()` returns `None` (process running).
/// Returns `false` for file tabs, exited terminals, or tabs without a PTY.
// Chunk: docs/chunks/terminal_close_guard - Process liveness detection
fn is_terminal_with_active_process(&mut self, pane_id: PaneId, index: usize) -> bool
```

Location: `crates/editor/src/editor_state.rs`

Note: This requires mutable access because `try_wait()` may reap a zombie process (standard POSIX behavior). We check the pane by ID rather than assuming active pane to support future multi-pane scenarios.

### Step 3: Add terminal-specific confirmation dialog helper

Add a helper method similar to `show_confirm_dialog()` but with terminal-specific wording.

**Implementation**:
```rust
/// Shows a confirmation dialog for closing a terminal with an active process.
///
/// Uses terminal-specific wording ("Kill running process?") and the
/// `CloseActiveTerminal` context variant.
// Chunk: docs/chunks/terminal_close_guard - Terminal close confirmation
fn show_terminal_close_confirm(&mut self, pane_id: PaneId, tab_idx: usize) {
    self.confirm_dialog = Some(ConfirmDialog::with_labels(
        "Kill running process?",
        "Cancel",
        "Kill",
    ));
    self.confirm_context = Some(ConfirmDialogContext::CloseActiveTerminal { pane_id, tab_idx });
    self.focus = EditorFocus::ConfirmDialog;
    self.dirty_region.merge(DirtyRegion::FullViewport);
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 4: Modify `close_tab()` to check terminal process liveness (TDD)

Update `close_tab()` to check for active terminal processes after the dirty-file check.

**Tests first** (add to `editor_state.rs` tests):
- `test_close_terminal_with_active_process_shows_confirm_dialog`
- `test_close_terminal_with_exited_process_closes_immediately`
- `test_close_file_tab_does_not_check_process_liveness`

**Implementation logic** (insert after the dirty check block):
```rust
// Chunk: docs/chunks/terminal_close_guard - Check terminal process liveness
// Check if this is a terminal with an active process
let active_terminal_pane_id = self.editor
    .active_workspace()
    .and_then(|ws| ws.active_pane())
    .and_then(|pane| {
        pane.tabs.get(index).and_then(|tab| {
            if tab.kind == TabKind::Terminal {
                Some(pane.id)
            } else {
                None
            }
        })
    });

if let Some(pane_id) = active_terminal_pane_id {
    if self.is_terminal_with_active_process(pane_id, index) {
        self.show_terminal_close_confirm(pane_id, index);
        return;
    }
}
```

Location: `crates/editor/src/editor_state.rs`, inside `close_tab()`

### Step 5: Handle `CloseActiveTerminal` in confirmation outcome (TDD)

Update `handle_confirm_dialog_confirmed()` to handle the new context variant.

**Tests first**:
- `test_confirm_terminal_close_kills_process_and_closes_tab`
- `test_cancel_terminal_close_preserves_tab_and_process`

**Implementation**:
```rust
// In handle_confirm_dialog_confirmed():
match ctx {
    ConfirmDialogContext::CloseDirtyTab { pane_id, tab_idx } => {
        self.force_close_tab(pane_id, tab_idx);
    }
    ConfirmDialogContext::QuitWithDirtyTabs { .. } => {
        self.should_quit = true;
    }
    // Chunk: docs/chunks/terminal_close_guard - Kill process and close terminal
    ConfirmDialogContext::CloseActiveTerminal { pane_id, tab_idx } => {
        self.kill_terminal_and_close_tab(pane_id, tab_idx);
    }
}
```

Add helper method:
```rust
/// Kills the terminal process and closes the tab.
///
/// This is called after the user confirms closing a terminal with an active process.
// Chunk: docs/chunks/terminal_close_guard - Terminal process termination
fn kill_terminal_and_close_tab(&mut self, pane_id: PaneId, tab_idx: usize) {
    // Kill the process first
    if let Some(workspace) = self.editor.active_workspace_mut() {
        if let Some(pane) = workspace.pane_by_id_mut(pane_id) {
            if let Some(tab) = pane.tabs.get_mut(tab_idx) {
                if let Some(term) = tab.as_terminal_buffer_mut() {
                    let _ = term.kill(); // Ignore errors - we're closing anyway
                }
            }
        }
    }
    // Then close the tab using existing force_close logic
    self.force_close_tab(pane_id, tab_idx);
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 6: Update `force_close_tab` to handle pane_id (if needed)

Review `force_close_tab()` to ensure it can work with a specific `pane_id`. Currently it may only operate on the active pane. If needed, update it to:
1. Look up the pane by `pane_id`
2. Close the tab at `tab_idx` in that specific pane

This ensures the correct tab is closed even if focus changed while the dialog was open.

Location: `crates/editor/src/editor_state.rs`

### Step 7: Integration test and clippy

Run the full test suite and clippy:
```bash
cargo test -p lite-edit
cargo test -p lite-edit-terminal
cargo clippy -p lite-edit -- -D warnings
cargo clippy -p lite-edit-terminal -- -D warnings
```

**Manual verification checklist**:
1. Open a terminal tab and start a long-running process (e.g., `sleep 100` or `top`)
2. Press Cmd+W → Dialog appears with "Kill running process?" and Cancel/Kill buttons
3. Press Escape → Dialog closes, terminal remains, process continues
4. Press Cmd+W again → Dialog reappears
5. Press Tab then Enter (or click Kill) → Terminal closes, process terminated
6. Open a terminal tab, let shell idle (no foreground process)
7. Press Cmd+W → Terminal closes immediately without dialog
8. Open a file, edit it (dirty), press Cmd+W → "Abandon unsaved changes?" dialog (unchanged behavior)
9. Click the close button (×) on a terminal tab with active process → Same dialog behavior as Cmd+W

### Step 8: Update code_paths in GOAL.md frontmatter

Update the chunk's GOAL.md frontmatter to include the files touched:
```yaml
code_paths:
  - crates/editor/src/confirm_dialog.rs
  - crates/editor/src/editor_state.rs
```

## Dependencies

No external dependencies. This chunk builds on existing infrastructure:
- `ConfirmDialog` and `ConfirmDialogContext` from `dirty_tab_close_confirm` / `generic_yes_no_modal`
- `TerminalBuffer::try_wait()` and `kill()` from `terminal_emulator`
- Focus routing through `EditorFocus::ConfirmDialog`

## Risks and Open Questions

- **Shell idle detection accuracy**: The goal mentions "no process attached or the process has already exited." In practice, a shell is always running when a terminal tab is open. The `try_wait()` approach checks if the *child process* (the shell) has exited. An idle shell prompt still has a running shell process, so `try_wait()` returns `None`. This means:
  - Terminal with shell prompt (no foreground job): Shows confirmation (shell is running)
  - Terminal after `exit` command or shell crash: Closes immediately

  This matches the stated goal ("PTY is attached to a running process") but may be stricter than some terminal emulators that detect foreground process vs background shell. If this is too strict, we could extend `TerminalBuffer` with foreground process group detection, but that's out of scope for this chunk.

- **Race condition on slow exit**: Between checking `try_wait()` and showing the dialog, the process could exit. This is benign - the user confirms, we call `kill()` (which is a no-op on an already-exited process), and the tab closes.

- **PTY without process**: The `TerminalBuffer::try_wait()` method returns `None` if no PTY is attached (`self.pty.as_mut()?.try_wait()`). This means a terminal created but never spawned a shell would show no dialog. This matches the goal ("no process attached").

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.
-->
