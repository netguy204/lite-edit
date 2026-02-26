<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds per-buffer file watching for files opened from outside the workspace. Currently, the `FileIndex` watcher only monitors the workspace root directory, so files opened via Cmd+O from external paths (e.g., `/etc/hosts`, `~/other-project/file.rs`) are invisible to the change detection system.

**Architecture Decision: Separate Per-Buffer Watcher**

We introduce a new `BufferFileWatcher` that manages individual file watchers for non-workspace files. This is separate from the `FileIndex` watcher because:

1. **Different scope**: `FileIndex` recursively watches a directory tree for the file picker; per-buffer watching tracks individual files or their parent directories
2. **Different lifecycle**: `FileIndex` lives as long as the workspace; buffer watchers are created/destroyed as tabs open/close
3. **Minimal overlap**: Files inside the workspace are already covered by `FileIndex`; we only need to watch files outside it

**Key Design Points:**

- The `BufferFileWatcher` lives in `EditorState` (parallel to `FileChangeSuppression`)
- When a file is opened via `associate_file()`, we check if it's outside the workspace root
- If outside, we register a watch on the file (or its parent directory for efficiency)
- The watcher sends `FileChanged` events through the existing event channel (same path as `FileIndex`)
- When a tab is closed, we unregister its watcher (only if no other tabs share that watch)
- The same debouncing and self-write suppression from `file_change_events` applies automatically (they operate on the event channel, not the watcher source)

**Testing Strategy (per TESTING_PHILOSOPHY.md):**

- Unit tests for the `BufferFileWatcher` state machine (add/remove/check logic)
- Integration tests (marked `#[ignore]`) for actual filesystem event flow
- The humble view architecture means we test the watcher logic without needing a window or GPU

## Sequence

### Step 1: Create the `BufferFileWatcher` module

Create a new module `crates/editor/src/buffer_file_watcher.rs` that manages per-buffer file watchers.

**Core struct:**

```rust
// Chunk: docs/chunks/buffer_file_watching - Per-buffer file watching
//!
//! Per-buffer file watching for files outside the workspace.
//!
//! Files opened from outside the workspace root (e.g., via Cmd+O navigating
//! to an external directory) are not covered by the FileIndex watcher. This
//! module manages individual file watchers for such files, sending FileChanged
//! events through the same event channel.

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify::event::{DataChange, ModifyKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::file_change_debouncer::FileChangeDebouncer;

/// Type alias for the file change callback (same as FileIndex).
pub type FileChangeCallback = Box<dyn Fn(PathBuf) + Send + Sync>;

/// Manages file watchers for buffers opened from outside the workspace.
///
/// Each unique external file (or its parent directory) gets a watcher. When
/// the watcher detects changes, it invokes the callback (after debouncing).
/// Watchers are reference-counted: closing a tab only removes the watcher if
/// no other tabs share that watch.
pub struct BufferFileWatcher {
    /// Map from watched path to (watcher, ref_count, stop_sender)
    /// The watched path may be the file itself or its parent directory.
    watchers: HashMap<PathBuf, WatchEntry>,
    /// Map from file path to watched path (for ref-counting)
    file_to_watch: HashMap<PathBuf, PathBuf>,
    /// Callback for file changes
    on_change: Option<Arc<FileChangeCallback>>,
    /// Workspace root (files inside this are not watched here)
    workspace_root: Option<PathBuf>,
}

struct WatchEntry {
    /// The watcher instance
    _watcher: RecommendedWatcher,
    /// Number of files using this watcher
    ref_count: usize,
    /// Set of target files being watched in this directory
    target_files: Arc<std::sync::Mutex<HashSet<PathBuf>>>,
    /// Channel to stop the watcher thread
    _stop_tx: Sender<()>,
    /// Handle to the watcher thread
    _thread_handle: Option<JoinHandle<()>>,
}
```

**Key methods:**

