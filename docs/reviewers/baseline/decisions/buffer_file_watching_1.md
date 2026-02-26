---
decision: APPROVE
summary: All success criteria satisfied; implementation follows documented patterns with proper integration into existing file change infrastructure.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: When a file outside the workspace is opened, a watcher is registered for that file (or its parent directory)

- **Status**: satisfied
- **Evidence**: `BufferFileWatcher::register()` in `buffer_file_watcher.rs:144-237` creates a watcher for the parent directory of external files. The watcher is created via `RecommendedWatcher::new()` and watches the parent directory in non-recursive mode (line 213). Integration point is in `editor_state.rs:3301-3307` where `register()` is called after `associate_file()`.

### Criterion 2: External modifications to files opened from outside the workspace produce `FileChanged(PathBuf)` events through the existing event channel

- **Status**: satisfied
- **Evidence**: The callback set in `editor_state.rs:563-566` calls `event_sender.send_file_changed(path)`, reusing the existing `FileChanged` event mechanism from `file_change_events`. The watcher thread in `spawn_watcher_thread()` (lines 306-352) filters for `Modify(Data(Content))` events and invokes the callback after debouncing.

### Criterion 3: The same debouncing and self-write suppression from `file_change_events` applies to non-workspace files

- **Status**: satisfied
- **Evidence**: The watcher thread uses `FileChangeDebouncer::with_default()` (line 313) — the same debouncer used by `FileIndex`. Events flow through `send_file_changed()` which goes through the same event channel where `FileChangeSuppression` applies. Self-write suppression is inherited from the shared event processing path.

### Criterion 4: When a non-workspace buffer is closed, its watcher is cleaned up (no leaked watchers)

- **Status**: satisfied
- **Evidence**: `close_tab_at_index()` in `editor_state.rs:3856-3862` extracts the associated file path before closing and calls `buffer_file_watcher.unregister(path)` after the tab is removed. The `unregister()` method (lines 247-274) decrements the reference count and removes the watcher when it reaches zero. The `WatchEntry` drop cleans up the thread via channel disconnection.

### Criterion 5: Files inside the workspace continue to use the existing `FileIndex` watcher (no duplicate events)

- **Status**: satisfied
- **Evidence**: `is_external()` (lines 123-128) returns `false` for paths that `starts_with(workspace_root)`. The `register()` method short-circuits at lines 145-148 with `if !self.is_external(path) { return Ok(()); }`. Unit test `test_register_internal_file_is_noop` confirms this behavior.

### Criterion 6: The fix is transparent — no new user-facing UI; buffers simply detect changes regardless of origin

- **Status**: satisfied
- **Evidence**: The implementation adds no new UI elements, dialogs, or visible indicators. Changes are purely in the backend infrastructure: `BufferFileWatcher` struct, integration points in `EditorState`. The existing `FileChanged` event processing handles both workspace and non-workspace files identically.

## Additional Observations

1. **Reference counting for shared directories**: When multiple external files share a parent directory, they share a single watcher (verified by `test_multiple_files_same_directory_share_watcher`). This is an efficiency optimization mentioned in the PLAN.md.

2. **Graceful degradation**: If watcher creation fails, the error is logged but doesn't prevent file opening (`eprintln!` at line 3306). This matches the plan's guidance: "watching is a nice-to-have, not critical."

3. **Workspace root updates**: The implementation correctly updates the workspace root on workspace switch, close, and creation (multiple integration points in `editor_state.rs`).

4. **Code backreferences**: Proper chunk backreferences are present in all new code.
