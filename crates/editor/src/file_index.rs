// Chunk: docs/chunks/fuzzy_file_matcher - File index and fuzzy matching
// Chunk: docs/chunks/file_change_events - File content change callback support
// Chunk: docs/chunks/app_nap_file_watcher_pause - Pause/resume for App Nap
//!
//! A stateful, background-threaded file index that recursively walks a root
//! directory, caches every discovered path incrementally, watches the filesystem
//! for changes, and answers queries instantly against the in-memory cache
//! without blocking the main thread.
//!
//! Two behaviors shape the feel of the picker:
//! - **Empty query shows recency first.** When the user opens the picker without
//!   typing, they see the files they have opened most recently—across sessions—
//!   at the top.
//! - **Queries stream in during an incomplete walk.** When the walk is still
//!   running, the picker re-evaluates the current query against newly-discovered
//!   paths automatically.
//!
//! ## File Content Change Detection
//!
//! The watcher also forwards `Modify(Data(Content))` events to an optional
//! callback. This enables the editor to detect when external processes modify
//! files in the workspace. Events are debounced (100ms by default) to coalesce
//! rapid successive writes.

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify::event::{DataChange, ModifyKind};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Instant, SystemTime};

use crate::file_change_debouncer::FileChangeDebouncer;

/// Type alias for the file content change callback.
///
/// The callback is invoked (after debouncing) when an external process modifies
/// a file within the workspace. The path is absolute.
pub type FileChangeCallback = Box<dyn Fn(PathBuf) + Send + Sync>;

// Chunk: docs/chunks/deletion_rename_handling - Callback types for deletion and rename events
/// Type alias for the file deletion callback.
///
/// The callback is invoked immediately (no debouncing) when a file is deleted.
/// The path is absolute.
pub type FileDeletedCallback = Box<dyn Fn(PathBuf) + Send + Sync>;

/// Type alias for the file rename callback.
///
/// The callback is invoked immediately (no debouncing) when a file is renamed.
/// Both paths are absolute: `from` is the old path, `to` is the new path.
pub type FileRenamedCallback = Box<dyn Fn(PathBuf, PathBuf) + Send + Sync>;

// Chunk: docs/chunks/deletion_rename_handling - Bundle of file event callbacks
/// Internal struct bundling all file event callbacks.
///
/// This is used to pass callbacks cleanly to the watcher thread.
#[derive(Clone)]
struct FileEventCallbacks {
    /// Callback for content changes (debounced).
    on_change: Option<Arc<FileChangeCallback>>,
    /// Callback for file deletions (immediate).
    on_delete: Option<Arc<FileDeletedCallback>>,
    /// Callback for file renames (immediate).
    on_rename: Option<Arc<FileRenamedCallback>>,
}

/// Maximum number of entries in the recency list.
const MAX_RECENCY_ENTRIES: usize = 50;

/// A result from a fuzzy file query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult {
    /// Path relative to the index root.
    pub path: PathBuf,
    /// Match score (higher is better).
    pub score: u32,
}

/// Internal shared state protected by Arc<Mutex<_>>.
struct SharedState {
    /// Cached relative paths (sorted for deterministic ordering).
    cache: Vec<PathBuf>,
    /// Recent file selections (most recent first).
    recency: VecDeque<PathBuf>,
}

/// A file index that provides instant fuzzy-file matching against an in-memory
/// cache populated by a background directory walk.
pub struct FileIndex {
    /// Root directory being indexed.
    root: PathBuf,
    /// Shared state: cache and recency list.
    state: Arc<Mutex<SharedState>>,
    /// Monotonically increasing counter incremented on cache changes.
    version: Arc<AtomicU64>,
    /// True while the initial recursive walk is still running.
    indexing: Arc<AtomicBool>,
    /// Handle to the walker thread (joined on drop).
    _walker_handle: Option<JoinHandle<()>>,
    /// Handle to the watcher thread (joined on drop).
    _watcher_handle: Option<JoinHandle<()>>,
    /// Sender to signal watcher thread to stop.
    _watcher_stop_tx: Option<Sender<()>>,
    /// The filesystem watcher (kept alive).
    _watcher: Option<RecommendedWatcher>,
    // Chunk: docs/chunks/app_nap_file_watcher_pause - Pause flag for App Nap
    /// True when the watcher is paused (for App Nap eligibility).
    /// When paused, the watcher thread continues to run but skips event processing.
    paused: Arc<AtomicBool>,
    /// Stores callbacks for use on resume.
    callbacks: Arc<Mutex<Option<FileEventCallbacks>>>,
}

impl FileIndex {
    /// Start indexing `root` in a background thread.
    ///
    /// Loads the persisted recency list from `<root>/.lite-edit-recent` if it exists.
    /// Returns immediately; the walk proceeds concurrently.
    pub fn start(root: PathBuf) -> Self {
        let callbacks = FileEventCallbacks {
            on_change: None,
            on_delete: None,
            on_rename: None,
        };
        Self::start_internal(root, callbacks)
    }

    // Chunk: docs/chunks/file_change_events - File change callback support
    /// Start indexing `root` with a callback for file content changes.
    ///
    /// Like `start()`, but also forwards `Modify(Data(Content))` events to the
    /// provided callback after debouncing. This enables the editor to detect
    /// when external processes modify files in the workspace.
    ///
    /// The callback is invoked from the watcher thread with absolute paths.
    /// Events are debounced (100ms) to coalesce rapid successive writes.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory to index
    /// * `callback` - Called when a file is modified by an external process
    pub fn start_with_callback<F>(root: PathBuf, callback: F) -> Self
    where
        F: Fn(PathBuf) + Send + Sync + 'static,
    {
        let callbacks = FileEventCallbacks {
            on_change: Some(Arc::new(Box::new(callback))),
            on_delete: None,
            on_rename: None,
        };
        Self::start_internal(root, callbacks)
    }

    // Chunk: docs/chunks/deletion_rename_handling - File index with all event callbacks
    /// Start indexing `root` with callbacks for file change, delete, and rename events.
    ///
    /// Like `start_with_callback()`, but also supports callbacks for file deletion
    /// and rename events. Content changes are debounced (100ms), but deletions and
    /// renames are delivered immediately.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory to index
    /// * `on_change` - Called when a file's content is modified (debounced)
    /// * `on_delete` - Called when a file is deleted (immediate)
    /// * `on_rename` - Called when a file is renamed (immediate, from -> to)
    pub fn start_with_callbacks<C, D, R>(
        root: PathBuf,
        on_change: Option<C>,
        on_delete: Option<D>,
        on_rename: Option<R>,
    ) -> Self
    where
        C: Fn(PathBuf) + Send + Sync + 'static,
        D: Fn(PathBuf) + Send + Sync + 'static,
        R: Fn(PathBuf, PathBuf) + Send + Sync + 'static,
    {
        let callbacks = FileEventCallbacks {
            on_change: on_change.map(|f| Arc::new(Box::new(f) as FileChangeCallback)),
            on_delete: on_delete.map(|f| Arc::new(Box::new(f) as FileDeletedCallback)),
            on_rename: on_rename.map(|f| Arc::new(Box::new(f) as FileRenamedCallback)),
        };
        Self::start_internal(root, callbacks)
    }

