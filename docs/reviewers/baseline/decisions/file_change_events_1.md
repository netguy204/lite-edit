---
decision: FEEDBACK
summary: "Core infrastructure is correct but EventSender is not propagated to Editor, so workspaces never receive file change callbacks"
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: A new `EditorEvent::FileChanged(PathBuf)` variant exists and flows through the event channel and drain loop

- **Status**: satisfied
- **Evidence**: `editor_event.rs:53-59` defines `FileChanged(PathBuf)` variant with proper documentation. `drain_loop.rs:196-198` handles it in `process_single_event()`.

### Criterion 2: `EventSender` has a `send_file_changed(path)` method

- **Status**: satisfied
- **Evidence**: `event_channel.rs:181-190` implements `send_file_changed()` with proper wake-up logic and tests at lines 479-500.

### Criterion 3: The `FileIndex` watcher thread forwards `Modify(Data(Content))` events for files within the workspace via the event channel

- **Status**: gap
- **Evidence**: `file_index.rs:108-113` provides `start_with_callback()` and the watcher correctly routes events at line 603-606. However, the callback is never actually wired up for any workspace because `Editor::set_event_sender()` (workspace.rs:1112) is never called. `EditorState::set_event_sender()` (editor_state.rs:443-445) only sets the event_sender on EditorState for PTY wakeups but does NOT propagate to `self.editor.set_event_sender()`. Result: all workspaces are created without the file change callback.

### Criterion 4: Rapid successive writes to the same file within 100ms produce only one `FileChanged` event (debouncing)

- **Status**: satisfied
- **Evidence**: `file_change_debouncer.rs` implements proper debouncing with comprehensive unit tests (lines 107-279). The debouncer is integrated in `file_index.rs:510` and flushed periodically at line 530-536.

### Criterion 5: A `save_file()` call does NOT produce a `FileChanged` event for the saved file (self-write suppression)

- **Status**: satisfied
- **Evidence**: `file_change_suppression.rs` implements TTL-based suppression with comprehensive tests. Called in `editor_state.rs:2898` before `fs::write()`. Checked in `drain_loop.rs:213-216` via `is_file_change_suppressed()`.

### Criterion 6: The drain loop receives `FileChanged` events but does not yet act on them (handler is a no-op placeholder for the next chunk)

- **Status**: satisfied
- **Evidence**: `drain_loop.rs:202-218` shows `handle_file_changed()` which checks suppression then does nothing (placeholder comment at line 217).

### Criterion 7: Existing `FileIndex` behavior (path cache for file picker) is unchanged

- **Status**: satisfied
- **Evidence**: All existing `FileIndex` tests pass. The `handle_fs_event()` function in `file_index.rs` maintains path cache logic for Create/Remove/Modify(Name) events unchanged. New content-change handling is additive.

## Feedback Items

### Issue 1: EventSender not propagated to Editor

- **id**: issue-evt-prop
- **location**: `crates/editor/src/editor_state.rs:443-445`
- **concern**: `EditorState::set_event_sender()` sets the event_sender only on EditorState (for PTY wakeups) but does NOT call `self.editor.set_event_sender()`. As a result, `Editor::event_sender` is always `None`, and all workspaces are created via `FileIndex::start()` (without callback) instead of `FileIndex::start_with_callback()`. File change events will never be forwarded to the event channel.
- **suggestion**: Add `self.editor.set_event_sender(sender.clone());` to `EditorState::set_event_sender()` so that subsequent workspace creations (and ideally, existing workspaces via some re-initialization mechanism) receive the callback.
- **severity**: functional
- **confidence**: high
