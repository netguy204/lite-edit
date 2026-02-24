<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Modify `EditorState::new_workspace()` to conditionally open a terminal tab instead
of an empty file tab when the workspace count is already ≥1 before the new workspace
is created. The startup workspace (via `add_startup_workspace()`) remains unchanged.

The key insight from the GOAL.md:
- `add_startup_workspace()` creates the **first** workspace of the session — keep it
  unchanged with an empty file tab / welcome screen for onboarding.
- `new_workspace()` is user-triggered via the directory picker and is always the
  **second workspace or later** — change this to open a terminal tab instead.

The existing `new_terminal_tab()` method (from the `terminal_tab_spawn` chunk) handles
all the terminal creation complexity: dimension calculation, shell spawning, PTY wakeup,
viewport initialization, and tab labeling. We reuse this directly.

**Implementation strategy:**

1. Check `self.editor.workspace_count()` **before** calling `self.editor.new_workspace()`
   to determine if this is the startup workspace or a subsequent one.
2. Call the inner `Editor::new_workspace()` without an initial tab (use `new_workspace_without_tab()`)
   when we want to add a terminal instead of a file tab.
3. After workspace creation, call `new_terminal_tab()` to spawn the terminal.

**Alternative considered:** Modifying `Editor::new_workspace()` to take a parameter
controlling initial tab type. Rejected because:
- It couples workspace creation logic to tab type knowledge
- `new_terminal_tab()` does significant work (dimension calculation, PTY spawning)
  that doesn't belong in workspace.rs
- The simpler approach of calling `new_terminal_tab()` after workspace creation keeps
  responsibilities separated

**Test strategy (per TESTING_PHILOSOPHY.md):**

Following TDD, we write failing tests first that verify:
1. Startup workspace still opens with empty file tab (no regression)
2. Second workspace (via directory picker) opens with terminal tab
3. Terminal tab label follows existing convention ("Terminal")
4. Terminal uses workspace's root_path as working directory

## Sequence

### Step 1: Add `new_workspace_without_tab()` method to `Editor`

Add an internal method to create a workspace without an initial tab. The existing
`new_workspace_internal()` already supports this via its `with_tab` parameter, so
we expose a public method that uses `with_tab: false`.

```rust
/// Creates a new workspace without any initial tabs and switches to it.
///
/// Returns the ID of the new workspace.
pub fn new_workspace_without_tab(&mut self, label: String, root_path: PathBuf) -> WorkspaceId {
    let ws_id = self.new_workspace_internal(label, root_path, false);
    self.active_workspace = self.workspaces.len() - 1;
    ws_id
}
```

Location: `crates/editor/src/workspace.rs`, impl `Editor` block (near line 1056)

### Step 2: Write failing tests for the new behavior

Add tests that verify the goal's success criteria:

**Test 1: `test_startup_workspace_has_empty_file_tab`**
- Create an `EditorState` via `new_deferred()` (simulating startup)
- Call `add_startup_workspace()`
- Assert the workspace has exactly 1 tab
- Assert the tab's kind is `TabKind::File`
- Assert the tab's buffer is empty (welcome screen state)

**Test 2: `test_second_workspace_has_terminal_tab`**
- Create an `EditorState` with one workspace (via `add_startup_workspace()`)
- Mock the directory picker to return a path
- Call `new_workspace()`
- Assert the new workspace has exactly 1 tab
- Assert the tab's kind is `TabKind::Terminal`
- Assert the tab's label is "Terminal"