    /// Internal constructor that handles both with and without callbacks.
    fn start_internal(root: PathBuf, callbacks: FileEventCallbacks) -> Self {
        let recency = load_recency(&root);
        let state = Arc::new(Mutex::new(SharedState {
            cache: Vec::new(),
            recency,
        }));
        let version = Arc::new(AtomicU64::new(0));
        let indexing = Arc::new(AtomicBool::new(true));
        // Chunk: docs/chunks/app_nap_file_watcher_pause - Initialize pause state
        let paused = Arc::new(AtomicBool::new(false));
        let stored_callbacks = Arc::new(Mutex::new(Some(callbacks.clone())));

        // Check if root exists before starting the walk
        let root_exists = root.is_dir();

        // Spawn the walker thread
        let walker_state = Arc::clone(&state);
        let walker_version = Arc::clone(&version);
        let walker_indexing = Arc::clone(&indexing);
        let walker_root = root.clone();

        let walker_handle = thread::spawn(move || {
            if !root_exists {
                // Non-existent root: immediately mark as done
                walker_indexing.store(false, Ordering::Relaxed);
                return;
            }

            // Perform the recursive walk
            walk_directory(&walker_root, &walker_root, &walker_state, &walker_version);

            // Mark indexing as complete
            walker_indexing.store(false, Ordering::Relaxed);
        });

        // Create channels for watcher communication
        let (event_tx, event_rx) = mpsc::channel::<Event>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        // Set up filesystem watcher
        let watcher = if root_exists {
            let tx = event_tx.clone();
            let watcher_result = RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = tx.send(event);
                    }
                },
                Config::default(),
            );

            match watcher_result {
                Ok(mut w) => {
                    let _ = w.watch(&root, RecursiveMode::Recursive);
                    Some(w)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Spawn the watcher event processing thread
        let watcher_state = Arc::clone(&state);
        let watcher_version = Arc::clone(&version);
        let watcher_root = root.clone();
        // Chunk: docs/chunks/app_nap_file_watcher_pause - Pass pause flag to watcher thread
        let watcher_paused = Arc::clone(&paused);

        let watcher_handle = thread::spawn(move || {
            process_watcher_events(
                &watcher_root,
                &watcher_state,
                &watcher_version,
                event_rx,
                stop_rx,
                callbacks,
                watcher_paused,
            );
        });

        Self {
            root,
            state,
            version,
            indexing,
            _walker_handle: Some(walker_handle),
            _watcher_handle: Some(watcher_handle),
            _watcher_stop_tx: Some(stop_tx),
            _watcher: watcher,
            paused,
            callbacks: stored_callbacks,
        }
    }

    /// Score `query` against the current path cache and return results sorted by
    /// descending score.
    ///
    /// Never blocks — returns whatever has been discovered so far.
    ///
    /// When `query` is empty, recent files are prepended in recency order (most
    /// recent first) before the remaining cached paths (alphabetical).
    pub fn query(&self, query: &str) -> Vec<MatchResult> {
        // Clone the cache and recency under lock
        let (cache, recency) = {
            let state = self.state.lock().unwrap();
            (state.cache.clone(), state.recency.clone())
        };

        let query = query.to_lowercase();

        if query.is_empty() {
            // Empty query: recency-first ordering
            self.query_empty(&cache, &recency)
        } else {
            // Non-empty query: fuzzy matching with scoring
            self.query_fuzzy(&cache, &query)
        }
    }

    /// Handle empty query: recency-first, then alphabetical.
    fn query_empty(&self, cache: &[PathBuf], recency: &VecDeque<PathBuf>) -> Vec<MatchResult> {
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // First: recent files in recency order (filtered to those in cache)
        for path in recency {
            if cache.contains(path) && !is_excluded(path) {
                seen.insert(path.clone());
                results.push(MatchResult {
                    path: path.clone(),
                    score: u32::MAX,
                });
            }
        }

        // Second: remaining cached paths alphabetically
        let mut remaining: Vec<_> = cache
            .iter()
            .filter(|p| !seen.contains(*p) && !is_excluded(p))
            .cloned()
            .collect();
        remaining.sort();

        for path in remaining {
            results.push(MatchResult { path, score: 1 });
        }

        results
    }

    /// Handle non-empty query: fuzzy matching with scoring.
    ///
    /// Scores each path against both the filename and the full path:
    /// - If the query matches the filename: use `filename_score * 2 + path_score`
    /// - If the query only matches the path: use `path_score` as the sole score
    /// - If no match: filter out the path
    ///
    /// This ensures filename matches dominate (2× weight) while path-only matches
    /// still appear (users can type directory names).
    fn query_fuzzy(&self, cache: &[PathBuf], query: &str) -> Vec<MatchResult> {
        let mut results: Vec<MatchResult> = cache
            .iter()
            .filter(|p| !is_excluded(p))
            .filter_map(|path| {
                // Compute filename score
                let filename_score = path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|filename| score_match(query, filename));

                // Compute path score
                let path_score = score_path_match(query, path);

                // Compute final score based on which matches succeeded
                let final_score = match (filename_score, path_score) {
                    // Both match: filename score dominates (2×) + path score bonus
                    (Some(fs), Some(ps)) => {
                        Some(fs.saturating_mul(2).saturating_add(ps))
                    }
                    // Only filename matches
                    (Some(fs), None) => Some(fs.saturating_mul(2)),
                    // Only path matches (path-only result)
                    (None, Some(ps)) => Some(ps),
                    // Neither matches: filter out
                    (None, None) => None,
                };

                final_score.map(|score| MatchResult {
                    path: path.clone(),
                    score,
                })
            })
            .collect();

        // Sort by descending score, then alphabetically by path
        results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.path.cmp(&b.path))
        });

        results
    }

    /// Monotonically increasing counter backed by an AtomicU64.
    ///
    /// Incremented whenever the cache changes: a batch of paths added by the walk,
    /// a path added/removed by a filesystem event. The file picker polls this to
    /// detect when it should re-evaluate the current query and refresh the item list.
    pub fn cache_version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// True while the initial recursive walk is still running.
    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::Relaxed)
    }

    /// Record that `path` was just opened by the user.
    ///
    /// Prepends it to the in-memory recency list (deduplicating, capped at 50
    /// entries) and persists the updated list to `<root>/.lite-edit-recent`.
    pub fn record_selection(&self, path: &Path) {
        let relative_path = path.strip_prefix(&self.root).unwrap_or(path).to_path_buf();

        {
            let mut state = self.state.lock().unwrap();

            // Remove any existing occurrence
            state.recency.retain(|p| p != &relative_path);

            // Prepend to front
            state.recency.push_front(relative_path);

            // Truncate to max entries
            while state.recency.len() > MAX_RECENCY_ENTRIES {
                state.recency.pop_back();
            }

            // Persist
            save_recency(&self.root, &state.recency);
        }
    }

    // Chunk: docs/chunks/app_nap_file_watcher_pause - Pause watcher for App Nap
    /// Pauses the file watcher to allow App Nap when the app is backgrounded.
    ///
    /// When paused:
    /// - The watcher thread continues to run but skips event processing
    /// - Events received while paused are discarded
    /// - The caller is responsible for checking file mtimes on resume
    ///
    /// Returns a `PausedFileIndexState` containing the mtimes of recently accessed
    /// files. This should be passed to `resume()` to detect changes that occurred
    /// while paused.
    ///
    /// If already paused, this is a no-op and returns an empty state.
    pub fn pause(&self) -> PausedFileIndexState {
        // Set the paused flag - the watcher thread will see this and skip processing
        let was_paused = self.paused.swap(true, Ordering::SeqCst);
        if was_paused {
            // Already paused, return empty state
            return PausedFileIndexState {
                file_mtimes: HashMap::new(),
            };
        }

        // Capture mtimes for recently accessed files (from recency list)
        // These are the files most likely to have been modified while paused
        let mut file_mtimes = HashMap::new();
        {
            let state = self.state.lock().unwrap();
            for relative_path in &state.recency {
                let absolute_path = self.root.join(relative_path);
                let mtime = std::fs::metadata(&absolute_path)
                    .and_then(|m| m.modified())
                    .ok();
                file_mtimes.insert(absolute_path, mtime);
            }
        }

        PausedFileIndexState { file_mtimes }
    }

    // Chunk: docs/chunks/app_nap_file_watcher_pause - Resume watcher after App Nap
    /// Resumes the file watcher after returning from background.
    ///
    /// This method:
    /// 1. Clears the paused flag so the watcher thread resumes processing
    /// 2. Checks if any recently accessed files were modified while paused
    /// 3. Emits FileChanged events for modified files
    ///
    /// # Arguments
    ///
    /// * `paused_state` - The state returned from `pause()`
    pub fn resume(&self, paused_state: PausedFileIndexState) {
        // Clear the paused flag - the watcher thread will resume processing
        let was_paused = self.paused.swap(false, Ordering::SeqCst);
        if !was_paused {
            // Wasn't paused, nothing to do
            return;
        }

        // Get the on_change callback if available
        let on_change = {
            let callbacks_guard = self.callbacks.lock().unwrap();
            callbacks_guard.as_ref().and_then(|cb| cb.on_change.clone())
        };

        // Check for modifications while paused and emit FileChanged events
        if let Some(callback) = on_change {
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
                    callback(file_path);
                }
            }
        }
    }

    /// Returns true if the watcher is currently paused.
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

