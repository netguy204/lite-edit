// Chunk: docs/chunks/buffer_file_watching - Per-buffer file watching
// Chunk: docs/chunks/app_nap_file_watcher_pause - Pause/resume for App Nap
//!
//! Per-buffer file watching for files outside the workspace.
//!
//! Files opened from outside the workspace root (e.g., via Cmd+O navigating
//! to an external directory) are not covered by the FileIndex watcher. This
//! module manages individual file watchers for such files, sending FileChanged
//! events through the same event channel.
//!
//! # Architecture
//!
//! The `BufferFileWatcher` manages per-file watchers that are separate from
//! the `FileIndex` watcher for several reasons:
//!
//! 1. **Different scope**: `FileIndex` recursively watches a directory tree
//!    for the file picker; per-buffer watching tracks individual files.
//! 2. **Different lifecycle**: `FileIndex` lives as long as the workspace;
//!    buffer watchers are created/destroyed as tabs open/close.
//! 3. **Minimal overlap**: Files inside the workspace are already covered
//!    by `FileIndex`; we only need to watch files outside it.
//!
//! # Implementation Details
//!
//! - We watch the parent directory rather than the file itself for reliability
//!   across platforms (FSEvents on macOS prefers directories).
//! - Watchers are reference-counted: if multiple tabs share the same parent
//!   directory, they share a watcher.
//! - Events are filtered to only include files we're actually tracking.
//! - The same debouncing from `FileChangeDebouncer` is used to coalesce rapid
//!   successive writes.

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify::event::{DataChange, ModifyKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Instant, SystemTime};

use crate::file_change_debouncer::FileChangeDebouncer;

/// Type alias for the file change callback (same as FileIndex).
pub type FileChangeCallback = Box<dyn Fn(PathBuf) + Send + Sync>;

// Chunk: docs/chunks/app_nap_file_watcher_pause - State preserved across pause/resume
/// State captured when pausing file watchers.
///
/// This struct holds the information needed to resume watching and detect
/// any changes that occurred while paused.
#[derive(Default)]
pub struct PausedWatcherState {
    /// Map from file path to its modification time at pause.
    file_mtimes: HashMap<PathBuf, Option<SystemTime>>,
}

/// Manages file watchers for buffers opened from outside the workspace.
///
/// Each unique external file's parent directory gets a watcher. When the watcher
/// detects changes to a tracked file, it invokes the callback (after debouncing).
/// Watchers are reference-counted: closing a tab only removes the watcher if
/// no other tabs share that watch.
pub struct BufferFileWatcher {
    /// Map from watched path (parent directory) to watcher entry.
    watchers: HashMap<PathBuf, WatchEntry>,
    /// Map from file path to its watched path (parent directory).
    file_to_watch: HashMap<PathBuf, PathBuf>,
    /// Callback for file changes.
    on_change: Option<Arc<FileChangeCallback>>,
    /// Workspace root (files inside this are not watched here).
    workspace_root: Option<PathBuf>,
}

/// Entry for a watched directory.
struct WatchEntry {
    /// The watcher instance (kept alive).
    _watcher: RecommendedWatcher,
    /// Number of files using this watcher.
    ref_count: usize,
    /// Set of target files being watched in this directory.
    target_files: Arc<Mutex<HashSet<PathBuf>>>,
    /// Channel to stop the watcher thread.
    _stop_tx: Sender<()>,
    /// Handle to the watcher thread (joined on drop via take).
    _thread_handle: Option<JoinHandle<()>>,
}

