<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk introduces directory picking when creating a new workspace (Cmd+N) and makes the `FileIndex` per-workspace. The approach is:

1. **Add NSOpenPanel wrapper module**: Create a thin wrapper around `NSOpenPanel` for directory selection, following the "humble object" pattern used by `clipboard.rs`. The wrapper exposes a single function that shows the dialog and returns `Option<PathBuf>`.

2. **Change Cmd+N flow**: Instead of immediately creating a workspace with `std::env::current_dir()`, the new flow is:
   - Show `NSOpenPanel` configured for directory selection
   - If user selects a directory: create workspace with that path as `root_path`, initialize a `FileIndex` for that workspace
   - If user cancels: no-op (no workspace created)

3. **Move FileIndex into Workspace**: Currently `EditorState` owns a single `FileIndex`. This chunk moves the `FileIndex` into `Workspace` so each workspace has its own index rooted at its `root_path`. This ensures the file picker (Cmd+P) searches the correct directory for each workspace.

4. **Derive workspace label from directory**: The workspace label will be the last path component of the selected directory (e.g., `/Users/foo/projects/bar` → label "bar").

The implementation builds on:
- `crates/editor/src/clipboard.rs` - Pattern for macOS Cocoa bindings with test isolation
- `crates/editor/src/workspace.rs` - Workspace model (`Workspace`, `Editor`)
- `crates/editor/src/editor_state.rs` - Keyboard handling, `new_workspace()` method
- `crates/editor/src/file_index.rs` - FileIndex for fuzzy file matching

## Sequence

### Step 1: Create dir_picker module

Create a new module `crates/editor/src/dir_picker.rs` that wraps `NSOpenPanel` for directory selection.

**Production implementation:**
- Use `objc2_app_kit::NSOpenPanel` to create a panel
- Configure: `setCanChooseFiles(false)`, `setCanChooseDirectories(true)`, `setAllowsMultipleSelection(false)`
- Call `runModal()` and check result (NSModalResponseOK)
- Return `Some(PathBuf)` with the selected directory, or `None` if cancelled

**Test implementation:**
- Use a `thread_local!` mock that allows tests to inject a response
- Provide `mock_set_next_directory(Option<PathBuf>)` for test setup
- Never touch real NSOpenPanel in tests

```rust
// Public API:
pub fn pick_directory() -> Option<PathBuf>;

// Test support (cfg(test) only):
pub fn mock_set_next_directory(dir: Option<PathBuf>);
```

Location: `crates/editor/src/dir_picker.rs`

### Step 2: Move FileIndex into Workspace

Refactor the ownership of `FileIndex`:

1. **Remove from EditorState:**
   - Remove `file_index: Option<FileIndex>` field from `EditorState`
   - Remove `last_cache_version: u64` field from `EditorState`

2. **Add to Workspace:**
   - Add `file_index: FileIndex` field to `Workspace` struct
   - Add `last_cache_version: u64` field to `Workspace` struct (for tracking cache changes during file picker)

3. **Initialize FileIndex in workspace creation:**
   - `Workspace::new()` and `Workspace::with_empty_tab()` accept `root_path` already
   - Add `FileIndex::start(root_path.clone())` initialization in these constructors

