---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/dir_picker.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/lib.rs
code_references:
  - ref: crates/editor/src/dir_picker.rs
    implements: "NSOpenPanel wrapper for directory selection with test mock support"
  - ref: crates/editor/src/dir_picker.rs#pick_directory
    implements: "Opens macOS directory picker dialog, returns selected path or None on cancel"
  - ref: crates/editor/src/workspace.rs#Workspace
    implements: "Per-workspace FileIndex field for fuzzy file matching"
  - ref: crates/editor/src/workspace.rs#Workspace::new
    implements: "Initializes FileIndex for new workspace with root_path"
  - ref: crates/editor/src/workspace.rs#Workspace::with_empty_tab
    implements: "Initializes FileIndex for workspace with empty tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_workspace
    implements: "Shows directory picker on Cmd+N and creates workspace with selected path"
  - ref: crates/editor/src/editor_state.rs#EditorState::open_file_picker
    implements: "Uses workspace's file_index for Cmd+P file picker"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_selector_confirm
    implements: "Records selection in workspace's file_index for recency"
  - ref: crates/editor/src/editor_state.rs#EditorState::tick_picker
    implements: "Streaming refresh using workspace's file_index cache version"
  - ref: crates/editor/src/lib.rs
    implements: "Module declaration for dir_picker"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tiling_tab_movement
- tiling_tree_model
---

# Chunk Goal

## Minor Goal

When the user creates a new workspace via Cmd+N, present a standard macOS directory picker dialog (`NSOpenPanel` configured for directory selection) so the user can choose a working directory for the new workspace. The selected directory becomes the workspace's `root_path`, which determines:

1. **Terminal working directory** — every terminal opened in the workspace starts in that directory (already wired via `root_path` in the terminal spawn path).
2. **File picker search root** — the fuzzy file picker (Cmd+P) searches files within that directory.

Currently, `new_workspace()` in `editor_state.rs` sets `root_path` to `std::env::current_dir()`, and the `FileIndex` is initialized once at startup with the process's cwd. This chunk changes the flow so that:
- Cmd+N opens an `NSOpenPanel` directory picker instead of immediately creating the workspace.
- If the user selects a directory, a new workspace is created with that directory as `root_path`, and the workspace's file index is initialized to search that directory.
- If the user cancels the dialog, no workspace is created.

This also requires making the `FileIndex` per-workspace (or re-initializing it on workspace switch) so that each workspace's file picker searches its own `root_path`. Currently there is a single `FileIndex` on `EditorState`.

## Success Criteria

- Pressing Cmd+N opens a standard macOS `NSOpenPanel` configured for directory selection (not file selection).
- Selecting a directory creates a new workspace whose `root_path` is set to the chosen directory.
- Cancelling the dialog does not create a workspace or change any state.
- Terminals opened in the new workspace start in the selected directory.
- Opening the file picker (Cmd+P) in the new workspace searches files under that workspace's `root_path`.
- The workspace label in the left rail shows the directory name (last path component).
- Existing workspaces and their file indexes are unaffected by creating a new workspace.