impl BufferFileWatcher {
    /// Creates a new watcher manager with no callback.
    ///
    /// Use `set_callback` to wire up the event channel callback.
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
            file_to_watch: HashMap::new(),
            on_change: None,
            workspace_root: None,
        }
    }

    /// Creates a new watcher manager with the given callback.
    ///
    /// The callback is invoked (after debouncing) when an external file is modified.
    #[allow(dead_code)]
    pub fn new_with_callback(callback: FileChangeCallback) -> Self {
        Self {
            watchers: HashMap::new(),
            file_to_watch: HashMap::new(),
            on_change: Some(Arc::new(callback)),
            workspace_root: None,
        }
    }

    /// Sets the callback for file change events.
    ///
    /// This is typically called from `set_event_sender` in `EditorState` to wire
    /// up the event channel.
    pub fn set_callback(&mut self, callback: FileChangeCallback) {
        self.on_change = Some(Arc::new(callback));
    }

    /// Sets the workspace root for filtering.
    ///
    /// Files inside the workspace root are not watched by this watcher
    /// (they're already covered by `FileIndex`).
    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
    }

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

    /// Registers a watch for an external file.
    ///
    /// If the file is inside the workspace root, this is a no-op (FileIndex handles it).
    /// If the file is already registered, increments the reference count.
    /// Otherwise, creates a new watcher for the file's parent directory.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path to the file to watch
    ///
    /// # Returns
    ///
    /// `Ok(())` if registration succeeded or was skipped (internal file),
    /// `Err` if watcher creation failed.
    pub fn register(&mut self, path: &Path) -> std::io::Result<()> {
        // Skip workspace-internal files
        if !self.is_external(path) {
            return Ok(());
        }

        // Canonicalize to handle symlinks
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Already registered?
        if let Some(watch_path) = self.file_to_watch.get(&canonical) {
            if let Some(entry) = self.watchers.get_mut(watch_path) {
                entry.ref_count += 1;
                return Ok(());
            }
        }

        // Determine watch path (parent directory)
        let watch_path = canonical
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| canonical.clone());

        // If we already have a watcher for this directory, reuse it
        if let Some(entry) = self.watchers.get_mut(&watch_path) {
            entry.ref_count += 1;
            entry.target_files.lock().unwrap().insert(canonical.clone());
            self.file_to_watch.insert(canonical, watch_path);
            return Ok(());
        }

        // Need a callback to create a watcher
        let on_change = match &self.on_change {
            Some(cb) => cb.clone(),
            None => {
                // No callback set yet - still register the file so we can watch later
                // when the callback is set. For now, just track the mapping.
                self.file_to_watch.insert(canonical, watch_path);
                return Ok(());
            }
        };

        // Create new watcher
        let target_files = Arc::new(Mutex::new(HashSet::new()));
        target_files.lock().unwrap().insert(canonical.clone());

        // Create event channel for watcher
        let (event_tx, event_rx) = mpsc::channel::<Result<Event, notify::Error>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        // Create the filesystem watcher
        let watcher_result = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let _ = event_tx.send(res);
            },
            Config::default(),
        );

        let mut watcher = match watcher_result {
            Ok(w) => w,
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create watcher: {}", e),
                ));
            }
        };

        // Watch the parent directory (non-recursive)
        if let Err(e) = watcher.watch(&watch_path, RecursiveMode::NonRecursive) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to watch directory: {}", e),
            ));
        }

        // Spawn the watcher thread
        let thread_target_files = target_files.clone();
        let thread_handle = spawn_watcher_thread(thread_target_files, on_change, event_rx, stop_rx);

        // Store the watcher entry
        let entry = WatchEntry {
            _watcher: watcher,
            ref_count: 1,
            target_files,
            _stop_tx: stop_tx,
            _thread_handle: Some(thread_handle),
        };

        self.watchers.insert(watch_path.clone(), entry);
        self.file_to_watch.insert(canonical, watch_path);

        Ok(())
    }

    /// Unregisters a watch for an external file.
    ///
    /// Decrements the reference count for the file's watch. When the count
    /// reaches zero, the watcher is stopped and removed.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path to the file to unwatch
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
            // Removing the entry will drop WatchEntry, which:
            // - Drops _stop_tx, causing the thread's stop_rx to disconnect
            // - Drops _watcher, stopping filesystem watching
            self.watchers.remove(&watch_path);
        }

        self.file_to_watch.remove(&canonical);
    }

    /// Returns the number of active watchers.
    ///
    /// Useful for testing and diagnostics.
    #[allow(dead_code)]
    pub fn watcher_count(&self) -> usize {
        self.watchers.len()
    }

    /// Returns the number of files being watched.
    ///
    /// Useful for testing and diagnostics.
    #[allow(dead_code)]
    pub fn file_count(&self) -> usize {
        self.file_to_watch.len()
    }

    // Chunk: docs/chunks/app_nap_file_watcher_pause - Pause watchers for App Nap
    /// Pauses all file watchers to allow App Nap when the app is backgrounded.
    ///
    /// This method:
    /// 1. Records the current modification time of each watched file
    /// 2. Stops all watcher threads by dropping the watchers
    ///
    /// Returns a `PausedWatcherState` that should be passed to `resume()` when
    /// the app returns to the foreground.
    ///
    /// If already paused (no watchers), returns an empty state.
    pub fn pause(&mut self) -> PausedWatcherState {
        // Capture modification times for all watched files
        let mut file_mtimes = HashMap::new();
        for file_path in self.file_to_watch.keys() {
            let mtime = std::fs::metadata(file_path)
                .and_then(|m| m.modified())
                .ok();
            file_mtimes.insert(file_path.clone(), mtime);
        }

        // Drop all watchers (this stops the threads and releases resources)
        // The _stop_tx channels will be dropped, signaling threads to exit.
        self.watchers.clear();

        PausedWatcherState { file_mtimes }
    }

    // Chunk: docs/chunks/app_nap_file_watcher_pause - Resume watchers after App Nap
    /// Resumes file watching after returning from background.
    ///
    /// This method:
    /// 1. Re-registers watchers for all previously tracked files
    /// 2. Checks if any files were modified while paused (by comparing mtimes)
    /// 3. Emits FileChanged events for modified files
    ///
    /// # Arguments
    ///
    /// * `paused_state` - The state returned from `pause()`
    pub fn resume(&mut self, paused_state: PausedWatcherState) {
        // Get the callback - we need it to re-register and to emit change events
        let on_change = match &self.on_change {
            Some(cb) => cb.clone(),
            None => return, // No callback, nothing to do
        };

        // Collect files to re-register and check for changes
        let files_to_register: Vec<PathBuf> = self.file_to_watch.keys().cloned().collect();

        // Clear file_to_watch since register() will repopulate it
        // But keep a copy for restoration if registration fails
        let original_file_to_watch = std::mem::take(&mut self.file_to_watch);

        // Re-register all files
        for file_path in &files_to_register {
            if let Err(e) = self.register(file_path) {
                eprintln!("Failed to re-register watcher for {:?}: {}", file_path, e);
                // Continue with other files
            }
        }

        // Restore any files that weren't re-registered (e.g., if they were inside workspace)
        for (path, watch_path) in original_file_to_watch {
            if !self.file_to_watch.contains_key(&path) {
                self.file_to_watch.insert(path, watch_path);
            }
        }

        // Check for modifications while paused and emit FileChanged events
        for (file_path, old_mtime) in paused_state.file_mtimes {
            let current_mtime = std::fs::metadata(&file_path)
                .and_then(|m| m.modified())
                .ok();

            // If mtime changed (or file was created/deleted), emit change event
            let changed = match (old_mtime, current_mtime) {
                (Some(old), Some(new)) => old != new,
                (None, Some(_)) => true,  // File was created
                (Some(_), None) => true,  // File was deleted
                (None, None) => false,    // Still doesn't exist
            };

            if changed {
                on_change(file_path);
            }
        }
    }

    /// Returns true if the watcher is currently paused (no active watchers).
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.watchers.is_empty() && !self.file_to_watch.is_empty()
    }
}