4. **Update EditorState methods:**
   - `open_file_picker()`: access `file_index` from `self.editor.active_workspace()` instead of `self.file_index`
   - `handle_selector_confirm()`: access via active workspace
   - `try_refresh_picker_if_indexing()`: access via active workspace
   - `poll_agents()`: no change needed (doesn't touch file_index)

Location: `crates/editor/src/workspace.rs`, `crates/editor/src/editor_state.rs`

### Step 3: Update EditorState::new_workspace to show directory picker

Modify `EditorState::new_workspace()` to:

1. Call `dir_picker::pick_directory()`
2. If `Some(selected_dir)`:
   - Extract directory name as label: `selected_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "workspace".to_string())`
   - Call `self.editor.new_workspace(label, selected_dir)` (this already initializes FileIndex per Step 2)
   - Mark `dirty_region.merge(DirtyRegion::FullViewport)`
3. If `None`: return early (no workspace created, no dirty region)

The workspace's `root_path` is now user-selected, and terminals opened in this workspace will use that path (existing wiring in `new_terminal_tab()` already reads `workspace.root_path`).

Location: `crates/editor/src/editor_state.rs`

### Step 4: Update file picker to use workspace's file index

Ensure `open_file_picker()` and related selector methods access the file index from the active workspace:

```rust
fn open_file_picker(&mut self) {
    let workspace = match self.editor.active_workspace_mut() {
        Some(ws) => ws,
        None => return,
    };

    let results = workspace.file_index.query("");
    // ... rest of setup ...
    workspace.last_cache_version = workspace.file_index.cache_version();
}
```

Similarly update:
- `handle_selector_key()` - re-query from workspace
- `handle_selector_confirm()` - record_selection on workspace's file_index
- `try_refresh_picker_if_indexing()` - check workspace's file_index version

Location: `crates/editor/src/editor_state.rs`

### Step 5: Add unit tests

**Test dir_picker module:**
- `test_mock_pick_directory_returns_set_value()` - verify mock returns injected path
- `test_mock_pick_directory_returns_none_by_default()` - verify mock returns None when not set
- `test_mock_pick_directory_consumes_value()` - verify mock is consumed after one call

**Test workspace FileIndex ownership:**
- `test_workspace_has_file_index()` - verify workspace initializes with FileIndex
- `test_workspace_file_index_uses_root_path()` - verify FileIndex is rooted at workspace's root_path
- `test_multiple_workspaces_have_independent_file_indexes()` - verify workspaces don't share indexes

**Test new_workspace flow (EditorState):**
- `test_new_workspace_with_cancelled_picker_does_nothing()` - mock returns None, workspace count unchanged
- `test_new_workspace_with_selection_creates_workspace()` - mock returns path, new workspace created with that root_path
- `test_new_workspace_label_from_directory_name()` - verify label is derived from selected directory

**Test file picker uses workspace index:**
- `test_file_picker_queries_active_workspace_index()` - verify picker uses current workspace's FileIndex

Location: `crates/editor/src/dir_picker.rs` (mod tests), `crates/editor/src/workspace.rs` (mod tests), `crates/editor/src/editor_state.rs` (mod tests)

### Step 6: Wire up module and update lib.rs

1. Add `mod dir_picker;` to `crates/editor/src/lib.rs`
2. Verify the build compiles and all tests pass
3. Manual verification:
   - Run the editor
   - Press Cmd+N, verify directory picker appears
   - Select a directory, verify new workspace appears in left rail with correct label
   - Press Cmd+N again, cancel the picker, verify no new workspace created
   - Open file picker (Cmd+P) in new workspace, verify it searches the selected directory
   - Open terminal (Cmd+Shift+T) in new workspace, verify it starts in the selected directory

Location: `crates/editor/src/lib.rs`

## Dependencies

- `objc2-app-kit` crate already in dependencies (provides `NSOpenPanel`)
- `workspace_model` chunk (ACTIVE) - provides the Workspace/Editor model
- `fuzzy_file_matcher` chunk (implicitly ACTIVE) - provides FileIndex

No new external dependencies required.

## Risks and Open Questions

1. **NSOpenPanel runModal() blocks the main thread**: This is acceptable for a file dialog - the user expects modal behavior. However, if the render loop is tied to the NSRunLoop, we need to verify the UI doesn't freeze visibly while the dialog is open.

2. **FileIndex memory footprint with multiple workspaces**: Each workspace will now have its own FileIndex with its own cache. For very large directories, this could increase memory usage. This is acceptable for v1 - most users won't have many workspaces simultaneously.

3. **Initial workspace at startup**: `Editor::new()` currently creates an initial workspace with `std::env::current_dir()`. This chunk doesn't change startup behavior - only Cmd+N shows the picker. The initial workspace will still use the process's cwd. This is intentional to preserve fast startup without modal dialogs.

4. **Test isolation for NSOpenPanel**: Following the clipboard pattern, tests will use a mock. However, there's no automated integration test that verifies the real NSOpenPanel works correctly. Manual testing is required.

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

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->