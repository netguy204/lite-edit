---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::initialize_symbol_indexing_for_all_workspaces
    implements: "Method to initialize symbol indexing for all workspaces after session restore"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_window
    implements: "Calls initialize_symbol_indexing_for_all_workspaces after session restore"
narrative: null
investigation: cross_file_goto_definition
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- alt_screen_viewport_reset
---
# Chunk Goal

## Minor Goal

Initialize the symbol index for all workspaces after session restore. Currently, the session restoration path in `main.rs:402-408` replaces the editor with a restored one but never calls `start_symbol_indexing()` on any workspace. Since session restore is the default startup path (used whenever a previous session exists), most users will have `symbol_index: None` on every workspace, making cross-file go-to-definition completely non-functional.

The fix is to iterate all restored workspaces after `state.editor = editor` and call `ws.start_symbol_indexing(Arc::clone(&state.language_registry))` on each.

## Success Criteria

- After session restore, every workspace has `symbol_index` set to `Some(...)` (not `None`)
- Background indexing starts for each restored workspace's `root_path`
- Cross-file go-to-definition no longer shows "Symbol index not initialized" after a session restore startup
- A test verifies that session-restored workspaces have their symbol index initialized
- Existing session restore tests continue to pass
