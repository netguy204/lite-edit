---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/main.rs
- crates/editor/src/session.rs
- crates/editor/tests/session_persistence.rs
code_references:
- ref: crates/editor/src/editor_state.rs#EditorState::setup_all_tab_highlighting
  implements: "Post-restore highlighting setup for all tabs across all workspaces"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- tsx_goto_functions
---

# Chunk Goal

## Minor Goal

When the application starts and restores a saved workspace session, all restored
buffers display as plain unstyled text. Syntax highlighting only appears after
manually reloading each buffer. This chunk fixes the restore path to apply syntax
highlighting to all restored tabs.

**Root cause:** During session restoration, `PaneData::into_pane()` in
`session.rs` creates tabs via `Tab::new_file()` which initializes with
`highlighter: None`. The code never calls `Tab::setup_highlighting()` on
restored tabs. In contrast, files opened via the file picker or directory
loading do call `setup_highlighting()`.

**Fix:** After restoring tabs from the session, call `setup_highlighting()` on
each tab with the `LanguageRegistry` and theme, mirroring what the normal
file-open paths do.

## Success Criteria

- Restored buffers have syntax highlighting immediately after session restore,
  with no user interaction required
- Verified by: open several files of different languages, quit, relaunch -- all
  files should be highlighted on first render
- No regression in normal file-open highlighting behavior