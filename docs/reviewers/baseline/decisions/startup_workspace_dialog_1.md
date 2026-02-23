---
decision: APPROVE
summary: All success criteria satisfied - startup flow properly gates workspace creation on directory selection via dialog or CLI argument
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: When the application starts without a directory argument, the NSOpenPanel directory picker is displayed.

- **Status**: satisfied
- **Evidence**: `main.rs` lines 639-653: `resolve_startup_directory()` checks `std::env::args().nth(1)` for a CLI argument. If no valid directory is provided, it calls `dir_picker::pick_directory()` which presents the NSOpenPanel. The dialog is shown BEFORE any window/view/resources are created (lines 657-670), ensuring proper modal behavior.

### Criterion 2: The selected directory becomes the root path of the initial workspace and its `FileIndex`.

- **Status**: satisfied
- **Evidence**: `main.rs` lines 717-718: After resolving the startup directory, `EditorState::new_deferred()` creates an editor with no workspaces, then `state.add_startup_workspace(startup_dir)` creates the workspace with the user-selected path. In `editor_state.rs` lines 361-370, `add_startup_workspace()` calls `self.editor.new_workspace(label, root_path)` which creates the workspace rooted at the selected directory. `Workspace::new()` (workspace.rs:501-518) passes `root_path` to `FileIndex::start()`, ensuring the FileIndex is rooted at the correct directory.

### Criterion 3: If the user cancels the picker, the application exits gracefully.

- **Status**: satisfied
- **Evidence**: `main.rs` lines 662-670: When `resolve_startup_directory()` returns `None` (user cancelled), the code calls `app.terminate(None)` and returns early. This happens BEFORE any window is created, providing clean exit behavior.

### Criterion 4: If a directory argument is provided on the command line (e.g. `lite-edit /some/path`), the picker is skipped and that path is used directly.

- **Status**: satisfied
- **Evidence**: `main.rs` lines 641-649: `resolve_startup_directory()` first checks `std::env::args().nth(1)`. If a valid directory path is provided (validated with `path.is_dir()`), it returns `Some(path)` immediately, bypassing the directory picker. Invalid paths (non-existent or not a directory) fall through to show the picker as graceful degradation.

### Criterion 5: The `FileIndex` never starts crawling `/` or any unexpectedly broad directory at startup.

- **Status**: satisfied
- **Evidence**: The key invariant is enforced by the startup flow design:
  1. `Editor::new_deferred()` (workspace.rs:893-901) creates an editor with NO workspaces, so no FileIndex is created.
  2. The only path to create a workspace at startup is through `add_startup_workspace()` with a user-provided path.
  3. `resolve_startup_directory()` only accepts paths that pass `path.is_dir()` from CLI args, or paths explicitly selected by the user via NSOpenPanel.
  4. The old code path that used `std::env::current_dir()` (which could return `/` when launched from Finder) is bypassed - `Editor::new()` still uses it for compatibility, but startup now uses `new_deferred()` exclusively.

  Unit tests in workspace.rs (lines 1439-1475) verify the deferred initialization pattern works correctly.
