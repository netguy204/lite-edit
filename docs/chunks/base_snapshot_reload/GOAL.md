---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/workspace.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/drain_loop.rs
code_references:
- ref: crates/editor/src/workspace.rs#Tab::base_content
  implements: "Base content snapshot field for tracking file state as last known on disk"
- ref: crates/editor/src/workspace.rs#Workspace::find_tab_by_path
  implements: "Tab lookup by file path for handling file change events"
- ref: crates/editor/src/workspace.rs#Workspace::find_tab_mut_by_path
  implements: "Mutable tab lookup by file path for reload operations"
- ref: crates/editor/src/editor_state.rs#clamp_position_to_buffer
  implements: "Cursor clamping utility for position preservation after reload"
- ref: crates/editor/src/editor_state.rs#EditorState::reload_file_tab
  implements: "Clean buffer reload logic when file changes externally"
- ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_changed
  implements: "File change event handler that triggers reload for clean buffers"
narrative: null
investigation: concurrent_edit_sync
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- file_change_events
created_after:
- emacs_keybindings
- terminal_close_guard
- welcome_file_backed
---

# Chunk Goal

## Minor Goal

Store a base content snapshot on each `Tab` and implement automatic reload for clean (unmodified) buffers when the file changes on disk. This establishes the base version tracking needed for three-way merge and delivers the simpler half of the concurrent-edit experience: if you haven't edited the buffer, it silently stays in sync with disk.

The `Tab` struct gains a `base_content: Option<String>` field that holds the file content as last known on disk. This is populated when a file is loaded (`associate_file()`) and when a file is saved (`save_file()`). When a `FileChanged` event arrives for a tab with `dirty == false`, the buffer is reloaded from disk and the base snapshot is updated.

## Success Criteria

- `Tab` has a `base_content: Option<String>` field
- `base_content` is set to the file's content when loaded via `associate_file()`
- `base_content` is set to the buffer's content when saved via `save_file()`
- When `EditorEvent::FileChanged(path)` arrives and the matching tab has `dirty == false`:
  - The buffer is reloaded from disk (same path as `associate_file()` but without changing the associated file)
  - `base_content` is updated to the new disk content
  - The viewport is refreshed (`DirtyRegion::FullViewport`)
  - Cursor position is preserved if still valid (clamped to buffer bounds if not)
- When `FileChanged` arrives and the matching tab has `dirty == true`, no reload happens (this is deferred to the three_way_merge chunk)
- When `FileChanged` arrives and no tab has the matching path, the event is ignored
- Syntax highlighting is re-applied after reload

## Rejected Ideas

### Re-read file on demand instead of storing a snapshot

We could skip storing `base_content` and re-read the file from disk when needed for merging, using mtime to detect staleness.

Rejected because: Re-reading at merge time adds I/O latency to the merge path and introduces a race condition (the file could change again between the notify event and the re-read). Storing the snapshot is simple, and per the investigation (H4), the memory cost is negligible for typical source files.