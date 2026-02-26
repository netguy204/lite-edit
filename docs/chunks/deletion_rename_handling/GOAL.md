---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_event.rs
- crates/editor/src/event_channel.rs
- crates/editor/src/confirm_dialog.rs
- crates/editor/src/file_index.rs
- crates/editor/src/workspace.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_event.rs#EditorEvent::FileDeleted
    implements: "Event variant for external file deletion detection"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::FileRenamed
    implements: "Event variant for external file rename detection"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_file_deleted
    implements: "Sends file-deleted events to the channel"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_file_renamed
    implements: "Sends file-renamed events to the channel"
  - ref: crates/editor/src/confirm_dialog.rs#ConfirmDialogContext::FileDeletedFromDisk
    implements: "Context variant for file deleted confirmation dialog"
  - ref: crates/editor/src/file_index.rs#FileDeletedCallback
    implements: "Callback type for file deletion events"
  - ref: crates/editor/src/file_index.rs#FileRenamedCallback
    implements: "Callback type for file rename events"
  - ref: crates/editor/src/file_index.rs#FileIndex::start_with_callbacks
    implements: "File index constructor with all event callbacks"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_deleted
    implements: "Forwards file deleted events to editor state"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_renamed
    implements: "Forwards file renamed events to editor state"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_file_deleted
    implements: "Shows confirm dialog for deleted files"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_file_renamed
    implements: "Updates tab path and label on file rename"
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

Handle file deletion and rename events for files with open buffers. When an external program deletes a file that has an open buffer, display a confirm dialog letting the user save the buffer contents to recreate the file or abandon the buffer. When an external program renames a file, update the tab's associated file path to follow the rename.

This chunk extends the `FileChanged` event routing (from `file_change_events`) to also handle `Remove` and `Modify(Name)` events from the `notify` watcher, adding new `EditorEvent` variants for these cases.

## Success Criteria

- New `EditorEvent` variants exist for file deletion and file rename (e.g., `FileDeleted(PathBuf)` and `FileRenamed { from: PathBuf, to: PathBuf }`)
- The `FileIndex` watcher thread forwards `Remove` events for files with open buffers
- The `FileIndex` watcher thread forwards `Modify(Name(_))` rename events with both old and new paths
- When `FileDeleted` arrives for a tab:
  - A confirm dialog is displayed with two options: "Save" (recreate the file with current buffer contents) and "Abandon" (close the tab)
  - If the user chooses Save, the file is written from the buffer and the tab continues normally
  - If the user chooses Abandon, the tab is closed
- When `FileRenamed` arrives for a tab:
  - `tab.associated_file` is updated to the new path
  - The tab label updates to reflect the new filename
  - Syntax highlighting is re-evaluated if the file extension changed
- Tabs without an `associated_file` are unaffected by these events
