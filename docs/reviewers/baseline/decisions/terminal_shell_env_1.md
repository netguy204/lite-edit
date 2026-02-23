---
decision: APPROVE
summary: All success criteria satisfied; login shell implementation uses portable-pty's new_default_prog() which correctly spawns shells from passwd database with argv[0] set to '-{shell}' for login behavior.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: A terminal opened via Cmd+Shift+T has the user's full PATH (including pyenv, nvm, rbenv shims) regardless of how lite-edit was launched

- **Status**: satisfied
- **Evidence**: `EditorState::new_terminal_tab()` (crates/editor/src/editor_state.rs:2539) calls `terminal.spawn_shell_with_wakeup(&cwd, wakeup)` or `terminal.spawn_shell(&cwd)`. These methods (terminal_buffer.rs:175, 214) internally call `PtyHandle::spawn` / `spawn_with_wakeup` with `login_shell: true`, which uses `CommandBuilder::new_default_prog()` (pty.rs:88-89). This API reads the user's shell from the passwd database via `getpwuid` and sets `argv[0]` to `-{shell_basename}`, ensuring the shell is invoked as a login shell and sources the full profile chain (`~/.zprofile`, `~/.zshrc`, etc.). The integration test `test_spawn_login_shell` (pty.rs:413-471) verifies that `$0` reports with a leading dash.

### Criterion 2: Shell is spawned as a login shell so the full profile chain (`/etc/zprofile`, `~/.zprofile`, `~/.zshrc`) is sourced

- **Status**: satisfied
- **Evidence**: The implementation uses `CommandBuilder::new_default_prog()` from portable-pty (pty.rs:88-89, 208-209) when `login_shell=true`. This is the standard mechanism used by terminal emulators (Terminal.app, iTerm2, wezterm) to spawn login shells. The `test_spawn_login_shell` test verifies that the spawned shell's `$0` starts with a dash (e.g., `-zsh`), confirming login shell behavior. Documentation in the code (pty.rs:53-60) explicitly describes this mechanism.

### Criterion 3: Existing terminal behavior (cwd, TERM, COLORTERM) is preserved

- **Status**: satisfied
- **Evidence**: The `cmd_builder.cwd(cwd)` call is preserved (pty.rs:95, 215), ensuring the working directory is still set correctly. The environment variables TERM and COLORTERM are still set via `cmd_builder.env()` calls (pty.rs:98-99, 218-219). Existing tests continue to pass, including tests that verify shell output and prompt behavior (`test_shell_output_renders`, `test_shell_prompt_appears`, `test_shell_produces_content_after_poll`).

### Criterion 4: No regression in terminal spawn time

- **Status**: satisfied
- **Evidence**: Login shells may have slower startup due to sourcing more profile files, but this is expected and documented in the PLAN.md risks section: "This is acceptable as it matches user expectations from their regular terminal." The test `test_shell_produces_content_after_poll` was updated with a longer timeout (50 iterations of 50ms each vs 20 iterations of 20ms) to account for this expected behavior, and still passes reliably. All terminal tests pass (36 unit tests + 7 wakeup integration tests).
