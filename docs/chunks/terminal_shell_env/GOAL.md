---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/pty.rs
- crates/terminal/src/terminal_buffer.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/terminal/src/pty.rs#PtyHandle::spawn
    implements: "Login shell spawning via CommandBuilder::new_default_prog() when login_shell=true"
  - ref: crates/terminal/src/pty.rs#PtyHandle::spawn_with_wakeup
    implements: "Login shell spawning with run-loop wakeup support"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::spawn_shell
    implements: "Simplified shell spawn API that always uses login shell mode"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::spawn_shell_with_wakeup
    implements: "Simplified shell spawn with wakeup, always login shell mode"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_terminal_tab
    implements: "Terminal tab creation using simplified spawn_shell API"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- scroll_wheel_speed
---

# Chunk Goal

## Minor Goal

Ensure the integrated terminal spawns shells with the user's full interactive environment, including PATH entries from tools like pyenv, rbenv, nvm, and other shell-initialized configuration.

When lite-edit is launched from the macOS GUI (Spotlight, Dock, Finder), it inherits a minimal login environment that lacks PATH modifications set up by the user's shell profile (`~/.zshrc`, `~/.zprofile`, `~/.bash_profile`, etc.). The spawned terminal shells inherit this limited environment, causing tools like `pyenv` to not be found.

The fix should ensure that shells spawned via `PtyHandle::spawn` / `spawn_with_wakeup` in `crates/terminal/src/pty.rs` get an environment equivalent to what the user sees when they open a standalone terminal emulator. The standard approach is to spawn the shell as a login shell (e.g., passing `-l` flag, or setting `argv[0]` to `-zsh`) so that it sources the user's full profile chain.

## Success Criteria

- A terminal opened via Cmd+Shift+T has the user's full PATH (including pyenv, nvm, rbenv shims) regardless of how lite-edit was launched
- Shell is spawned as a login shell so the full profile chain (`/etc/zprofile`, `~/.zprofile`, `~/.zshrc`) is sourced
- Existing terminal behavior (cwd, TERM, COLORTERM) is preserved
- No regression in terminal spawn time