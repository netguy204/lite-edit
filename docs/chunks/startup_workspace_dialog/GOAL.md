---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Editor::new_deferred
    implements: "Deferred editor initialization with no workspaces, enabling startup dialog flow"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_deferred
    implements: "Deferred state initialization that creates no workspace until directory is selected"
  - ref: crates/editor/src/editor_state.rs#EditorState::add_startup_workspace
    implements: "Adds initial workspace with user-selected directory and derives label from path"
  - ref: crates/editor/src/main.rs#AppDelegate::resolve_startup_directory
    implements: "Resolves startup directory from CLI argument or NSOpenPanel picker with graceful fallback"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_window
    implements: "Shows directory picker before window creation, exits gracefully on cancel"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tiling_workspace_integration
- workspace_dir_picker
- workspace_identicon
---

# Chunk Goal

## Minor Goal

On application startup, present the workspace open dialog (NSOpenPanel directory picker) so the user can select the root directory for the initial workspace. This prevents the current behavior where `std::env::current_dir()` is used as the default workspace root — which can resolve to `/` when launched from Finder or Spotlight — causing the `FileIndex` path collector to crawl the entire filesystem and consume all CPU and memory.

Currently, `EditorState::new()` in `crates/editor/src/workspace.rs` unconditionally creates an initial workspace rooted at `current_dir()`. Instead, the startup flow should:

1. Show the NSOpenPanel directory picker before (or immediately after) creating the initial workspace.
2. Use the selected directory as the workspace root path.
3. If the user cancels the dialog, the application exits gracefully.

This directly supports the project's **minimal footprint** and **fast startup** goals — an editor that immediately pegs CPU indexing the entire filesystem on first launch violates both.

## Success Criteria

- When the application starts without a directory argument, the NSOpenPanel directory picker is displayed.
- The selected directory becomes the root path of the initial workspace and its `FileIndex`.
- If the user cancels the picker, the application exits gracefully.
- If a directory argument is provided on the command line (e.g. `lite-edit /some/path`), the picker is skipped and that path is used directly.
- The `FileIndex` never starts crawling `/` or any unexpectedly broad directory at startup.