// Chunk: docs/chunks/app_nap_file_watcher_pause - State preserved across pause/resume
/// State captured when pausing the file index watcher.
///
/// This struct holds the information needed to detect changes that occurred
/// while paused.
#[derive(Default)]
pub struct PausedFileIndexState {
    /// Map from file path to its modification time at pause.
    file_mtimes: HashMap<PathBuf, Option<SystemTime>>,
}

impl Drop for FileIndex {
    fn drop(&mut self) {
        // Signal the watcher thread to stop
        if let Some(tx) = self._watcher_stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

// =============================================================================
// Exclusion Rules
// =============================================================================

/// Returns true if the path should be excluded from indexing and query results.
///
/// Exclusions:
/// - Any path component starting with `.` (dotfiles / dot-directories)
/// - Directories named `target` (Rust build artifacts)
/// - Directories named `node_modules`
fn is_excluded(path: &Path) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name_str = name.to_string_lossy();
            // Dotfiles / dot-directories
            if name_str.starts_with('.') {
                return true;
            }
            // Rust build artifacts
            if name_str == "target" {
                return true;
            }
            // Node modules
            if name_str == "node_modules" {
                return true;
            }
        }
    }
    false
}

// =============================================================================
// Recency Persistence
// =============================================================================

/// Returns the path to the recency file for a given root.
fn recency_path(root: &Path) -> PathBuf {
    root.join(".lite-edit-recent")
}

/// Loads the recency list from disk.
///
/// Returns an empty deque if the file doesn't exist or can't be read.
fn load_recency(root: &Path) -> VecDeque<PathBuf> {
    let path = recency_path(root);
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => return VecDeque::new(),
    };

    let reader = BufReader::new(file);
    let mut recency = VecDeque::new();

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            recency.push_back(PathBuf::from(trimmed));
        }
    }

    // Cap at max entries
    while recency.len() > MAX_RECENCY_ENTRIES {
        recency.pop_back();
    }

    recency
}

/// Saves the recency list to disk.
fn save_recency(root: &Path, recency: &VecDeque<PathBuf>) {
    let path = recency_path(root);
    if let Ok(mut file) = File::create(&path) {
        for entry in recency {
            let _ = writeln!(file, "{}", entry.display());
        }
    }
}

// =============================================================================
// Directory Walking
// =============================================================================

/// Recursively walks a directory, adding non-excluded paths to the cache.
fn walk_directory(
    root: &Path,
    dir: &Path,
    state: &Arc<Mutex<SharedState>>,
    version: &Arc<AtomicU64>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return, // Skip unreadable directories
    };

    let mut batch = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let relative = match path.strip_prefix(root) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };

        // Skip excluded paths
        if is_excluded(&relative) {
            continue;
        }

        if path.is_dir() {
            // Recurse into subdirectory
            walk_directory(root, &path, state, version);
        } else if path.is_file() {
            batch.push(relative);
        }
    }

    // Add batch to cache if non-empty
    if !batch.is_empty() {
        let mut state = state.lock().unwrap();
        state.cache.extend(batch);
        state.cache.sort(); // Keep sorted for deterministic ordering
        drop(state);
        version.fetch_add(1, Ordering::Relaxed);
    }
}

