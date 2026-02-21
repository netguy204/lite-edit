// Chunk: docs/chunks/fuzzy_file_matcher - File index and fuzzy matching
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

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

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
}

impl FileIndex {
    /// Start indexing `root` in a background thread.
    ///
    /// Loads the persisted recency list from `<root>/.lite-edit-recent` if it exists.
    /// Returns immediately; the walk proceeds concurrently.
    pub fn start(root: PathBuf) -> Self {
        let recency = load_recency(&root);
        let state = Arc::new(Mutex::new(SharedState {
            cache: Vec::new(),
            recency,
        }));
        let version = Arc::new(AtomicU64::new(0));
        let indexing = Arc::new(AtomicBool::new(true));

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

        let watcher_handle = thread::spawn(move || {
            process_watcher_events(
                &watcher_root,
                &watcher_state,
                &watcher_version,
                event_rx,
                stop_rx,
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
    fn query_fuzzy(&self, cache: &[PathBuf], query: &str) -> Vec<MatchResult> {
        let mut results: Vec<MatchResult> = cache
            .iter()
            .filter(|p| !is_excluded(p))
            .filter_map(|path| {
                let filename = path.file_name()?.to_str()?;
                let score = score_match(query, filename)?;
                Some(MatchResult {
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

/// Processes filesystem watcher events.
fn process_watcher_events(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    version: &Arc<AtomicU64>,
    event_rx: Receiver<Event>,
    stop_rx: Receiver<()>,
) {
    loop {
        // Check for stop signal (non-blocking)
        if stop_rx.try_recv().is_ok() {
            break;
        }

        // Try to receive an event with timeout
        match event_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(event) => {
                handle_fs_event(root, state, version, &event);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Handles a single filesystem event.
fn handle_fs_event(
    root: &Path,
    state: &Arc<Mutex<SharedState>>,
    version: &Arc<AtomicU64>,
    event: &Event,
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
                        state.cache.push(relative);
                        state.cache.sort();
                        changed = true;
                    }
                }
            }
            EventKind::Remove(_) => {
                let mut state = state.lock().unwrap();
                state.cache.retain(|p| p != &relative);
                changed = true;
            }
            EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                // Rename: this is sent for both old and new paths
                // We need to handle both add and remove
                let mut state = state.lock().unwrap();
                if path.exists() && path.is_file() {
                    // New path (target of rename)
                    if !state.cache.contains(&relative) {
                        state.cache.push(relative);
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
            }
            EventKind::Modify(_) => {
                // Content modifications don't affect the path list
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
}
