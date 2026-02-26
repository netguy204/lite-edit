---
decision: FEEDBACK
summary: "Prior feedback not addressed: EventSender still not propagated from EditorState to Editor, so workspaces never receive file change callbacks"
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: A new `EditorEvent::FileChanged(PathBuf)` variant exists and flows through the event channel and drain loop

- **Status**: satisfied
- **Evidence**: `editor_event.rs:53-59` defines `FileChanged(PathBuf)` variant with backreference comment and proper documentation. `drain_loop.rs:196-198` handles it in `process_single_event()`. Tests at `editor_event.rs:163-173` verify it's a priority event and not user input.

### Criterion 2: `EventSender` has a `send_file_changed(path)` method

- **Status**: satisfied
- **Evidence**: `event_channel.rs:181-190` implements `send_file_changed(path: PathBuf)` with proper wake-up logic. Tests at lines 479-500 verify the method sends events correctly and calls the waker.

### Criterion 3: The `FileIndex` watcher thread forwards `Modify(Data(Content))` events for files within the workspace via the event channel

- **Status**: gap
- **Evidence**: `file_index.rs:108-113` provides `start_with_callback()` and the watcher correctly routes events at line 603-606 via the debouncer. HOWEVER, the wiring is broken:
  1. `main.rs:417` calls `state.set_event_sender(sender.clone())`
  2. `EditorState::set_event_sender()` (editor_state.rs:443-445) stores the sender in `self.event_sender` for PTY wakeups but does NOT call `self.editor.set_event_sender()`
  3. `Editor::set_event_sender()` (workspace.rs:1112) exists and would propagate to workspaces, but is NEVER CALLED
  4. Result: All workspaces are created with `FileIndex::start()` (no callback) instead of `FileIndex::start_with_callback()`

  The iteration 1 review identified this same issue. It was not fixed.

### Criterion 4: Rapid successive writes to the same file within 100ms produce only one `FileChanged` event (debouncing)

- **Status**: satisfied
- **Evidence**: `file_change_debouncer.rs` implements proper debouncing with comprehensive unit tests (lines 107-279). Tests cover: single event not emitted immediately, event emitted after debounce window, rapid writes coalesce, different files tracked independently, boundary conditions. The debouncer is integrated in `file_index.rs:510` and flushed at line 530-536.

### Criterion 5: A `save_file()` call does NOT produce a `FileChanged` event for the saved file (self-write suppression)

- **Status**: satisfied
- **Evidence**: `file_change_suppression.rs` implements TTL-based (1 second) suppression with one-shot behavior and comprehensive tests (lines 120-250). Called in `editor_state.rs:2898` via `self.file_change_suppression.suppress(path.clone())` before `fs::write()`. Checked in `drain_loop.rs:213-216` via `state.is_file_change_suppressed()`.

### Criterion 6: The drain loop receives `FileChanged` events but does not yet act on them (handler is a no-op placeholder for the next chunk)

- **Status**: satisfied
- **Evidence**: `drain_loop.rs:202-218` shows `handle_file_changed()` which checks suppression then does nothing (comment at line 217: "Placeholder: future chunks will implement reload/merge behavior").

### Criterion 7: Existing `FileIndex` behavior (path cache for file picker) is unchanged

- **Status**: satisfied
- **Evidence**: All 1043 existing tests pass including `file_index::tests::*`. The `handle_fs_event()` function in `file_index.rs` maintains path cache logic for Create/Remove/Modify(Name) events unchanged (lines 567-619). Content modification handling at lines 603-606 is additive and does not affect the path cache.

## Feedback Items

### Issue 1: EventSender not propagated to Editor (recurring from iteration 1)

- **id**: issue-evt-prop-v2
- **location**: `crates/editor/src/editor_state.rs:443-445`
- **concern**: `EditorState::set_event_sender()` stores the event sender only on `EditorState.event_sender` (used for PTY wakeups) but does NOT propagate to `self.editor.set_event_sender()`. As a result, `Editor.event_sender` is always `None`, and all workspaces are created via `FileIndex::start()` (without callback) instead of `FileIndex::start_with_callback()`. File change events will never be forwarded to the event channel. This is the same issue identified in iteration 1 review.
- **suggestion**: Add the following line to `EditorState::set_event_sender()`:
  ```rust
  self.editor.set_event_sender(sender.clone());
  ```
  This ensures `Editor.event_sender` is set, so `Editor::new_workspace_internal()` (workspace.rs:1137) will pass the sender to workspace constructors, enabling file change callbacks.
- **severity**: functional
- **confidence**: high

## Escalation Reason

N/A - not escalating.