- `new()` / `new_with_callback()`: Create the watcher manager
- `set_workspace_root(&mut self, root: PathBuf)`: Set the workspace root for filtering
- `register(&mut self, path: &Path) -> std::io::Result<()>`: Register a watch for an external file
- `unregister(&mut self, path: &Path)`: Remove a watch (decrement ref count)
- `is_external(&self, path: &Path) -> bool`: Check if a path is outside the workspace

Location: `crates/editor/src/buffer_file_watcher.rs`

### Step 2: Implement the `is_external` check

```rust
impl BufferFileWatcher {
    /// Returns true if the path is outside the workspace root.
    ///
    /// If no workspace root is set, all paths are considered external.
    /// Paths inside the workspace are already watched by FileIndex.
    pub fn is_external(&self, path: &Path) -> bool {
        match &self.workspace_root {
            Some(root) => !path.starts_with(root),
            None => true, // No workspace = all files are "external"
        }
    }
}
```

This helper is used by `associate_file()` to decide whether to register a watch.

### Step 3: Implement `register` with watcher creation

When registering a file:
1. Check if it's already registered (increment ref count)
2. If not, create a new `RecommendedWatcher` for the file's parent directory
3. Start a thread to process events with debouncing
4. Track the file → watch path mapping

**Why watch the parent directory?**

Watching individual files is fragile on some platforms (FSEvents on macOS prefers directories). Watching the parent directory and filtering events for our file is more reliable. This also handles atomic writes (temp file + rename) correctly.

```rust
/// Register a watch for an external file.
///
/// If the file is inside the workspace root, this is a no-op (FileIndex handles it).
/// If the file is already registered, increments the reference count.
/// Otherwise, creates a new watcher for the file's parent directory.
pub fn register(&mut self, path: &Path) -> std::io::Result<()> {
    // Skip workspace-internal files
    if !self.is_external(path) {
        return Ok(());
    }

    let path = path.to_path_buf();
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

    // Already registered?
    if let Some(watch_path) = self.file_to_watch.get(&canonical) {
        if let Some(entry) = self.watchers.get_mut(watch_path) {
            entry.ref_count += 1;
            return Ok(());
        }
    }

    // Determine watch path (parent directory)
    let watch_path = canonical.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| canonical.clone());

    // If we already have a watcher for this directory, reuse it
    if let Some(entry) = self.watchers.get_mut(&watch_path) {
        entry.ref_count += 1;
        entry.target_files.lock().unwrap().insert(canonical.clone());
        self.file_to_watch.insert(canonical, watch_path);
        return Ok(());
    }

    // Create new watcher
    // ... (watcher setup code, see Step 4)
}
```

### Step 4: Implement watcher thread with debouncing and filtering

The watcher thread:
1. Receives events from the `notify` watcher
2. Filters for `Modify(Data(Content))` events on the target file
3. Applies debouncing (reuses `FileChangeDebouncer`)
4. Invokes the callback for ready events

```rust
fn spawn_watcher_thread(
    target_files: Arc<std::sync::Mutex<HashSet<PathBuf>>>,
    on_change: Arc<FileChangeCallback>,
    event_rx: Receiver<Result<Event, notify::Error>>,
    stop_rx: Receiver<()>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut debouncer = FileChangeDebouncer::with_default();

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            match event_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    // Filter for content changes on our target files
                    if matches!(event.kind, EventKind::Modify(ModifyKind::Data(_))) {
                        let targets = target_files.lock().unwrap();
                        for path in &event.paths {
                            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                            if targets.contains(&canonical) {
                                debouncer.register(canonical, Instant::now());
                            }
                        }
                    }
                }
                Ok(Err(_)) => {} // Watcher error, ignore
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Flush ready events
            let now = Instant::now();
            for path in debouncer.flush_ready(now) {
                on_change(path);
            }
        }
    })
}
```

### Step 5: Implement `unregister` with reference counting

