---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/buffer_file_watcher.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/lib.rs
code_references:
  - ref: crates/editor/src/buffer_file_watcher.rs#BufferFileWatcher
    implements: "Core per-buffer file watcher struct managing watchers for external files"
  - ref: crates/editor/src/buffer_file_watcher.rs#BufferFileWatcher::is_external
    implements: "Check if a path is outside the workspace root"
  - ref: crates/editor/src/buffer_file_watcher.rs#BufferFileWatcher::register
    implements: "Register a watch for an external file with reference counting"
  - ref: crates/editor/src/buffer_file_watcher.rs#BufferFileWatcher::unregister
    implements: "Unregister a watch and cleanup when reference count reaches zero"
  - ref: crates/editor/src/buffer_file_watcher.rs#spawn_watcher_thread
    implements: "Event processing thread with debouncing and filtering"
  - ref: crates/editor/src/editor_state.rs#EditorState::buffer_file_watcher
    implements: "Integration of per-buffer watcher into EditorState"
  - ref: crates/editor/src/editor_state.rs#EditorState::set_event_sender
    implements: "Wire buffer file watcher callback to event channel"
  - ref: crates/editor/src/editor_state.rs#EditorState::associate_file
    implements: "Register external file watch when file is associated"
  - ref: crates/editor/src/editor_state.rs#EditorState::close_tab
    implements: "Unregister external file watch when tab is closed"
  - ref: crates/editor/src/lib.rs
    implements: "Module export for buffer_file_watcher"
narrative: null
investigation: concurrent_edit_sync
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- file_change_events
created_after:
- terminal_cursor_shading
---

# Chunk Goal

## Minor Goal

Watch all open buffers for filesystem changes, not just files within the workspace root. Currently the `FileIndex` watcher (`crates/editor/src/file_index.rs`) watches the workspace root directory recursively, so files opened from outside the workspace (e.g., via Cmd+O file picker navigating to an external path) are invisible to the watcher. External modifications to these files produce no `FileChanged` events and the buffer shows stale content.

This chunk adds per-buffer file watching so that any file with an open buffer — regardless of whether it lives inside or outside the workspace — triggers `FileChanged` events when modified externally. This completes the file change detection foundation so that downstream chunks (base_snapshot_reload, three_way_merge) work for all open files.

## Success Criteria

- When a file outside the workspace is opened, a watcher is registered for that file (or its parent directory)
- External modifications to files opened from outside the workspace produce `FileChanged(PathBuf)` events through the existing event channel
- The same debouncing and self-write suppression from `file_change_events` applies to non-workspace files
- When a non-workspace buffer is closed, its watcher is cleaned up (no leaked watchers)
- Files inside the workspace continue to use the existing `FileIndex` watcher (no duplicate events)
- The fix is transparent — no new user-facing UI; buffers simply detect changes regardless of origin