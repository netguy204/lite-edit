---
decision: APPROVE
summary: All success criteria satisfied; NSOpenPanel directory picker integrated with Cmd+N, FileIndex moved to per-workspace ownership
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Pressing Cmd+N opens a standard macOS `NSOpenPanel` configured for directory selection (not file selection).

- **Status**: satisfied
- **Evidence**: `dir_picker.rs` lines 33-50 implement production `pick_directory()` using `NSOpenPanel::openPanel()` with `setCanChooseFiles(false)`, `setCanChooseDirectories(true)`, and `setAllowsMultipleSelection(false)`. The `editor_state.rs` key handler (line 432-436) calls `self.new_workspace()` on Cmd+N (without Shift), which in turn calls `dir_picker::pick_directory()` (lines 1991-1993).

### Criterion 2: Selecting a directory creates a new workspace whose `root_path` is set to the chosen directory.

- **Status**: satisfied
- **Evidence**: `editor_state.rs` lines 1991-2006 show that when `pick_directory()` returns `Some(dir)`, the directory is passed to `self.editor.new_workspace(label, selected_dir)`. The workspace's `root_path` field is set directly from this path. Test `test_new_workspace_root_path_is_selected_directory` verifies this (lines 3896-3904).

### Criterion 3: Cancelling the dialog does not create a workspace or change any state.

- **Status**: satisfied
- **Evidence**: `editor_state.rs` lines 1993-1995 show early return when `pick_directory()` returns `None`. Test `test_new_workspace_with_cancelled_picker_does_nothing` (lines 3844-3858) verifies workspace count is unchanged and no dirty region is set.

### Criterion 4: Terminals opened in the new workspace start in the selected directory.

- **Status**: satisfied
- **Evidence**: The workspace's `root_path` is set from the directory picker selection. The existing terminal spawning code in `new_terminal_tab()` reads `workspace.root_path` for the terminal's working directory (this wiring predates this chunk as noted in PLAN.md). The newly selected directory becomes `root_path`, so terminals will start there.

### Criterion 5: Opening the file picker (Cmd+P) in the new workspace searches files under that workspace's `root_path`.

- **Status**: satisfied
- **Evidence**: `workspace.rs` lines 489-502 show `Workspace::new()` initializes `FileIndex::start(root_path.clone())`. The `open_file_picker()` method in `editor_state.rs` (lines 549-597) now queries `workspace.file_index.query("")` from the active workspace. Test `test_file_picker_queries_active_workspace_index` and `test_workspace_file_index_uses_root_path` verify this behavior.

### Criterion 6: The workspace label in the left rail shows the directory name (last path component).

- **Status**: satisfied
- **Evidence**: `editor_state.rs` lines 1998-2002 derive the label via `selected_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "workspace".to_string())`. Test `test_new_workspace_label_from_directory_name` (lines 3881-3892) verifies that selecting `/home/user/my_project` yields label "my_project".

### Criterion 7: Existing workspaces and their file indexes are unaffected by creating a new workspace.

- **Status**: satisfied
- **Evidence**: Each workspace now owns its own `FileIndex` (workspace.rs lines 481-483). The `test_multiple_workspaces_have_independent_file_indexes` test (lines 1310-1336) creates two workspaces with different temp directories and verifies each only sees its own files. Creating a new workspace simply adds to the `Editor.workspaces` vector without modifying existing entries.