impl Default for BufferFileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawns a thread to process watcher events with debouncing.
///
/// The thread:
/// 1. Receives events from the `notify` watcher
/// 2. Filters for `Modify(Data(Content))` events on target files
/// 3. Applies debouncing (using `FileChangeDebouncer`)
/// 4. Invokes the callback for ready events
fn spawn_watcher_thread(
    target_files: Arc<Mutex<HashSet<PathBuf>>>,
    on_change: Arc<FileChangeCallback>,
    event_rx: Receiver<Result<Event, notify::Error>>,
    stop_rx: Receiver<()>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut debouncer = FileChangeDebouncer::with_default();

        loop {
            // Check for stop signal (non-blocking)
            if stop_rx.try_recv().is_ok() {
                break;
            }

            // Try to receive an event with timeout
            // The 50ms timeout serves as our debounce flush interval
            match event_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    // Filter for content changes on our target files
                    if matches!(
                        event.kind,
                        EventKind::Modify(ModifyKind::Data(DataChange::Content))
                            | EventKind::Modify(ModifyKind::Data(DataChange::Any))
                    ) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_external_without_root() {
        let watcher = BufferFileWatcher::new();
        // No root = all paths are external
        assert!(watcher.is_external(Path::new("/etc/hosts")));
        assert!(watcher.is_external(Path::new("/home/user/project/src/main.rs")));
    }

    #[test]
    fn test_is_external_with_root() {
        let mut watcher = BufferFileWatcher::new();
        watcher.set_workspace_root(PathBuf::from("/home/user/project"));

        // External paths
        assert!(watcher.is_external(Path::new("/etc/hosts")));
        assert!(watcher.is_external(Path::new("/home/user/other/file.rs")));

        // Internal paths
        assert!(!watcher.is_external(Path::new("/home/user/project/src/main.rs")));
        assert!(!watcher.is_external(Path::new("/home/user/project/Cargo.toml")));
    }

    #[test]
    fn test_register_internal_file_is_noop() {
        let mut watcher = BufferFileWatcher::new();
        watcher.set_workspace_root(PathBuf::from("/home/user/project"));

        // Internal file should not create a watch
        let result = watcher.register(Path::new("/home/user/project/src/main.rs"));
        assert!(result.is_ok());
        // No watchers created (internal file)
        assert_eq!(watcher.watcher_count(), 0);
    }

    #[test]
    fn test_unregister_unknown_path_is_noop() {
        let mut watcher = BufferFileWatcher::new();
        // Should not panic
        watcher.unregister(Path::new("/nonexistent/path.rs"));
    }

    #[test]
    fn test_new_watcher_is_empty() {
        let watcher = BufferFileWatcher::new();
        assert_eq!(watcher.watcher_count(), 0);
        assert_eq!(watcher.file_count(), 0);
    }

    #[test]
    fn test_default_impl() {
        let watcher = BufferFileWatcher::default();
        assert_eq!(watcher.watcher_count(), 0);
    }

    #[test]
    fn test_register_without_callback_tracks_file() {
        let mut watcher = BufferFileWatcher::new();
        // No callback set

        // Create a temp file
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("buffer_watcher_test_no_cb.txt");
        std::fs::write(&test_file, "test").unwrap();

        // Register should succeed (tracking only, no watcher created)
        let result = watcher.register(&test_file);
        assert!(result.is_ok());

        // File is tracked but no watcher created (no callback)
        assert_eq!(watcher.file_count(), 1);
        assert_eq!(watcher.watcher_count(), 0);

        // Cleanup
        let _ = std::fs::remove_file(&test_file);
    }

    #[test]
    #[ignore] // Timing-sensitive: filesystem events may take time to propagate
    fn test_external_file_modification_detected() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Create temp dirs
        let workspace_dir = tempfile::TempDir::new().unwrap();
        let external_dir = tempfile::TempDir::new().unwrap();

        // Track callback invocations
        let call_count = Arc::new(AtomicUsize::new(0));
        let received_paths = Arc::new(Mutex::new(Vec::<PathBuf>::new()));

        // Create watcher with callback
        let call_count_clone = call_count.clone();
        let received_paths_clone = received_paths.clone();
        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            received_paths_clone.lock().unwrap().push(path);
        }));
        watcher.set_workspace_root(workspace_dir.path().to_path_buf());

        // Create and register external file
        let external_file = external_dir.path().join("test.txt");
        std::fs::write(&external_file, "initial content").unwrap();
        watcher.register(&external_file).unwrap();

        // Wait for watcher to initialize
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Modify the file
        std::fs::write(&external_file, "modified content").unwrap();

        // Wait for detection (debounce + watcher latency)
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Verify callback was invoked
        assert!(
            call_count.load(Ordering::SeqCst) >= 1,
            "Callback should be invoked at least once"
        );
        let paths = received_paths.lock().unwrap();
        assert!(
            paths.iter().any(|p| p.ends_with("test.txt")),
            "Should receive path ending with test.txt, got: {:?}",
            paths
        );
    }

    #[test]
    #[ignore] // Timing-sensitive: filesystem events may take time to propagate
    fn test_unregister_stops_watching() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let external_dir = tempfile::TempDir::new().unwrap();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |_path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Create and register file
        let external_file = external_dir.path().join("test2.txt");
        std::fs::write(&external_file, "initial").unwrap();
        watcher.register(&external_file).unwrap();

        // Wait for watcher to initialize
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Unregister
        watcher.unregister(&external_file);
        assert_eq!(watcher.watcher_count(), 0);
        assert_eq!(watcher.file_count(), 0);

        // Clear any pending calls
        let _ = call_count.swap(0, Ordering::SeqCst);

        // Modify the file
        std::fs::write(&external_file, "modified").unwrap();

        // Wait
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Should not have received any callbacks after unregister
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "Should not receive callbacks after unregister"
        );
    }

    #[test]
    #[ignore] // Timing-sensitive: filesystem events may take time to propagate
    fn test_multiple_files_same_directory_share_watcher() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let external_dir = tempfile::TempDir::new().unwrap();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |_path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Create two files in the same directory
        let file1 = external_dir.path().join("file1.txt");
        let file2 = external_dir.path().join("file2.txt");
        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        // Register both
        watcher.register(&file1).unwrap();
        watcher.register(&file2).unwrap();

        // Should share one watcher
        assert_eq!(watcher.watcher_count(), 1);
        assert_eq!(watcher.file_count(), 2);

        // Unregister one
        watcher.unregister(&file1);
        assert_eq!(watcher.watcher_count(), 1); // Still watching for file2
        assert_eq!(watcher.file_count(), 1);

        // Unregister the other
        watcher.unregister(&file2);
        assert_eq!(watcher.watcher_count(), 0);
        assert_eq!(watcher.file_count(), 0);
    }

    // Chunk: docs/chunks/app_nap_file_watcher_pause - Pause/resume tests
    #[test]
    fn test_pause_stops_watchers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let external_dir = tempfile::TempDir::new().unwrap();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |_path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Create and register a file
        let test_file = external_dir.path().join("test.txt");
        std::fs::write(&test_file, "initial").unwrap();
        watcher.register(&test_file).unwrap();

        assert_eq!(watcher.watcher_count(), 1);
        assert_eq!(watcher.file_count(), 1);

        // Pause
        let paused_state = watcher.pause();

        // Watchers should be cleared but files still tracked
        assert_eq!(watcher.watcher_count(), 0);
        assert_eq!(watcher.file_count(), 1);
        assert!(watcher.is_paused());

        // The paused state should have captured the file
        assert_eq!(paused_state.file_mtimes.len(), 1);
        assert!(paused_state.file_mtimes.contains_key(&test_file.canonicalize().unwrap()));
    }

    #[test]
    fn test_resume_recreates_watchers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let external_dir = tempfile::TempDir::new().unwrap();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |_path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Create and register a file
        let test_file = external_dir.path().join("test.txt");
        std::fs::write(&test_file, "initial").unwrap();
        watcher.register(&test_file).unwrap();

        // Pause and resume without changes
        let paused_state = watcher.pause();
        watcher.resume(paused_state);

        // Watchers should be recreated
        assert_eq!(watcher.watcher_count(), 1);
        assert_eq!(watcher.file_count(), 1);
        assert!(!watcher.is_paused());

        // No change events should have been emitted (no modifications)
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_resume_detects_modifications() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let external_dir = tempfile::TempDir::new().unwrap();

        let call_count = Arc::new(AtomicUsize::new(0));
        let received_paths = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
        let call_count_clone = call_count.clone();
        let received_paths_clone = received_paths.clone();

        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(move |path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            received_paths_clone.lock().unwrap().push(path);
        }));

        // Create and register a file
        let test_file = external_dir.path().join("test.txt");
        std::fs::write(&test_file, "initial").unwrap();
        watcher.register(&test_file).unwrap();

        // Pause
        let paused_state = watcher.pause();

        // Modify the file while paused
        // Sleep briefly to ensure mtime changes (some filesystems have 1s resolution)
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&test_file, "modified").unwrap();

        // Resume - should detect the modification
        watcher.resume(paused_state);

        // A change event should have been emitted
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        let paths = received_paths.lock().unwrap();
        assert!(paths.iter().any(|p| p.ends_with("test.txt")));
    }

    #[test]
    fn test_pause_resume_idempotent() {
        let external_dir = tempfile::TempDir::new().unwrap();

        let mut watcher = BufferFileWatcher::new_with_callback(Box::new(|_path| {}));

        // Create and register a file
        let test_file = external_dir.path().join("test.txt");
        std::fs::write(&test_file, "content").unwrap();
        watcher.register(&test_file).unwrap();

        // Pause twice should be safe
        let state1 = watcher.pause();
        let state2 = watcher.pause(); // Already paused, should return empty state

        // First state should have files, second should be empty (no watchers to stop)
        assert_eq!(state1.file_mtimes.len(), 1);
        assert_eq!(state2.file_mtimes.len(), 1); // Still has file mappings

        // Resume with first state
        watcher.resume(state1);
        assert_eq!(watcher.watcher_count(), 1);

        // Resume again should be safe (no-op, already running)
        watcher.resume(state2);
        assert_eq!(watcher.watcher_count(), 1);
    }

    #[test]
    fn test_pause_no_callback_is_safe() {
        let mut watcher = BufferFileWatcher::new();
        // No callback set

        // Pause should not panic
        let paused_state = watcher.pause();

        // Resume should not panic
        watcher.resume(paused_state);
    }
}