```rust
/// Unregister a watch for an external file.
///
/// Decrements the reference count for the file's watch. When the count
/// reaches zero, the watcher is stopped and removed.
pub fn unregister(&mut self, path: &Path) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Find the watch path
    let watch_path = match self.file_to_watch.get(&canonical) {
        Some(wp) => wp.clone(),
        None => return, // Not registered
    };

    // Decrement ref count and remove from target files
    let should_remove = if let Some(entry) = self.watchers.get_mut(&watch_path) {
        entry.target_files.lock().unwrap().remove(&canonical);
        entry.ref_count -= 1;
        entry.ref_count == 0
    } else {
        false
    };

    // Remove watch if no more references
    if should_remove {
        self.watchers.remove(&watch_path);
        // WatchEntry's Drop will signal the thread to stop
    }

    self.file_to_watch.remove(&canonical);
}
```

### Step 6: Add `BufferFileWatcher` to `EditorState`

Add the watcher to `EditorState`:

```rust
// In EditorState struct:
/// Per-buffer file watcher for files outside the workspace.
/// Manages watchers for files opened via Cmd+O from external directories.
// Chunk: docs/chunks/buffer_file_watching - Per-buffer file watching
buffer_file_watcher: BufferFileWatcher,
```

Initialize in `EditorState::new()`:
- Create `BufferFileWatcher::new()` initially
- The callback will need to be wired up in `EditorController` where we have access to `EventSender`

### Step 7: Wire callback in EditorController

In `EditorController` (or wherever the event sender is configured), set the callback:

```rust
// Chunk: docs/chunks/buffer_file_watching - Wire up file change callback
let event_sender_for_buffer_watcher = event_sender.clone();
state.buffer_file_watcher.set_callback(move |path| {
    let _ = event_sender_for_buffer_watcher.send_file_changed(path);
});
```

Also set the workspace root when a workspace is created/changed:

```rust
// Chunk: docs/chunks/buffer_file_watching - Set workspace root for buffer watcher
if let Some(ws) = state.editor.active_workspace() {
    state.buffer_file_watcher.set_workspace_root(ws.root_path.clone());
}
```

### Step 8: Wire up watcher in `associate_file`

In `EditorState::associate_file()`, after successfully loading/associating a file:

```rust
// After setting associated_file on the tab:
// Chunk: docs/chunks/buffer_file_watching - Register external file watch
if let Err(e) = self.buffer_file_watcher.register(&path) {
    // Log but don't fail - watching is a nice-to-have, not critical
    eprintln!("Failed to watch external file {:?}: {}", path, e);
}
```

This is safe for workspace-internal files because `register()` checks `is_external()` first.

### Step 9: Wire up watcher cleanup in `close_tab`

When a tab is closed, unregister its watch. Find the tab closing logic and add:

```rust
// In close_tab logic, after removing the tab:
if let Some(ref path) = removed_tab.associated_file {
    // Chunk: docs/chunks/buffer_file_watching - Unregister external file watch
    self.buffer_file_watcher.unregister(path);
}
```

### Step 10: Add unit tests for `BufferFileWatcher`

Test the state management logic without filesystem events:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_external_without_root() {
        let watcher = BufferFileWatcher::new();
        // No root = all paths are external
        assert!(watcher.is_external(Path::new("/etc/hosts")));
    }

    #[test]
    fn test_is_external_with_root() {
        let mut watcher = BufferFileWatcher::new();
        watcher.set_workspace_root(PathBuf::from("/home/user/project"));

        assert!(watcher.is_external(Path::new("/etc/hosts")));
        assert!(watcher.is_external(Path::new("/home/user/other/file.rs")));
        assert!(!watcher.is_external(Path::new("/home/user/project/src/main.rs")));
    }

    #[test]
    fn test_register_internal_file_is_noop() {
        let mut watcher = BufferFileWatcher::new();
        watcher.set_workspace_root(PathBuf::from("/home/user/project"));

        // Internal file should not create a watch
        let result = watcher.register(Path::new("/home/user/project/src/main.rs"));
        assert!(result.is_ok());
        // No watchers created (internal file)
    }

    #[test]
    fn test_unregister_unknown_path_is_noop() {
        let mut watcher = BufferFileWatcher::new();
        // Should not panic
        watcher.unregister(Path::new("/nonexistent/path.rs"));
    }
}
```

### Step 11: Add integration test for external file watching

Create an integration test (marked `#[ignore]`) that:

