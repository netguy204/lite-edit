# Implementation Plan

## Approach

When lite-edit is launched from macOS GUI (Spotlight, Dock, Finder), it inherits a minimal login environment that lacks PATH modifications set up by the user's shell profile chain (`~/.zshrc`, `~/.zprofile`, etc.). This causes tools like `pyenv`, `nvm`, and `rbenv` to not be found in terminals spawned within the app.

The fix is to spawn shells as **login shells** so they source the user's full profile chain. On Unix systems, a login shell is invoked by setting `argv[0]` to `-{shell_basename}` (e.g., `-zsh` instead of `zsh`). This is the standard mechanism used by terminal emulators like Terminal.app, iTerm2, and wezterm.

The `portable-pty` crate (v0.8.1) already supports this pattern through its `CommandBuilder::new_default_prog()` API, which:
1. Uses `get_shell()` to determine the user's shell from the passwd database
2. Sets `argv[0]` to `-{basename}` when spawning

We will modify `PtyHandle::spawn` and `spawn_with_wakeup` to accept a `login_shell: bool` parameter that controls whether to spawn as a login shell. The terminal buffer layer will then pass `true` for shell spawning while preserving the ability to spawn non-login processes for other use cases.

**Testing approach**: Per TESTING_PHILOSOPHY.md, the shell spawning itself is platform-dependent and cannot be easily unit tested. However, we can verify:
1. The login shell flag propagates correctly through the API
2. Integration tests can spawn a login shell and verify environment variables are present

## Sequence

### Step 1: Add login_shell parameter to PtyHandle::spawn

Modify `PtyHandle::spawn()` in `crates/terminal/src/pty.rs` to accept a `login_shell: bool` parameter.

When `login_shell` is true:
- Use `CommandBuilder::new_default_prog()` which automatically:
  - Reads the user's shell from the passwd database (via `getpwuid`)
  - Sets `argv[0]` to `-{shell_basename}` for login shell behavior
  - Respects the `cwd` setting

When `login_shell` is false:
- Use the existing `CommandBuilder::new(cmd)` approach for explicit commands

Location: `crates/terminal/src/pty.rs`

### Step 2: Add login_shell parameter to PtyHandle::spawn_with_wakeup

Apply the same change to `PtyHandle::spawn_with_wakeup()` for consistency.

The implementation mirrors Step 1 but includes the wakeup signaling on PTY output.

Location: `crates/terminal/src/pty.rs`

### Step 3: Update TerminalBuffer::spawn_shell to use login shell mode

Modify `TerminalBuffer::spawn_shell()` and `spawn_shell_with_wakeup()` in `crates/terminal/src/terminal_buffer.rs`:

- Change the signature to remove the explicit `shell` parameter
- Call `PtyHandle::spawn` with `login_shell: true`
- The shell will be determined automatically by `portable-pty`'s `get_shell()` which reads from `/etc/passwd`

This simplifies the API and ensures shells are always spawned as login shells with the correct environment.

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 4: Update TerminalBuffer::spawn_command for non-login spawning

Ensure `spawn_command()` and `spawn_command_with_wakeup()` continue to work for explicit commands (non-login shells). These should pass `login_shell: false` to preserve current behavior for running specific commands.

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 5: Update EditorState::new_terminal_tab to use simplified API

Update `EditorState::new_terminal_tab()` in `crates/editor/src/editor_state.rs`:

- Remove the manual `$SHELL` lookup (no longer needed)
- Call the simplified `spawn_shell_with_wakeup()` or `spawn_shell()` without shell path parameter

This is now cleaner because `portable-pty` handles shell detection internally via the passwd database, which is more reliable than `$SHELL` environment variable (which may not be set in GUI-launched apps).

Location: `crates/editor/src/editor_state.rs`

### Step 6: Add integration test for login shell environment

Create an integration test that:
1. Spawns a login shell via `PtyHandle::spawn` with `login_shell: true`
2. Runs `echo $0` to verify the shell reports as a login shell (should show `-zsh` or similar)
3. Optionally checks that common profile indicators are present

This test verifies the end-to-end behavior without mocking platform-specific details.

Location: `crates/terminal/src/pty.rs` (in the `#[cfg(test)]` module)

### Step 7: Update existing tests

Review and update existing tests in `pty.rs` that call `PtyHandle::spawn()` to include the new `login_shell` parameter:
- `test_spawn_echo` → use `login_shell: false` (explicit command)
- `test_spawn_exit_code` → use `login_shell: false` (explicit command)

Location: `crates/terminal/src/pty.rs`

## Dependencies

- **terminal_emulator** chunk: Provides the base `PtyHandle` and `TerminalBuffer` types
- **terminal_tab_spawn** chunk: Provides `EditorState::new_terminal_tab()` which we modify

Both are already ACTIVE, so this chunk can proceed.

## Risks and Open Questions

1. **Shell detection on macOS**: The `portable-pty` crate uses `getpwuid()` to get the user's shell from the passwd database. This should work correctly on macOS, but if a user has modified their shell outside of the standard mechanism, it might not detect the correct shell. This is standard behavior for terminal emulators.

2. **Profile sourcing order**: Login shells source different files than interactive non-login shells:
   - Login: `/etc/zprofile` → `~/.zprofile` → `/etc/zshrc` → `~/.zshrc` → `/etc/zlogin` → `~/.zlogin`
   - Interactive non-login: `/etc/zshrc` → `~/.zshrc`

   By spawning as a login shell, we ensure the full chain is sourced, matching standalone terminal emulator behavior.

3. **Performance**: Login shells may have slower startup due to sourcing more files. This is acceptable as it matches user expectations from their regular terminal. Users with slow profiles will see the same behavior in lite-edit as in Terminal.app.

4. **Backward compatibility**: The API change to `spawn_shell()` (removing the shell parameter) is a breaking change to the terminal crate's public API. However, since this is an internal crate, this is acceptable.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->