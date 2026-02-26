---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/buffer_file_watcher.rs
- crates/editor/src/file_index.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/editor_event.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/main.rs
- crates/editor/src/event_channel.rs
code_references:
  - ref: crates/editor/src/buffer_file_watcher.rs#PausedWatcherState
    implements: "State preserved across pause/resume for external file watchers"
  - ref: crates/editor/src/buffer_file_watcher.rs#BufferFileWatcher::pause
    implements: "Pause external file watchers, capturing modification times"
  - ref: crates/editor/src/buffer_file_watcher.rs#BufferFileWatcher::resume
    implements: "Resume external file watchers, detecting changes while paused"
  - ref: crates/editor/src/file_index.rs#PausedFileIndexState
    implements: "State preserved across pause/resume for workspace file index"
  - ref: crates/editor/src/file_index.rs#FileIndex::pause
    implements: "Pause workspace watcher thread processing"
  - ref: crates/editor/src/file_index.rs#FileIndex::resume
    implements: "Resume workspace watcher and detect changes while paused"
  - ref: crates/editor/src/editor_state.rs#PausedFileWatchersState
    implements: "Combined paused state for all file watchers"
  - ref: crates/editor/src/editor_state.rs#EditorState::pause_file_watchers
    implements: "Coordinate pausing of all file watchers for App Nap"
  - ref: crates/editor/src/editor_state.rs#EditorState::resume_file_watchers
    implements: "Coordinate resuming of all file watchers after App Nap"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::PauseFileWatchers
    implements: "Event type for pausing file watchers"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::ResumeFileWatchers
    implements: "Event type for resuming file watchers"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_pause_file_watchers
    implements: "Send pause event through the event channel"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_resume_file_watchers
    implements: "Send resume event through the event channel"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- app_nap_blink_timer
created_after:
- buffer_file_watching
- highlight_injection
---

# Chunk Goal

## Minor Goal

Pause file watchers when the app is backgrounded (window not key) and resume
them when the app returns to the foreground. While FSEvents is already
coalesced by the OS, the `notify` crate's watcher threads still wake the
process to deliver events through the event channel, which can prevent or
interrupt App Nap. Pausing watchers eliminates this wakeup source entirely.

On resume, any file changes that accumulated while paused should be detected
(either by re-scanning watched paths or by relying on FSEvents' coalesced
delivery upon re-subscribe). This ensures the editor shows up-to-date content
when the user switches back.

Supports GOAL.md's "minimal footprint" property.

## Success Criteria

- When the window resigns key status, file watchers are paused (stopped or
  unsubscribed) so they no longer deliver events or wake the process.
- When the window becomes key, file watchers are resumed and any changes that
  occurred while paused are detected and applied.
- No stale buffer content after switching back to lite-edit following an
  external file modification.
- No regressions in existing file watching behavior or tests.
- The pause/resume mechanism integrates cleanly with the `buffer_file_watcher`
  infrastructure from the `buffer_file_watching` chunk.