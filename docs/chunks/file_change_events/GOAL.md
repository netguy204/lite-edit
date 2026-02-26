---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_event.rs
- crates/editor/src/event_channel.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/file_change_debouncer.rs
- crates/editor/src/file_change_suppression.rs
- crates/editor/src/file_index.rs
- crates/editor/src/workspace.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_event.rs#EditorEvent::FileChanged
    implements: "New event variant for external file content modifications"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_file_changed
    implements: "Thread-safe method to send FileChanged events from watcher thread"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_changed
    implements: "Drain loop handler for FileChanged events with suppression check"
  - ref: crates/editor/src/file_change_debouncer.rs#FileChangeDebouncer
    implements: "Pure debouncing state machine — coalesces rapid writes within 100ms window"
  - ref: crates/editor/src/file_change_suppression.rs#FileChangeSuppression
    implements: "Self-write suppression registry — prevents save_file triggering reload"
  - ref: crates/editor/src/file_index.rs#FileIndex::start_with_callback
    implements: "FileIndex constructor that accepts a file change callback"
  - ref: crates/editor/src/file_index.rs#FileChangeCallback
    implements: "Type alias for the file change callback signature"
  - ref: crates/editor/src/editor_state.rs#EditorState::is_file_change_suppressed
    implements: "Suppression check delegating to FileChangeSuppression"
narrative: null
investigation: concurrent_edit_sync
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- emacs_keybindings
- terminal_close_guard
- welcome_file_backed
---

# Chunk Goal

## Minor Goal

Route file content-modification events from the existing `FileIndex` filesystem watcher to the editor event loop. This is the foundation for all concurrent-edit-sync behavior — without detecting that a file has changed on disk, no reload or merge can happen.

Currently, the `FileIndex` watcher (`crates/editor/src/file_index.rs`) receives `Modify(Data(Content))` events from the `notify` crate but discards them at line 529-531. This chunk routes those events to a new `EditorEvent::FileChanged(PathBuf)` variant so the drain loop can act on them.

Key design requirements from the investigation (`docs/investigations/concurrent_edit_sync/OVERVIEW.md`):
- Filter on `Modify(Data(Content))` specifically — ignore metadata-only changes
- Add ~100ms debounce window to coalesce rapid successive writes (e.g., Claude Code writing a file with multiple syscalls)
- Suppress self-triggered events: when `save_file()` writes to disk, the watcher will fire for our own write. Add a short-lived suppression mechanism (e.g., a set of paths to ignore, cleared after a timeout) around save operations

## Success Criteria

- A new `EditorEvent::FileChanged(PathBuf)` variant exists and flows through the event channel and drain loop
- `EventSender` has a `send_file_changed(path)` method
- The `FileIndex` watcher thread forwards `Modify(Data(Content))` events for files within the workspace via the event channel
- Rapid successive writes to the same file within 100ms produce only one `FileChanged` event (debouncing)
- A `save_file()` call does NOT produce a `FileChanged` event for the saved file (self-write suppression)
- The drain loop receives `FileChanged` events but does not yet act on them (handler is a no-op placeholder for the next chunk)
- Existing `FileIndex` behavior (path cache for file picker) is unchanged

## Rejected Ideas

### Create a second `RecommendedWatcher` for buffer-level watching

We could create a separate watcher that only watches files with open buffers, rather than extending the existing workspace-wide watcher.

Rejected because: The existing watcher already receives the events we need — they're just discarded. Adding a second watcher adds complexity and resource usage for no benefit. The investigation prototype confirmed that `Modify(Data(Content))` events arrive reliably from the existing watcher.