**Test 3: `test_second_workspace_terminal_uses_workspace_root_path`**
- Create a second workspace with a specific root_path
- Verify the terminal's working directory is the workspace's root_path
- (This is implicitly tested via `new_terminal_tab()` using workspace's `root_path`)

Location: `crates/editor/src/editor_state.rs` (test module, near existing workspace tests)

### Step 3: Modify `EditorState::new_workspace()` to spawn terminal for subsequent workspaces

Update `new_workspace()` to check workspace count and conditionally create a terminal:

```rust
// Chunk: docs/chunks/workspace_initial_terminal - Terminal tab for subsequent workspaces
pub fn new_workspace(&mut self) {
    // Show directory picker dialog
    let selected_dir = match dir_picker::pick_directory() {
        Some(dir) => dir,
        None => return, // User cancelled, do nothing
    };

    // Derive workspace label from directory name
    let label = selected_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string());

    // Check if this is a subsequent workspace (not the startup workspace)
    let is_subsequent = self.editor.workspace_count() >= 1;

    if is_subsequent {
        // Subsequent workspaces get a terminal tab instead of empty file tab
        self.editor.new_workspace_without_tab(label, selected_dir);
        self.new_terminal_tab();
    } else {
        // First workspace gets empty file tab (for welcome screen)
        self.editor.new_workspace(label, selected_dir);
    }

    self.dirty_region.merge(DirtyRegion::FullViewport);
}
```

Location: `crates/editor/src/editor_state.rs`, `new_workspace()` method (line ~2625)

### Step 4: Verify existing tests still pass

Run the existing test suite to ensure no regressions:

```bash
cargo test --package lite-edit-editor
```

Key tests to verify:
- `test_cmd_n_creates_new_workspace` - should still work
- `test_new_workspace_with_cancelled_picker_does_nothing` - unchanged behavior
- `test_new_workspace_with_selection_creates_workspace` - will now have terminal tab
- `test_new_workspace_label_from_directory_name` - unchanged
- `test_new_workspace_root_path_is_selected_directory` - unchanged
- `test_startup_workspace_has_empty_file_tab` - new test, verifies startup unchanged

Note: `test_new_workspace_with_selection_creates_workspace` may need adjustment since
it currently expects the workspace to have an empty file tab. Update it to verify
the terminal tab instead.

### Step 5: Update existing tests that assume file tabs in new workspaces

Update `test_new_workspace_with_selection_creates_workspace` to verify the new
terminal tab behavior:

```rust
#[test]
fn test_new_workspace_with_selection_creates_workspace() {
    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_size(160.0);
    state.update_viewport_dimensions(800.0, 600.0); // Need dimensions for terminal sizing

    assert_eq!(state.editor.workspace_count(), 1);

    // Mock returns a directory
    dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/project")));
    state.new_workspace();

    // Should now have 2 workspaces
    assert_eq!(state.editor.workspace_count(), 2);
    // Should be switched to the new workspace
    assert_eq!(state.editor.active_workspace, 1);
    // Should be dirty
    assert!(state.is_dirty());

    // Chunk: docs/chunks/workspace_initial_terminal - Second workspace gets terminal tab
    // The new workspace should have a terminal tab, not an empty file tab
    let workspace = state.editor.active_workspace().unwrap();
    assert_eq!(workspace.tab_count(), 1);
    let tab = workspace.active_tab().unwrap();
    assert_eq!(tab.kind, TabKind::Terminal);
    assert_eq!(tab.label, "Terminal");
}
```

Location: `crates/editor/src/editor_state.rs` (test module)

### Step 6: Add backreference comment to `new_workspace()`

Add a chunk backreference to the modified method for traceability:

```rust
// Chunk: docs/chunks/workspace_initial_terminal - Terminal tab for subsequent workspaces
```

Location: `crates/editor/src/editor_state.rs`, before `new_workspace()` method

---

**BACKREFERENCE COMMENTS**

Add backreference comments to:
- `EditorState::new_workspace()`: `// Chunk: docs/chunks/workspace_initial_terminal`
- `Editor::new_workspace_without_tab()`: `// Chunk: docs/chunks/workspace_initial_terminal`

## Dependencies

- **terminal_tab_spawn** (ACTIVE): Provides `EditorState::new_terminal_tab()` which
  handles all terminal creation complexity. This chunk reuses that method.
- **workspace_dir_picker** (ACTIVE): Provides `EditorState::new_workspace()` and the
  directory picker integration. This chunk modifies that method.
- **startup_workspace_dialog** (ACTIVE): Provides `add_startup_workspace()` which
  remains unchanged — this chunk explicitly preserves its behavior.

All dependencies are complete (status: ACTIVE).

## Risks and Open Questions

**Low risk:**

1. **Terminal dimension edge case**: If `new_terminal_tab()` is called before viewport
   dimensions are set (unlikely but possible), it will guard against zero dimensions
   and simply not create a terminal. This is acceptable — the user can still create
   a terminal manually via Cmd+Shift+T.

2. **Shell spawn failure**: If the shell fails to spawn, `new_terminal_tab()` logs
   the error but still creates the tab. The user sees a terminal tab that shows
   an error, which is acceptable UX.

**Verification needed:**

3. **Test environment**: The tests mock `dir_picker::pick_directory()` which is good.
   However, `new_terminal_tab()` spawns a real shell process in tests. Verify that
   existing terminal tests handle this correctly (they do — see `terminal_tab_spawn`
   tests). No additional mocking needed.

## Deviations

*To be populated during implementation.*