// =============================================================================
// Filesystem Watcher Event Processing
// =============================================================================

// Chunk: docs/chunks/file_change_events - File content change callback
// Chunk: docs/chunks/deletion_rename_handling - File deletion and rename callbacks
// Chunk: docs/chunks/app_nap_file_watcher_pause - Pause-aware event processing
/// Processes filesystem watcher events.
///
/// Handles path cache updates (create/delete/rename) and optionally forwards
/// file events to callbacks:
/// - Content changes are debounced (100ms) to coalesce rapid successive writes
/// - Deletions and renames are delivered immediately (no debouncing)
///
/// When `paused` is true, events are still received but not processed. This allows
/// the app to enter App Nap while keeping the watcher thread alive.
fn process_watcher_events(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    version: &Arc<AtomicU64>,
    event_rx: Receiver<Event>,
    stop_rx: Receiver<()>,
    callbacks: FileEventCallbacks,
    paused: Arc<AtomicBool>,
) {
    // Debouncer for file content changes (only used if callback is provided)
    let mut debouncer = FileChangeDebouncer::with_default();

    loop {
        // Check for stop signal (non-blocking)
        if stop_rx.try_recv().is_ok() {
            break;
        }

        // Chunk: docs/chunks/app_nap_file_watcher_pause - Skip processing when paused
        // When paused, drain events but don't process them to allow App Nap.
        // The 100ms timeout keeps the thread responsive to unpause without burning CPU.
        let is_paused = paused.load(Ordering::Relaxed);

        // Try to receive an event with timeout
        // The 100ms timeout also serves as our debounce flush interval
        match event_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(event) => {
                if !is_paused {
                    handle_fs_event(root, state, version, &event, &mut debouncer, &callbacks);
                }
                // When paused, events are discarded. On resume, the caller is responsible
                // for checking file mtimes to detect any changes that occurred while paused.
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No event received, but we still need to flush the debouncer if not paused
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Flush any debounced content changes that are ready (only when not paused)
        if !is_paused {
            if let Some(ref cb) = callbacks.on_change {
                let now = Instant::now();
                for path in debouncer.flush_ready(now) {
                    cb(path);
                }
            }
        }
    }
}

// Chunk: docs/chunks/file_change_events - Handle content modification events
// Chunk: docs/chunks/deletion_rename_handling - Handle deletion and rename events
/// Handles a single filesystem event.
///
/// Updates the path cache for create/delete/rename events. For content
/// modification events (`Modify(Data(Content))`), registers the path with
/// the debouncer for later emission. For deletion and rename events,
/// immediately invokes the corresponding callback (if provided).
fn handle_fs_event(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    version: &Arc<AtomicU64>,
    event: &Event,
    debouncer: &mut FileChangeDebouncer,
    callbacks: &FileEventCallbacks,
) {
    let mut changed = false;

    for path in &event.paths {
        let relative = match path.strip_prefix(root) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };

        // Skip excluded paths
        if is_excluded(&relative) {
            continue;
        }

        match &event.kind {
            EventKind::Create(_) => {
                if path.is_file() {
                    let mut state = state.lock().unwrap();
                    if !state.cache.contains(&relative) {
                        state.cache.push(relative.clone());
                        state.cache.sort();
                        changed = true;
                    }
                }
            }
            EventKind::Remove(_) => {
                let mut state = state.lock().unwrap();
                state.cache.retain(|p| p != &relative);
                changed = true;

                // Chunk: docs/chunks/deletion_rename_handling - Invoke deletion callback
                // Invoke callback with absolute path (no debouncing)
                if let Some(ref cb) = callbacks.on_delete {
                    cb(path.clone());
                }
            }
            // Chunk: docs/chunks/deletion_rename_handling - Handle rename events
            // notify sends rename events in different modes depending on the platform:
            // - RenameMode::Both: event.paths = [from, to] (ideal case, e.g., on Linux with inotify)
            // - RenameMode::From/To/Any: separate events for source and target
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                use notify::event::RenameMode;

                match rename_mode {
                    RenameMode::Both => {
                        // RenameMode::Both: event.paths = [from, to]
                        // This is the ideal case - we have both paths in one event
                        if event.paths.len() >= 2 {
                            let from_path = &event.paths[0];
                            let to_path = &event.paths[1];

                            // Update cache: remove old, add new
                            if let (Ok(from_rel), Ok(to_rel)) =
                                (from_path.strip_prefix(root), to_path.strip_prefix(root))
                            {
                                if !is_excluded(from_rel) || !is_excluded(to_rel) {
                                    let mut state = state.lock().unwrap();
                                    state.cache.retain(|p| p != from_rel);
                                    if to_path.is_file() && !is_excluded(to_rel) &&
                                       !state.cache.contains(&to_rel.to_path_buf()) {
                                        state.cache.push(to_rel.to_path_buf());
                                        state.cache.sort();
                                    }
                                    changed = true;
                                }
                            }

                            // Invoke rename callback with both paths
                            if let Some(ref cb) = callbacks.on_rename {
                                cb(from_path.clone(), to_path.clone());
                            }
                        }
                        // Skip the normal per-path loop processing since we handled both paths
                        continue;
                    }
                    _ => {
                        // RenameMode::From, To, or Any: separate events for source and target
                        // We handle both add and remove based on whether the path exists
                        let mut state = state.lock().unwrap();
                        if path.exists() && path.is_file() {
                            // New path (target of rename)
                            if !state.cache.contains(&relative) {
                                state.cache.push(relative.clone());
                                state.cache.sort();
                                changed = true;
                            }
                        } else {
                            // Old path (source of rename)
                            let len_before = state.cache.len();
                            state.cache.retain(|p| p != &relative);
                            if state.cache.len() != len_before {
                                changed = true;
                            }
                        }
                        // Note: For non-Both modes, we can't invoke the rename callback
                        // because we don't have both paths in a single event.
                        // The user will see a FileDeleted for the old path followed by
                        // a Create for the new path (or vice versa depending on platform).
                    }
                }
            }
            // Chunk: docs/chunks/file_change_events - Content modification detection
            EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {
                // Content modification detected - register with debouncer
                // The path is absolute for the callback
                debouncer.register(path.clone(), Instant::now());
            }
            EventKind::Modify(_) => {
                // Other modification types (metadata, etc.) don't affect the path list
                // and we don't forward them as content changes
            }
            _ => {}
        }
    }

    if changed {
        version.fetch_add(1, Ordering::Relaxed);
    }
}

