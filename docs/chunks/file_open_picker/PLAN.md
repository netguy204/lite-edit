<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds `Cmd+O` support to open files via the native macOS NSOpenPanel,
following the established patterns in the codebase:

1. **Humble object pattern** (per TESTING_PHILOSOPHY.md): Create a new
   `file_picker.rs` module that mirrors `dir_picker.rs` — a thin wrapper around
   NSOpenPanel with a thread-local mock for test isolation. The real NSOpenPanel
   code is never touched during tests.

2. **App-level shortcut handling**: Add `Cmd+O` interception in
   `EditorState::handle_key()` alongside the existing `Cmd+P`, `Cmd+S`, etc.
   shortcuts (around line 670-830 in editor_state.rs).

3. **Reuse `associate_file`**: The existing `EditorState::associate_file(path)`
   method already handles loading file contents, setting syntax highlighting,
   and resetting viewport. We call this after the picker returns a file path.

4. **Terminal tab no-op**: `associate_file` already guards against terminal tabs
   via `active_tab_is_file()`. The `Cmd+O` handler will similarly be a no-op
   when the active tab is a terminal.

5. **TDD approach**: Write failing tests first for the mock infrastructure and
   the keyboard shortcut integration, then implement to make them pass.

## Sequence

### Step 1: Create file_picker.rs module (test mock infrastructure)

Create `crates/editor/src/file_picker.rs` mirroring `dir_picker.rs` structure:

- Production code (`#[cfg(not(test))]`): `pick_file() -> Option<PathBuf>` using
  `NSOpenPanel` with `setCanChooseFiles(true)`, `setCanChooseDirectories(false)`,
  `setAllowsMultipleSelection(false)`.
- Test code (`#[cfg(test)]`): `thread_local!` with `MOCK_FILE` and a
  `mock_set_next_file(Option<PathBuf>)` function.
- Unit tests for the mock behavior: returns set value, returns None by default,
  consumes value after one call, can be reset.

Location: `crates/editor/src/file_picker.rs`

### Step 2: Register file_picker module in lib.rs

Add module declaration to `crates/editor/src/lib.rs`:

```rust
// Chunk: docs/chunks/file_open_picker - File picker for opening files via Cmd+O
mod file_picker;
```

Location: `crates/editor/src/lib.rs`

### Step 3: Write failing tests for Cmd+O behavior

Add tests in `crates/editor/src/editor_state.rs` (in the test module):

1. `test_cmd_o_opens_file_into_active_tab`: Mock `pick_file` to return a path,
   press `Cmd+O`, verify buffer contents match file contents and associated_file
   is set.

2. `test_cmd_o_cancelled_picker_leaves_tab_unchanged`: Mock `pick_file` to return
   `None`, press `Cmd+O`, verify buffer contents unchanged.

3. `test_cmd_o_no_op_on_terminal_tab`: Switch to terminal tab, press `Cmd+O`,
   verify no changes (terminal remains active, no crash).

4. `test_cmd_o_does_not_insert_character`: Press `Cmd+O`, verify 'o' is not
   inserted into the buffer (same pattern as `test_cmd_p_does_not_insert_p`).

Location: `crates/editor/src/editor_state.rs` (test module)

### Step 4: Implement handle_cmd_o method

Add a new method `handle_cmd_o(&mut self)` in `EditorState`:

```rust
/// Handles Cmd+O to open a file via the native macOS file picker.
/// Chunk: docs/chunks/file_open_picker - Open file via system file picker
fn handle_cmd_o(&mut self) {
    // No-op for terminal tabs (associate_file also guards, but early return is cleaner)
    if !self.active_tab_is_file() {
        return;
    }

    if let Some(path) = file_picker::pick_file() {
        self.associate_file(path);
    }
}
```

Location: `crates/editor/src/editor_state.rs` (near `handle_cmd_p`, around line 853)

### Step 5: Add Cmd+O interception in handle_key

In `EditorState::handle_key()`, add the `Cmd+O` case within the
`if event.modifiers.command && !event.modifiers.control` block:

```rust
// Cmd+O (without Ctrl) opens system file picker
// Chunk: docs/chunks/file_open_picker
if let Key::Char('o') = event.key {
    self.handle_cmd_o();
    return;
}
```

Location: `crates/editor/src/editor_state.rs` (inside handle_key, after Cmd+N block)

### Step 6: Add import for file_picker in editor_state.rs

Add `use crate::file_picker;` to the imports at the top of `editor_state.rs`.

Location: `crates/editor/src/editor_state.rs` (imports section)

### Step 7: Update welcome screen hotkey documentation

Add `("Cmd+O", "Open file from disk")` to the HOTKEYS constant in the "File"
category.

Location: `crates/editor/src/welcome_screen.rs` (HOTKEYS constant, line ~83)

### Step 8: Add backreference to main.rs

Add chunk backreference comment to `main.rs`:

```rust
// Chunk: docs/chunks/file_open_picker - System file picker (Cmd+O) integration
```

Location: `crates/editor/src/main.rs` (chunk comment block at top)

### Step 9: Run tests and verify

Run `cargo test` in the editor crate to verify all tests pass.

## Dependencies

No external dependencies. This chunk relies on:
- Existing `associate_file` infrastructure
- `objc2_app_kit::NSOpenPanel` (already in dependencies for `dir_picker`)
- Test patterns established by `dir_picker.rs`

## Risks and Open Questions

- **NSOpenPanel modal blocks the main thread**: This is the same behavior as
  `dir_picker` and is expected for macOS file/directory pickers. The main event
  loop resumes after the user confirms or cancels.

- **MainThreadMarker safety**: The `pick_file()` call requires being on the main
  thread. This is guaranteed because `handle_key` is called from the main event
  loop. The code asserts this with `MainThreadMarker::new().expect(...)`.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->