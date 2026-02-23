<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current startup flow in `main.rs` creates an `EditorState::new()` with an empty buffer, which internally creates an `Editor::new()`. `Editor::new()` unconditionally calls `std::env::current_dir()` to set the initial workspace's `root_path`. When launched from Finder or Spotlight, `current_dir()` resolves to `/`, causing the `FileIndex` to crawl the entire filesystem.

**Solution**: Introduce an `Editor::new_deferred()` constructor that creates an editor with no workspaces initially. The main entry point (`setup_window` in `main.rs`) will show the NSOpenPanel directory picker *before* creating the initial workspace. Based on the result:

1. **User selects a directory**: Create a workspace rooted at the selected directory.
2. **User cancels**: The application exits gracefully (no workspace, no window, no FileIndex crawling).

This follows the **humble object** pattern — the macOS-specific dialog call lives at the platform boundary (`main.rs`), while the workspace logic remains testable in isolation.

**Command-line argument handling**: For terminal usage, we check `std::env::args()` for a directory argument. If provided, we skip the dialog and use that path directly. This enables `lite-edit /some/path` to open directly.

## Sequence

### Step 1: Add `Editor::new_deferred()` constructor

Create a new constructor in `workspace.rs` that initializes the `Editor` struct without creating any initial workspace. This allows the startup flow to defer workspace creation until after the directory picker runs.

**Location**: `crates/editor/src/workspace.rs`

**Changes**:
- Add `Editor::new_deferred(line_height: f32) -> Self` that creates an empty workspaces vec
- Existing `Editor::new()` can be retained for compatibility (creates workspace with `current_dir()` as before)

**Tests**:
- `test_editor_new_deferred_has_no_workspaces` — verifies empty workspace list
- `test_editor_new_deferred_can_add_workspace` — verifies `new_workspace()` works after deferred init

### Step 2: Add `EditorState::empty()` constructor or modify `new()`

The public `EditorState::new()` currently takes a buffer and creates the editor. We need a way to create an `EditorState` where no workspace exists yet. There are two options:

**Option A** (preferred): Add `EditorState::new_deferred(font_metrics: FontMetrics) -> Self` that uses `Editor::new_deferred()` internally. This keeps the API explicit.

**Option B**: Modify the test helper `EditorState::empty()` if it already exists and make it public.

After reviewing the code, `EditorState::empty()` exists as a test helper. We'll add a new `EditorState::new_deferred()` for production use that's similar but appropriate for the startup flow.

**Location**: `crates/editor/src/editor_state.rs`

**Changes**:
- Add `EditorState::new_deferred(font_metrics: FontMetrics) -> Self`
- This creates an EditorState with `Editor::new_deferred()`

**Tests**:
- Tests will verify that the deferred state has no active workspace initially

### Step 3: Implement startup directory resolution in `main.rs`

Modify `setup_window()` in `main.rs` to:

1. Parse command-line arguments for a directory path
2. If argument provided and valid directory: use it directly
3. If no argument: show `pick_directory()` dialog
4. If user selects directory: create workspace with that root
5. If user cancels (and no CLI arg): terminate the application gracefully

**Location**: `crates/editor/src/main.rs`

**Changes**:
- Add a helper function `resolve_startup_directory() -> Option<PathBuf>` that:
  - Checks `std::env::args().nth(1)` for a directory argument
  - Returns `Some(path)` if valid directory argument provided
  - Calls `pick_directory()` and returns the result if no argument
- Modify `setup_window()` to:
  - Call `resolve_startup_directory()`
  - If `None`, call `app.terminate(None)` to exit
  - If `Some(path)`, create the EditorState with that workspace root

**No tests**: This is platform boundary code (NSOpenPanel, NSApplication termination). Per testing philosophy, this is "humble object" code that is not unit-tested. We verify behavior manually.

### Step 4: Create the initial workspace with selected directory

After getting the directory from Step 3, `setup_window()` creates the workspace:

1. Create `EditorState::new_deferred(font_metrics)`
2. Call `editor.new_workspace(label, path)` where label is derived from directory name
3. Add an empty tab to the workspace (to match current startup behavior with welcome screen)

**Location**: `crates/editor/src/main.rs`

**Changes**:
- After directory resolution, create workspace with the selected path
- The workspace label is derived from the directory's last path component (same pattern as `new_workspace()` in `editor_state.rs`)

### Step 5: Handle graceful exit on cancel

If the user cancels the directory picker and no CLI argument was provided, terminate the application before creating any window/view/resources.

**Location**: `crates/editor/src/main.rs`

**Changes**:
- Move directory resolution to happen *before* creating the Metal view and renderer
- If resolution returns `None`, call `NSApplication::terminate(None)` and return early
- This prevents any window from appearing if the user immediately cancels

### Step 6: Unit tests for Editor and EditorState deferred constructors

Add tests in the respective modules to verify the deferred initialization works correctly.

**Location**: `crates/editor/src/workspace.rs` (test module)

**Tests**:
```rust
#[test]
fn test_editor_new_deferred_has_no_workspaces() {
    let editor = Editor::new_deferred(16.0);
    assert_eq!(editor.workspace_count(), 0);
    assert!(editor.active_workspace().is_none());
}

#[test]
fn test_editor_new_deferred_can_add_workspace() {
    let mut editor = Editor::new_deferred(16.0);
    editor.new_workspace("test".to_string(), PathBuf::from("/test"));
    assert_eq!(editor.workspace_count(), 1);
    assert!(editor.active_workspace().is_some());
}
```

**Location**: `crates/editor/src/editor_state.rs` (test module)

**Tests**:
```rust
#[test]
fn test_editor_state_new_deferred_has_no_workspace() {
    let state = EditorState::new_deferred(test_font_metrics());
    assert_eq!(state.editor.workspace_count(), 0);
}

#[test]
fn test_editor_state_new_deferred_can_create_workspace() {
    let mut state = EditorState::new_deferred(test_font_metrics());
    dir_picker::mock_set_next_directory(Some(PathBuf::from("/test")));
    state.new_workspace();
    assert_eq!(state.editor.workspace_count(), 1);
}
```

### Step 7: Verify FileIndex never crawls root

The core invariant we're protecting: the `FileIndex` should never be initialized with `/` or any unexpectedly broad directory.

**Verification approach**:
1. The `Workspace::new()` constructor receives `root_path` and passes it to `FileIndex::start()`
2. With our changes, `root_path` only comes from:
   - User selection via directory picker
   - Explicit CLI argument
3. Neither path can produce `/` unless the user explicitly chooses it

**No additional code changes needed** — the invariant is enforced by the startup flow design.

## Dependencies

- **workspace_dir_picker chunk**: Already complete (ACTIVE). Provides `dir_picker::pick_directory()` and the per-workspace FileIndex infrastructure.
- **tiling_workspace_integration chunk**: Already complete (ACTIVE). Provides the `Workspace` and `Editor` models we're extending.

## Risks and Open Questions

1. **Timing of NSOpenPanel**: The panel must run on the main thread. In `setup_window()`, we're already on the main thread (called from `applicationDidFinishLaunching`), so this is safe. However, we need to ensure we don't create any views/windows before the dialog runs, or the dialog might appear behind them.

2. **Empty state handling**: After creating a deferred editor with no workspaces, callers of `active_workspace()` will get `None`. The code must handle this gracefully. Reviewing the codebase, most access goes through `active_workspace().unwrap()` or `?` operators. We need to ensure no panics occur in the brief period between `EditorState::new_deferred()` and `new_workspace()`.

3. **Application exit timing**: Calling `NSApplication::terminate()` from within `applicationDidFinishLaunching` might have unusual behavior. Testing will confirm this works correctly.

4. **CLI argument validation**: If a path is provided but doesn't exist or isn't a directory, we could either:
   - Show the picker as fallback
   - Show an error and exit
   - We'll choose: show the picker as fallback (graceful degradation)

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