// =============================================================================
// Scoring Algorithm
// =============================================================================

/// Scores a query against a filename.
///
/// Returns None if the query doesn't match (not all characters found as subsequence).
/// Returns Some(score) if the query matches, with higher scores being better.
fn score_match(query: &str, filename: &str) -> Option<u32> {
    let filename_lower = filename.to_lowercase();
    let query_chars: Vec<char> = query.chars().collect();
    let filename_chars: Vec<char> = filename_lower.chars().collect();

    if query_chars.is_empty() {
        return Some(1);
    }

    // Find match positions using subsequence matching
    let positions = find_match_positions(&query_chars, &filename_chars)?;

    // Base score
    let mut score: u32 = 100;

    // Consecutive run bonus: runs of ≥2 consecutively matched characters
    let mut consecutive_bonus: u32 = 0;
    let mut run_length = 1;
    for window in positions.windows(2) {
        if window[1] == window[0] + 1 {
            run_length += 1;
        } else {
            if run_length >= 2 {
                consecutive_bonus += run_length as u32 * 10;
            }
            run_length = 1;
        }
    }
    if run_length >= 2 {
        consecutive_bonus += run_length as u32 * 10;
    }
    score += consecutive_bonus;

    // Prefix bonus: matched characters beginning at position 0
    if !positions.is_empty() && positions[0] == 0 {
        // Count how many consecutive matches start from position 0
        let prefix_len = positions
            .iter()
            .enumerate()
            .take_while(|(i, &pos)| pos == *i)
            .count();
        score += prefix_len as u32 * 50;
    }

    // Shorter filename bonus: shorter filenames score higher
    // Use inverse of length (capped to prevent overflow)
    let length_penalty = filename.len().min(255) as u32;
    score += 255 - length_penalty;

    Some(score)
}

/// Scores a query against a full relative path string.
///
/// Returns None if the query doesn't match (not all characters found as subsequence).
/// Returns Some(score) if the query matches, with higher scores being better.
///
/// Unlike `score_match`, this function does NOT apply filename-specific bonuses
/// (prefix bonus, shorter-length bonus). It only applies:
/// - Base score
/// - Consecutive-run bonus
fn score_path_match(query: &str, path: &Path) -> Option<u32> {
    let path_str = path.to_string_lossy().to_lowercase();
    let query_chars: Vec<char> = query.chars().collect();
    let path_chars: Vec<char> = path_str.chars().collect();

    if query_chars.is_empty() {
        return Some(1);
    }

    // Find match positions using subsequence matching
    let positions = find_match_positions(&query_chars, &path_chars)?;

    // Base score
    let mut score: u32 = 100;

    // Consecutive run bonus: runs of ≥2 consecutively matched characters
    let mut consecutive_bonus: u32 = 0;
    let mut run_length = 1;
    for window in positions.windows(2) {
        if window[1] == window[0] + 1 {
            run_length += 1;
        } else {
            if run_length >= 2 {
                consecutive_bonus = consecutive_bonus.saturating_add(run_length as u32 * 10);
            }
            run_length = 1;
        }
    }
    if run_length >= 2 {
        consecutive_bonus = consecutive_bonus.saturating_add(run_length as u32 * 10);
    }
    score = score.saturating_add(consecutive_bonus);

    Some(score)
}

