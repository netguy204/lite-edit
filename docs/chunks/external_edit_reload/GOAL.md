---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/file_index.rs
- crates/editor/src/buffer_file_watcher.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/workspace.rs
- crates/editor/src/drain_loop.rs
code_references:
- ref: crates/editor/src/file_index.rs#handle_fs_event
  implements: "Atomic-write detection: registers Create and Rename events for already-known files with debouncer so file change callback fires"
- ref: crates/editor/src/buffer_file_watcher.rs#spawn_watcher_thread
  implements: "Buffer watcher handles Create and Rename events in addition to Modify, covering atomic-write patterns"
- ref: crates/editor/src/workspace.rs#Tab
  implements: "Per-tab last_known_mtime field for mtime-based staleness detection"
- ref: crates/editor/src/editor_state.rs#EditorState::check_active_tab_staleness
  implements: "Mtime-based staleness check on pane focus change — reloads clean tabs, merges dirty tabs"
- ref: crates/editor/src/editor_state.rs#EditorState::check_workspace_staleness
  implements: "Mtime-based staleness check on workspace switch — checks all tabs in all panes"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- orchestrator_monitor_skill
---

# Chunk Goal

## Minor Goal

Fix external file change detection so that buffers reload automatically when
an external process (e.g. Claude Code, git, another editor) modifies a file
that is open in lite-edit.

Currently, when an external process edits a file:
- If the pane with that file is **not focused**, the change is not picked up
- **Refocusing** the pane still does not trigger a reload
- Only a **manual file reload** shows the updated contents

The file watcher infrastructure exists (chunks `file_change_events`,
`base_snapshot_reload`) but the event-to-reload pipeline appears to break
when the buffer's pane is not focused. The fix should ensure that:
1. File watcher events trigger buffer reloads regardless of pane focus state
2. If the watcher event was somehow missed, refocusing a pane checks file
   mtime and reloads if stale
3. Edits to files in inactive workspaces are also reflected when the user
   switches back to that workspace

## Success Criteria

- An external edit to a file open in an unfocused pane is reflected in the
  buffer within a few seconds, without user interaction
- Refocusing a pane with a stale buffer triggers a reload
- Switching to an inactive workspace reloads any buffers modified while
  the workspace was inactive
- No reload occurs if the buffer has unsaved local changes (dirty buffer
  should prompt or skip, not silently overwrite)
- Existing file watcher behavior for focused panes is unaffected