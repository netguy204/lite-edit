<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Route file content-modification events from the existing `FileIndex` filesystem watcher to the editor event loop. The `FileIndex` already receives `Modify(Data(Content))` events from the `notify` crate (FSEvents on macOS) but discards them at `file_index.rs:529-531`. This chunk adds the infrastructure to:

1. Forward those events to a new `EditorEvent::FileChanged(PathBuf)` variant
2. Apply ~100ms debouncing to coalesce rapid successive writes
3. Suppress self-triggered events when `save_file()` writes to disk

The investigation (`docs/investigations/concurrent_edit_sync/OVERVIEW.md`) confirmed that the existing `notify` watcher reliably delivers `Modify(Data(Content))` events with 2-61ms latency. No second watcher is needed.

**Key architectural points:**

- The `FileIndex` lives in `Workspace`, which doesn't have direct access to `EventSender`. We'll add an optional callback (`on_file_changed: Option<Box<dyn Fn(PathBuf) + Send + Sync>>`) to `FileIndex` that the watcher thread invokes when content changes are detected.
- `EditorState` will wire this callback to send `EditorEvent::FileChanged(PathBuf)` through the existing event channel.
- Debouncing will be implemented in the watcher thread using a simple timer-based approach: track `(PathBuf, Instant)` of pending events and only forward after 100ms of quiet time.
- Self-write suppression will use a short-lived set of paths stored in `EditorState`, populated before `save_file()` writes and cleared after a short timeout (or on the next event loop cycle).

**Testing approach (per TESTING_PHILOSOPHY.md):**

- Unit tests for the debouncing logic (pure time-based state machine, no filesystem)
- Unit tests for the self-write suppression registry (pure HashSet operations)
- Integration tests (marked `#[ignore]`) for end-to-end event flow (require filesystem events, which are slow/flaky in CI)
- The drain loop handler will be a no-op placeholder, so no behavior to test there yet

## Subsystem Considerations

No existing subsystems are directly relevant to this work. The chunk establishes the foundation for concurrent-edit-sync but doesn't touch any cross-cutting patterns documented in `docs/subsystems/`.

## Sequence

### Step 1: Add `EditorEvent::FileChanged(PathBuf)` variant

Add a new variant to `EditorEvent` in `crates/editor/src/editor_event.rs`:

```rust
/// A file was modified externally (on disk)
///
/// This event is sent when the filesystem watcher detects that a file
/// within the workspace was modified by an external process. The path
/// is absolute.
FileChanged(PathBuf),
```

Update `is_priority_event()` to return `true` for `FileChanged` (external edits should be processed promptly, not delayed behind accumulated PTY output).

Update `is_user_input()` to return `false` for `FileChanged` (it's not user input, so shouldn't reset cursor blink).

Location: `crates/editor/src/editor_event.rs`

### Step 2: Add `send_file_changed` method to `EventSender`

Add a method to `EventSender` in `crates/editor/src/event_channel.rs`:

```rust
/// Sends a file-changed event to the channel.
///
/// This is called from the FileIndex watcher thread when an external
/// content modification is detected.
pub fn send_file_changed(&self, path: PathBuf) -> Result<(), SendError<EditorEvent>> {
    let result = self.inner.sender.send(EditorEvent::FileChanged(path));
    (self.inner.run_loop_waker)();
    result
}
```

Location: `crates/editor/src/event_channel.rs`

### Step 3: Add no-op handler for `FileChanged` in the drain loop

Add a match arm in `EventDrainLoop::process_single_event()`:

```rust
EditorEvent::FileChanged(_path) => {
    // Placeholder: future chunks will implement reload/merge behavior
}
```

This ensures the event type compiles and flows through the system, even though the handler does nothing yet.

Location: `crates/editor/src/drain_loop.rs`

### Step 4: Implement debounce state machine

Create a new module `crates/editor/src/file_change_debouncer.rs` that implements a debouncing state machine:

```rust
/// Debounces file change events, coalescing rapid successive writes.
///
/// When a file change is registered, the debouncer waits 100ms for
/// additional changes before emitting. If another change arrives for
/// the same path within the window, the timer resets.
pub struct FileChangeDebouncer {
    /// Pending paths and when they were last changed
    pending: HashMap<PathBuf, Instant>,
    /// Debounce window duration
    debounce_ms: u64,
}

impl FileChangeDebouncer {
    pub fn new(debounce_ms: u64) -> Self;

    /// Register a file change. Returns paths that should be emitted now
    /// (i.e., paths whose debounce window has expired).
    pub fn register(&mut self, path: PathBuf, now: Instant) -> Vec<PathBuf>;

    /// Check for paths ready to emit (called periodically).
    pub fn flush_ready(&mut self, now: Instant) -> Vec<PathBuf>;
}
```

The debouncer is a pure data structure with no I/O, making it easy to test. The watcher thread will call `register()` on each event and periodically call `flush_ready()`.

Location: `crates/editor/src/file_change_debouncer.rs`

### Step 5: Add file change callback to `FileIndex`

Modify `FileIndex` to accept an optional callback for file content changes:

1. Add a new field to `EventSenderInner` (or a new inner struct):
   ```rust
   file_change_callback: Option<Arc<dyn Fn(PathBuf) + Send + Sync>>,
   ```

2. Add a method to set the callback:
   ```rust
   pub fn set_file_change_callback<F>(&mut self, callback: F)
   where
       F: Fn(PathBuf) + Send + Sync + 'static
   ```