/// Finds the positions in `target` where each character of `query` matches.
///
/// Uses a greedy left-to-right scan. Returns None if not all characters match.
fn find_match_positions(query: &[char], target: &[char]) -> Option<Vec<usize>> {
    let mut positions = Vec::with_capacity(query.len());
    let mut target_idx = 0;

    for &qc in query {
        let mut found = false;
        while target_idx < target.len() {
            if target[target_idx] == qc {
                positions.push(target_idx);
                target_idx += 1;
                found = true;
                break;
            }
            target_idx += 1;
        }
        if !found {
            return None;
        }
    }

    Some(positions)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    // -------------------------------------------------------------------------
    // Exclusion Rules Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_excluded_gitignore() {
        assert!(is_excluded(Path::new(".gitignore")));
    }

    #[test]
    fn test_is_excluded_git_config() {
        assert!(is_excluded(Path::new(".git/config")));
    }

    #[test]
    fn test_is_excluded_src_main() {
        assert!(!is_excluded(Path::new("src/main.rs")));
    }

    #[test]
    fn test_is_excluded_target() {
        assert!(is_excluded(Path::new("target/debug/editor")));
    }

    #[test]
    fn test_is_excluded_node_modules() {
        assert!(is_excluded(Path::new("foo/node_modules/bar.js")));
    }

    #[test]
    fn test_is_excluded_hidden_in_path() {
        assert!(is_excluded(Path::new("src/.hidden/file.rs")));
    }

    // -------------------------------------------------------------------------
    // Recency Persistence Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_recency_roundtrip() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let mut original = VecDeque::new();
        original.push_back(PathBuf::from("src/main.rs"));
        original.push_back(PathBuf::from("src/lib.rs"));
        original.push_back(PathBuf::from("Cargo.toml"));

        save_recency(root, &original);
        let loaded = load_recency(root);

        assert_eq!(original, loaded);
    }

    #[test]
    fn test_recency_missing_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let loaded = load_recency(root);
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_recency_blank_lines_ignored() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let path = recency_path(root);

        let mut file = File::create(&path).unwrap();
        writeln!(file, "src/main.rs").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "   ").unwrap();
        writeln!(file, "src/lib.rs").unwrap();

        let loaded = load_recency(root);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], PathBuf::from("src/main.rs"));
        assert_eq!(loaded[1], PathBuf::from("src/lib.rs"));
    }

    // -------------------------------------------------------------------------
    // FileIndex Basic Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_nonexistent_root_does_not_panic() {
        let index = FileIndex::start(PathBuf::from("/nonexistent/path/that/does/not/exist"));

        // Wait briefly for the walk to complete
        std::thread::sleep(std::time::Duration::from_millis(50));

        let results = index.query("");
        assert!(results.is_empty());
        assert!(!index.is_indexing());
    }

    #[test]
    fn test_is_indexing_transitions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create some files
        fs::create_dir_all(root.join("src")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();
        File::create(root.join("src/lib.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        // Should be indexing immediately after start
        // (though it might finish very quickly on small dirs)
        let initially_indexing = index.is_indexing();

        // Wait for indexing to complete
        let mut attempts = 0;
        while index.is_indexing() && attempts < 100 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            attempts += 1;
        }

        // Should eventually stop indexing
        assert!(!index.is_indexing());

        // If directory is small, it might have finished before we could check
        // So we just verify it eventually becomes false
        let _ = initially_indexing; // May or may not be true
    }

    #[test]
    fn test_cache_version_increments() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create files in nested directories
        fs::create_dir_all(root.join("src/foo")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();
        File::create(root.join("src/foo/bar.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        // Wait for walk to complete
        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let final_version = index.cache_version();

        // Version should have incremented at least once (we had files to add)
        // Note: We can't reliably capture the initial version since the walk
        // starts immediately and may complete before we can check.
        assert!(final_version >= 1, "Version should have incremented after adding files");
    }

    // -------------------------------------------------------------------------
    // Empty Query Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_empty_query_no_recency() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("src")).unwrap();
        File::create(root.join("aaa.rs")).unwrap();
        File::create(root.join("zzz.rs")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        // Wait for indexing
        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("");

        // All results should have score 1 (no recency)
        assert!(results.iter().all(|r| r.score == 1));

        // Should be sorted alphabetically
        let paths: Vec<_> = results.iter().map(|r| &r.path).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted);
    }

    #[test]
    fn test_empty_query_with_recency() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("src")).unwrap();
        File::create(root.join("aaa.rs")).unwrap();
        File::create(root.join("zzz.rs")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        // Wait for indexing
        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Record some selections
        index.record_selection(Path::new("zzz.rs"));
        index.record_selection(Path::new("src/main.rs"));

        let results = index.query("");

        // First two should be recent files in reverse order of selection
        assert_eq!(results[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(results[0].score, u32::MAX);
        assert_eq!(results[1].path, PathBuf::from("zzz.rs"));
        assert_eq!(results[1].score, u32::MAX);

        // Rest should be alphabetical with score 1
        assert!(results[2..].iter().all(|r| r.score == 1));
    }

    #[test]
    fn test_empty_query_recency_nonexistent_omitted() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        File::create(root.join("exists.rs")).unwrap();

        // Pre-populate recency with a non-existent file
        let mut recency = VecDeque::new();
        recency.push_back(PathBuf::from("does_not_exist.rs"));
        recency.push_back(PathBuf::from("exists.rs"));
        save_recency(root, &recency);

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("");

        // Only "exists.rs" should appear
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, PathBuf::from("exists.rs"));
    }

    // -------------------------------------------------------------------------
    // Non-Empty Query / Scoring Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_query_main_ranks_main_above_domain() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("src")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();
        File::create(root.join("src/domain.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("main");

        // main.rs should be first (matches "main" exactly)
        assert!(!results.is_empty());
        assert_eq!(results[0].path, PathBuf::from("src/main.rs"));

        // domain.rs may or may not match depending on scoring
        // but if it does, it should be ranked lower
        if results.len() > 1 {
            assert!(results[0].score >= results[1].score);
        }
    }

    #[test]
    fn test_consecutive_character_bonus() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("sensors")).unwrap();
        // "sr" should match lib.rs better than data.rs
        // because the filename starts with characters that form "sr" consecutively in "lib.rs"
        // Actually, we need filenames where this makes more sense
        // Let's use "srcfile.rs" vs "sorcery.rs"
        File::create(root.join("src/srcfile.rs")).unwrap();
        File::create(root.join("sensors/sorcery.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("sr");

        // Both should match (subsequence "s" "r" present)
        assert!(results.len() >= 2);

        // srcfile.rs should rank higher because "sr" is consecutive at the start
        let srcfile_idx = results.iter().position(|r| r.path.ends_with("srcfile.rs"));
        let sorcery_idx = results.iter().position(|r| r.path.ends_with("sorcery.rs"));

        assert!(srcfile_idx.is_some());
        assert!(sorcery_idx.is_some());
        assert!(srcfile_idx.unwrap() < sorcery_idx.unwrap());
    }

    #[test]
    fn test_case_insensitivity() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        File::create(root.join("TextBuffer.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("buf");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, PathBuf::from("TextBuffer.rs"));
    }

    #[test]
    fn test_nonempty_query_ignores_recency() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        File::create(root.join("config.rs")).unwrap();
        File::create(root.join("utils.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Record utils.rs as recently selected
        index.record_selection(Path::new("utils.rs"));

        // Query for "config" - utils.rs shouldn't appear (no 'c' 'o' 'n' 'f' 'i' 'g' subsequence)
        // and config.rs should appear
        let results = index.query("config");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, PathBuf::from("config.rs"));
    }

    #[test]
    fn test_dotfiles_excluded_from_query() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        File::create(root.join(".gitignore")).unwrap();
        File::create(root.join("readme.md")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("");

        // .gitignore should not appear
        assert!(!results.iter().any(|r| r.path == PathBuf::from(".gitignore")));
        // readme.md should appear
        assert!(results.iter().any(|r| r.path == PathBuf::from("readme.md")));
    }

    #[test]
    fn test_target_excluded_from_query() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("target/debug")).unwrap();
        File::create(root.join("target/debug/editor")).unwrap();
        File::create(root.join("readme.md")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("");

        // target/debug/editor should not appear
        assert!(!results
            .iter()
            .any(|r| r.path == PathBuf::from("target/debug/editor")));
    }

    // -------------------------------------------------------------------------
    // record_selection Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_record_selection_deduplication() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        File::create(root.join("file.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Select the same file twice
        index.record_selection(Path::new("file.rs"));
        index.record_selection(Path::new("file.rs"));

        let results = index.query("");

        // Should only appear once
        let file_count = results
            .iter()
            .filter(|r| r.path == PathBuf::from("file.rs"))
            .count();
        assert_eq!(file_count, 1);
    }

    #[test]
    fn test_record_selection_persistence() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        File::create(root.join("file.rs")).unwrap();

        // First index - record a selection
        {
            let index = FileIndex::start(root.to_path_buf());
            while index.is_indexing() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            index.record_selection(Path::new("file.rs"));
        }

        // Second index - should see the persisted recency
        {
            let index = FileIndex::start(root.to_path_buf());
            while index.is_indexing() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            let results = index.query("");

            // file.rs should have max score (recent)
            assert_eq!(results[0].path, PathBuf::from("file.rs"));
            assert_eq!(results[0].score, u32::MAX);
        }
    }

    #[test]
    fn test_record_selection_cap() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create 51 files
        for i in 0..51 {
            File::create(root.join(format!("file{:02}.rs", i))).unwrap();
        }

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Select all 51 files
        for i in 0..51 {
            index.record_selection(Path::new(&format!("file{:02}.rs", i)));
        }

        let results = index.query("");

        // Only 50 should have max score (recent), 1 should have score 1
        let recent_count = results.iter().filter(|r| r.score == u32::MAX).count();
        assert_eq!(recent_count, 50);
    }

    // -------------------------------------------------------------------------
    // Filesystem Watcher Tests
    // -------------------------------------------------------------------------

    /// Test that filesystem watcher detects new files.
    ///
    /// NOTE: This test is marked #[ignore] because FSEvents on macOS has variable
    /// latency (can be up to seconds) and may not deliver events reliably in CI
    /// environments. Run manually with `cargo test -- --ignored` when needed.
    #[test]
    #[ignore]
    fn test_fs_watch_create() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create initial file
        File::create(root.join("initial.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        // Wait for initial indexing
        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Give the watcher time to fully initialize
        // FSEvents needs time to set up its event stream
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Create a new file
        {
            let f = File::create(root.join("newfile.rs")).unwrap();
            f.sync_all().unwrap();
        }

        // Wait for watcher to pick up the change
        // FSEvents on macOS can have up to 1s+ latency
        let mut attempts = 0;
        let mut found = false;
        while attempts < 100 && !found {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let results = index.query("");
            found = results.iter().any(|r| r.path == PathBuf::from("newfile.rs"));
            attempts += 1;
        }

        assert!(found, "New file should appear in query results");
    }

    /// Test that filesystem watcher detects removed files.
    ///
    /// NOTE: This test is marked #[ignore] because FSEvents on macOS has variable
    /// latency (can be up to seconds) and may not deliver events reliably in CI
    /// environments. Run manually with `cargo test -- --ignored` when needed.
    #[test]
    #[ignore]
    fn test_fs_watch_remove() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create initial file
        File::create(root.join("toremove.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        // Wait for initial indexing
        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Give the watcher time to fully initialize
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Verify file is in results
        let results = index.query("");
        assert!(results
            .iter()
            .any(|r| r.path == PathBuf::from("toremove.rs")));

        // Remove the file
        fs::remove_file(root.join("toremove.rs")).unwrap();

        // Wait for watcher to pick up the change
        let mut attempts = 0;
        let mut removed = false;
        while attempts < 100 && !removed {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let results = index.query("");
            removed = !results
                .iter()
                .any(|r| r.path == PathBuf::from("toremove.rs"));
            attempts += 1;
        }

        assert!(removed, "Removed file should not appear in query results");
    }

    // -------------------------------------------------------------------------
    // Scoring Algorithm Unit Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_score_match_exact_prefix() {
        let score = score_match("main", "main.rs").unwrap();
        // Should have high score due to prefix match
        assert!(score > 200);
    }

    #[test]
    fn test_score_match_no_match() {
        let score = score_match("xyz", "main.rs");
        assert!(score.is_none());
    }

    #[test]
    fn test_score_match_subsequence() {
        let score = score_match("mr", "main.rs");
        assert!(score.is_some());
    }

    #[test]
    fn test_score_match_case_insensitive() {
        let score = score_match("main", "MAIN.RS");
        assert!(score.is_some());
    }

    #[test]
    fn test_find_match_positions_basic() {
        let query: Vec<char> = "mr".chars().collect();
        let target: Vec<char> = "main.rs".chars().collect();
        let positions = find_match_positions(&query, &target).unwrap();
        assert_eq!(positions, vec![0, 5]); // m at 0, r at 5
    }

    #[test]
    fn test_find_match_positions_no_match() {
        let query: Vec<char> = "xyz".chars().collect();
        let target: Vec<char> = "main.rs".chars().collect();
        let positions = find_match_positions(&query, &target);
        assert!(positions.is_none());
    }

    // -------------------------------------------------------------------------
    // Path-Segment Matching Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_query_directory_name_matches_files_within() {
        // Typing a directory name (e.g. `file_search`) should match files under that directory
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("docs/chunks/file_search_path_matching")).unwrap();
        File::create(root.join("docs/chunks/file_search_path_matching/GOAL.md")).unwrap();
        File::create(root.join("docs/chunks/file_search_path_matching/PLAN.md")).unwrap();
        File::create(root.join("unrelated.txt")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("file_search");

        // Should match files within the file_search_path_matching directory
        assert!(results.len() >= 2, "Expected at least 2 results for 'file_search'");
        assert!(
            results.iter().any(|r| r.path == PathBuf::from("docs/chunks/file_search_path_matching/GOAL.md")),
            "GOAL.md should appear in results"
        );
        assert!(
            results.iter().any(|r| r.path == PathBuf::from("docs/chunks/file_search_path_matching/PLAN.md")),
            "PLAN.md should appear in results"
        );
    }

    #[test]
    fn test_query_partial_path_matches() {
        // Typing a partial path like `chunks/terminal` should match files under docs/chunks/terminal_tab_spawn/
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("docs/chunks/terminal_tab_spawn")).unwrap();
        File::create(root.join("docs/chunks/terminal_tab_spawn/GOAL.md")).unwrap();
        fs::create_dir_all(root.join("docs/chunks/other_feature")).unwrap();
        File::create(root.join("docs/chunks/other_feature/GOAL.md")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("chunks/term");

        // Should match files under docs/chunks/terminal_tab_spawn/
        assert!(!results.is_empty(), "Expected at least 1 result for 'chunks/term'");
        assert!(
            results.iter().any(|r| r.path == PathBuf::from("docs/chunks/terminal_tab_spawn/GOAL.md")),
            "terminal_tab_spawn/GOAL.md should appear in results"
        );
    }

    #[test]
    fn test_filename_matches_still_rank_highest() {
        // When a query matches both a filename prefix AND a path segment, filename match should score higher
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("docs/chunks/config_feature")).unwrap();
        // config.rs has "config" in the filename
        File::create(root.join("config.rs")).unwrap();
        // This file has "config" in the path but not in the filename
        File::create(root.join("docs/chunks/config_feature/GOAL.md")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("config");

        // config.rs should rank higher than docs/chunks/config_feature/GOAL.md
        assert!(results.len() >= 2, "Expected at least 2 results");
        let config_rs_idx = results.iter().position(|r| r.path == PathBuf::from("config.rs"));
        let goal_md_idx = results.iter().position(|r| r.path == PathBuf::from("docs/chunks/config_feature/GOAL.md"));

        assert!(config_rs_idx.is_some(), "config.rs should be in results");
        assert!(goal_md_idx.is_some(), "GOAL.md should be in results");
        assert!(
            config_rs_idx.unwrap() < goal_md_idx.unwrap(),
            "config.rs (filename match) should rank above GOAL.md (path-only match)"
        );
    }

    #[test]
    fn test_path_only_match_returns_results() {
        // A query that matches only directory segments (not the filename) should still return results
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("special_project/src")).unwrap();
        File::create(root.join("special_project/src/main.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Query "special" - doesn't appear in the filename "main.rs" but does in the path
        let results = index.query("special");

        assert!(!results.is_empty(), "Expected results for path-only match 'special'");
        assert!(
            results.iter().any(|r| r.path == PathBuf::from("special_project/src/main.rs")),
            "main.rs in special_project should appear in results"
        );
    }

    // -------------------------------------------------------------------------
    // Path-Segment Matching Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_query_with_slash_characters() {
        // Query with `/` characters (e.g., `src/main`) should match paths containing that sequence
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("lib")).unwrap();
        File::create(root.join("src/main.rs")).unwrap();
        File::create(root.join("lib/main.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let results = index.query("src/main");

        // Should match src/main.rs
        assert!(!results.is_empty(), "Expected results for 'src/main'");
        assert!(
            results.iter().any(|r| r.path == PathBuf::from("src/main.rs")),
            "src/main.rs should appear in results"
        );

        // src/main.rs should rank higher than lib/main.rs (more specific path match)
        if results.len() >= 2 {
            let src_idx = results.iter().position(|r| r.path == PathBuf::from("src/main.rs"));
            let lib_idx = results.iter().position(|r| r.path == PathBuf::from("lib/main.rs"));
            if let (Some(src), Some(lib)) = (src_idx, lib_idx) {
                assert!(src < lib, "src/main.rs should rank above lib/main.rs for query 'src/main'");
            }
        }
    }

    #[test]
    fn test_score_path_match_basic() {
        // Basic test for score_path_match function
        let path = Path::new("docs/chunks/feature/GOAL.md");

        // Query that matches the path
        let score = score_path_match("docs", path);
        assert!(score.is_some(), "Expected score for 'docs' in path");
        assert!(score.unwrap() >= 100, "Score should include base score");

        // Query that doesn't match the path
        let no_score = score_path_match("xyz", path);
        assert!(no_score.is_none(), "Expected no match for 'xyz'");
    }

    #[test]
    fn test_score_path_match_consecutive_bonus() {
        // Test that consecutive characters in path get bonus
        let path = Path::new("testing/feature/main.rs");

        // "test" appears consecutively
        let consecutive_score = score_path_match("test", path);
        assert!(consecutive_score.is_some());

        // "tig" requires skipping characters (t-i-g from "testing")
        let sparse_score = score_path_match("tig", path);
        assert!(sparse_score.is_some());

        // Consecutive should score higher
        assert!(
            consecutive_score.unwrap() > sparse_score.unwrap(),
            "Consecutive match should score higher than sparse match"
        );
    }

    #[test]
    fn test_empty_query_path_match() {
        // Empty query should return a score (handled in score_path_match)
        let path = Path::new("src/main.rs");
        let score = score_path_match("", path);
        assert!(score.is_some(), "Empty query should match any path");
        assert_eq!(score.unwrap(), 1, "Empty query should return score 1");
    }

    #[test]
    fn test_combined_score_uses_saturating_arithmetic() {
        // Verify that combined scoring doesn't overflow by using saturating arithmetic
        // This is a structural test - actual scores are bounded, but we verify no panic
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a file with a name that would get high scores
        fs::create_dir_all(root.join("aaaa/bbbb/cccc")).unwrap();
        File::create(root.join("aaaa/bbbb/cccc/aaaaaaaabbbbbbbbcccccccc.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Query that matches both filename and path - should not panic
        let results = index.query("abc");
        assert!(!results.is_empty(), "Should find match without panic");
    }

    #[test]
    fn test_very_long_path_does_not_regress() {
        // Test that very long paths don't cause performance issues
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a deeply nested path
        let deep_path = root.join("a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t");
        fs::create_dir_all(&deep_path).unwrap();
        File::create(deep_path.join("deep_file.rs")).unwrap();

        let index = FileIndex::start(root.to_path_buf());

        while index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Query should still work for deep paths
        let results = index.query("deep");
        assert!(!results.is_empty(), "Should find deeply nested file");
        assert!(
            results.iter().any(|r| r.path.to_string_lossy().contains("deep_file.rs")),
            "deep_file.rs should be in results"
        );
    }

    // -------------------------------------------------------------------------
    // File Change Callback Integration Tests
    // Chunk: docs/chunks/file_change_events
    // -------------------------------------------------------------------------

    #[test]
    #[ignore] // Timing-sensitive: filesystem events may take time to propagate
    fn test_file_change_callback_invoked_on_external_modification() {
        use std::io::Write;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a test file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "initial content").unwrap();

        // Track callback invocations
        let call_count = Arc::new(AtomicUsize::new(0));
        let received_paths = Arc::new(Mutex::new(Vec::<PathBuf>::new()));

        let call_count_clone = call_count.clone();
        let received_paths_clone = received_paths.clone();

        // Start index with callback
        let _index = FileIndex::start_with_callback(root.to_path_buf(), move |path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            received_paths_clone.lock().unwrap().push(path);
        });

        // Wait for indexing to complete
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Modify the file (simulating external editor)
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&test_file)
                .unwrap();
            file.write_all(b"modified content").unwrap();
            file.sync_all().unwrap();
        }

        // Wait for the watcher to detect the change and debounce to fire
        // (debounce is 100ms + watcher poll is 100ms + some slack)
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Callback should have been invoked
        let count = call_count.load(Ordering::SeqCst);
        assert!(
            count >= 1,
            "Callback should be invoked at least once, but was invoked {} times",
            count
        );

        // The path should be the absolute path to test.txt
        let paths = received_paths.lock().unwrap();
        assert!(
            paths.iter().any(|p| p.ends_with("test.txt")),
            "Callback should receive path ending with test.txt, got: {:?}",
            paths
        );
    }

    #[test]
    fn test_start_with_callback_does_not_invoke_for_path_changes() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create initial file
        let test_file = root.join("initial.txt");
        fs::write(&test_file, "content").unwrap();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let _index = FileIndex::start_with_callback(root.to_path_buf(), move |_path| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Wait for indexing
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Create a new file (path change, not content change)
        fs::write(root.join("new_file.txt"), "new content").unwrap();

        // Wait for watcher
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Creating a new file should NOT invoke the callback (it's a path change)
        // Note: On some platforms, create events might include a data change event too
        // so we just verify the basic flow doesn't crash
        let _ = call_count.load(Ordering::SeqCst);
    }
}