1. Creates a temp directory as "external" (outside mock workspace)
2. Creates a `BufferFileWatcher` with a test callback
3. Registers a file in the temp directory
4. Modifies the file externally
5. Waits for debounce + watcher latency
6. Verifies the callback was invoked with the correct path

```rust
#[test]
#[ignore] // Timing-sensitive: filesystem events may take time to propagate
fn test_external_file_modification_detected() {
    use tempfile::TempDir;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // Create temp dirs
    let workspace_dir = TempDir::new().unwrap();
    let external_dir = TempDir::new().unwrap();

    // Track callback invocations
    let call_count = Arc::new(AtomicUsize::new(0));
    let received_paths = Arc::new(std::sync::Mutex::new(Vec::<PathBuf>::new()));

    // Create watcher
    let call_count_clone = call_count.clone();
    let received_paths_clone = received_paths.clone();
    let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |path| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        received_paths_clone.lock().unwrap().push(path);
    }));
    watcher.set_workspace_root(workspace_dir.path().to_path_buf());

    // Create and register external file
    let external_file = external_dir.path().join("test.txt");
    fs::write(&external_file, "initial content").unwrap();
    watcher.register(&external_file).unwrap();

    // Wait for watcher to initialize
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Modify the file
    fs::write(&external_file, "modified content").unwrap();

    // Wait for detection (debounce + watcher latency)
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify callback was invoked
    assert!(call_count.load(Ordering::SeqCst) >= 1);
    let paths = received_paths.lock().unwrap();
    assert!(paths.iter().any(|p| p.ends_with("test.txt")));
}
```

### Step 12: Export module and update lib.rs

Add the new module to `crates/editor/src/lib.rs`:

```rust
// Chunk: docs/chunks/buffer_file_watching - Per-buffer file watching module
pub mod buffer_file_watcher;
```

### Step 13: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched:

```yaml
code_paths:
  - crates/editor/src/buffer_file_watcher.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/lib.rs
```

## Dependencies

- **file_change_events** (chunk): The existing `FileChangeDebouncer` and event channel infrastructure. This chunk must be complete.
- **notify** crate: Already in use by `FileIndex` for filesystem watching.

## Risks and Open Questions

1. **Thread safety of callback from multiple watchers**: Each external directory gets its own watcher thread, all invoking the same callback (which sends to the event channel). This is safe because `EventSender` is `Send + Sync` (Arc-wrapped internally).

2. **Canonicalization edge cases**: We canonicalize paths to handle symlinks, but canonicalization can fail for non-existent paths. We fall back to the original path in that case.

3. **Directory watching granularity**: We watch the parent directory rather than the file itself. This means we may receive spurious events for sibling files. The filtering logic rejects events for files we're not tracking.

4. **Workspace root changes**: When the active workspace changes, the workspace root changes. Files that were "external" may now be "internal". We don't migrate watches on workspace change — duplicate FileChanged events are harmless.

5. **Multiple files in same directory**: When two external files share a parent directory, they share a watcher. The reference counting ensures the watcher stays alive until both are closed. The filter set (`target_files`) tracks which files in that directory we care about.

6. **Watcher resource limits**: Each external file's parent directory gets a watcher. If a user opens many files from many different directories, we could exhaust system watcher resources. This is unlikely in practice (FSEvents on macOS is very efficient), but worth monitoring.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