3. Modify `handle_fs_event()` to invoke the callback (with debouncing) when `EventKind::Modify(ModifyKind::Data(DataChange::Content))` is detected.

Actually, the callback needs to be set at construction time since `FileIndex::start()` spawns threads immediately. We'll modify `FileIndex::start()` to optionally accept the callback, or add a `start_with_callback()` variant.

**Design decision:** Since the watcher thread is already running and we need the debouncer state to live there, the cleanest approach is:

1. The callback is set at `FileIndex` construction time
2. The watcher thread owns a `FileChangeDebouncer` instance
3. On each `Modify(Data(Content))` event, the watcher thread calls `debouncer.register(path, Instant::now())`
4. The watcher thread's main loop also calls `debouncer.flush_ready()` on each iteration and invokes the callback for ready paths

Location: `crates/editor/src/file_index.rs`

### Step 6: Wire up the file change callback in `Workspace`

Modify `Workspace::new()` and `Workspace::with_empty_tab()` to accept an optional `EventSender` and wire up the file change callback:

```rust
pub fn new(id: WorkspaceId, label: String, root_path: PathBuf, event_sender: Option<EventSender>) -> Self {
    let file_index = if let Some(sender) = event_sender {
        FileIndex::start_with_callback(root_path.clone(), move |path| {
            let _ = sender.send_file_changed(path);
        })
    } else {
        FileIndex::start(root_path.clone())
    };
    // ...
}
```

This requires threading `EventSender` through from `EditorState` where it's available.

Location: `crates/editor/src/workspace.rs`, `crates/editor/src/editor_state.rs`

### Step 7: Implement self-write suppression registry

Add a self-write suppression mechanism to `EditorState`:

```rust
/// Paths to ignore for file change events (self-write suppression).
///
/// When save_file() writes to disk, it adds the path here. File change
/// events for paths in this set are ignored. Paths are cleared after
/// a short timeout or on the next event loop tick.
suppress_file_changes: HashSet<PathBuf>,
```

Add methods:
```rust
/// Suppress file change events for this path temporarily.
pub fn suppress_file_change(&mut self, path: PathBuf);

/// Check if a path is suppressed, and if so, remove it from the set.
/// Returns true if the path WAS suppressed (and should be ignored).
pub fn is_file_change_suppressed(&mut self, path: &Path) -> bool;
```

Modify `save_file()` to call `suppress_file_change()` before writing.

The suppression is cleared when `is_file_change_suppressed()` is called and returns true (single-use suppression), ensuring we don't permanently ignore a file.

Location: `crates/editor/src/editor_state.rs`

### Step 8: Update `FileChanged` handler to check suppression

Update the `FileChanged` handler in the drain loop to check suppression:

```rust
EditorEvent::FileChanged(path) => {
    // Check if this is a self-triggered event (our own save)
    if self.state.is_file_change_suppressed(&path) {
        // Ignore - this was our own write
        return;
    }
    // Placeholder: future chunks will implement reload/merge behavior
}
```

Location: `crates/editor/src/drain_loop.rs`

### Step 9: Add unit tests for debouncer

Write comprehensive unit tests for `FileChangeDebouncer`:

- Test that a single event is not emitted immediately
- Test that an event is emitted after the debounce window
- Test that rapid successive writes to the same file coalesce into one event
- Test that changes to different files are tracked independently
- Test boundary conditions (empty state, multiple files, exact timing)

Location: `crates/editor/src/file_change_debouncer.rs` (in `#[cfg(test)]` module)

### Step 10: Add unit tests for self-write suppression

Write unit tests for the suppression registry:

- Test that suppressing a path causes `is_file_change_suppressed` to return true
- Test that the path is removed after checking (single-use)
- Test that unsuppressed paths return false
- Test multiple paths can be suppressed independently

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 11: Add integration test for end-to-end event flow (ignored)

Write an integration test (marked `#[ignore]`) that:

1. Creates a `FileIndex` with a callback that captures emitted paths
2. Writes to a file in the watched directory
3. Waits for the debounce window + some margin
4. Verifies the callback was invoked with the correct path

This test is marked `#[ignore]` because FSEvents on macOS has variable latency and may not deliver events reliably in CI environments.

Location: `crates/editor/src/file_index.rs` (in `#[cfg(test)]` module)

### Step 12: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched:

```yaml
code_paths:
  - crates/editor/src/editor_event.rs
  - crates/editor/src/event_channel.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/file_change_debouncer.rs
  - crates/editor/src/file_index.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
```

## Dependencies

- No external crate dependencies needed. The `notify` crate is already in use.
- No other chunks need to complete first (this is the foundation chunk).

## Risks and Open Questions

1. **Thread safety of callback invocation:** The callback is invoked from the watcher thread. Using `EventSender::send_file_changed()` is safe because `EventSender` is `Send + Sync` (it wraps an `Arc<Sender>`).

2. **Debounce timer implementation:** The watcher thread's main loop already polls with a 100ms timeout (`recv_timeout(Duration::from_millis(100))`). We can piggyback on this to also flush the debouncer, avoiding the need for a separate timer thread.

3. **Path normalization:** The `notify` crate delivers absolute paths, which should match what we store in `Tab::associated_file`. May need to canonicalize both sides to handle symlinks. Defer this complexity unless issues arise.

4. **Event ordering:** If a file is modified multiple times in rapid succession, the debouncer ensures we emit only one event. But what if the file is deleted before we emit? The delete event will arrive separately, and the `deletion_rename_handling` chunk will handle it. The content-change event can safely be a no-op if the file no longer exists when we try to reload.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
