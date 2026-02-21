---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/file_index.rs
  - crates/editor/src/main.rs
  - crates/editor/Cargo.toml
code_references:
  - ref: crates/editor/src/file_index.rs#FileIndex
    implements: "Core file index struct that manages background directory walk, in-memory path cache, and filesystem watching"
  - ref: crates/editor/src/file_index.rs#FileIndex::start
    implements: "Initializes the index, loads persisted recency, spawns background walker and watcher threads"
  - ref: crates/editor/src/file_index.rs#FileIndex::query
    implements: "Non-blocking fuzzy query against cached paths with recency-first empty query handling"
  - ref: crates/editor/src/file_index.rs#FileIndex::record_selection
    implements: "Records file selection for recency tracking with deduplication and persistence"
  - ref: crates/editor/src/file_index.rs#FileIndex::cache_version
    implements: "Atomic counter for detecting cache changes to enable streaming results"
  - ref: crates/editor/src/file_index.rs#FileIndex::is_indexing
    implements: "Flag indicating whether background walk is still in progress"
  - ref: crates/editor/src/file_index.rs#MatchResult
    implements: "Query result struct with path and score"
  - ref: crates/editor/src/file_index.rs#is_excluded
    implements: "Exclusion rules for dotfiles, target/, and node_modules/"
  - ref: crates/editor/src/file_index.rs#score_match
    implements: "Fuzzy scoring algorithm with consecutive run, prefix, and filename length bonuses"
  - ref: crates/editor/src/file_index.rs#walk_directory
    implements: "Recursive directory walker with batch-based cache updates"
  - ref: crates/editor/src/file_index.rs#process_watcher_events
    implements: "FSEvents consumer thread for live filesystem change detection"
narrative: file_buffer_association
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- delete_to_line_start
- ibeam_cursor
---

# Fuzzy File Matcher

## Minor Goal

Add a `FileIndex` — a stateful, background-threaded file index that recursively walks a root directory, caches every discovered path incrementally, watches the filesystem for changes, and answers queries instantly against the in-memory cache without blocking the main thread. Two behaviours shape the feel of the picker:

- **Empty query shows recency first.** When the user opens the picker without typing, they see the files they have opened most recently — across sessions — at the top, so returning to a recent file is usually one or two keystrokes.
- **Queries stream in during an incomplete walk.** When the walk is still running, the picker re-evaluates the current query against newly-discovered paths automatically, so results accumulate visibly rather than appearing all at once when the walk finishes.

## Success Criteria

### `FileIndex` struct

Lives in a new file (e.g. `crates/editor/src/file_index.rs`). All internal threading and synchronisation is hidden behind its public API.

```rust
pub struct FileIndex { /* opaque */ }

impl FileIndex {
    /// Start indexing `root` in a background thread.
    /// Loads the persisted recency list from `<root>/.lite-edit-recent` if it exists.
    /// Returns immediately; the walk proceeds concurrently.
    pub fn start(root: PathBuf) -> Self;

    /// Score `query` against the current path cache and return results sorted by
    /// descending score. Never blocks — returns whatever has been discovered so far.
    ///
    /// When `query` is empty, recent files are prepended in recency order (most
    /// recent first) before the remaining cached paths (alphabetical).
    pub fn query(&self, query: &str) -> Vec<MatchResult>;

    /// Monotonically increasing counter backed by an AtomicU64. Incremented
    /// whenever the cache changes: a batch of paths added by the walk, a path
    /// added/removed by a filesystem event. The file picker polls this to detect
    /// when it should re-evaluate the current query and refresh the item list.
    pub fn cache_version(&self) -> u64;

    /// True while the initial recursive walk is still running.
    pub fn is_indexing(&self) -> bool;

    /// Record that `path` was just opened by the user. Prepends it to the
    /// in-memory recency list (deduplicating, capped at 50 entries) and
    /// persists the updated list to `<root>/.lite-edit-recent`.
    pub fn record_selection(&self, path: &Path);
}

pub struct MatchResult {
    /// Path relative to the index root.
    pub path: PathBuf,
    pub score: u32,
}
```

### Background walk

- A single dedicated thread (`std::thread::spawn`) walks the root directory recursively, depth-first.
- Paths are pushed into the shared cache in directory-sized batches (lock acquired once per directory, not once per file), then `cache_version` is incremented by 1 for each batch.
- Walk exclusions — skipped entirely along with their subtrees:
  - Any path component starting with `.` (dotfiles / dot-directories).
  - Directories named `target` (Rust build artifacts).
  - Directories named `node_modules`.
- Unreadable directories are skipped silently.
- On completion, the thread sets an `Arc<AtomicBool>` so `is_indexing()` returns `false`.

### File system watching

- After the initial walk, a watcher is started on the root using the [`notify`](https://crates.io/crates/notify) crate (FSEvents on macOS). A dedicated thread consumes watcher events:
  - **Create**: if the path passes exclusion rules, add to cache and increment `cache_version`.
  - **Remove**: remove from cache and increment `cache_version`.
  - **Rename**: remove old path, add new path, increment `cache_version`.
  - **Modify**: no-op (content changes do not affect the path list).
- The watcher handle is owned by `FileIndex` and dropped when it drops.

### Empty query — recency-first ordering

When `query` is empty, `query()` returns:

1. **Recent files** (from the recency list), in most-recent-first order, filtered to paths currently present in the cache. Each is given a high fixed score (e.g. `u32::MAX`) so they always sort above non-recent results.
2. **All other cached paths**, sorted alphabetically, with a uniform score of 1.

The recency list is stored in memory as a `VecDeque<PathBuf>` (relative to root) in an `Arc<Mutex<_>>`. Maximum 50 entries. When `record_selection` is called:

1. Remove any existing occurrence of the path from the list.
2. Prepend the path to the front.
3. Truncate to 50 entries.
4. Persist: overwrite `<root>/.lite-edit-recent` with one relative path per line (UTF-8).

On `FileIndex::start`, attempt to read `<root>/.lite-edit-recent`; if it exists and is readable, populate the initial recency list from it (one path per line, ignoring blank lines and paths that fail basic validation). Missing file is silently ignored.

The `.lite-edit-recent` file starts with `.` and is therefore already excluded from walk and query results by the exclusion rules.

### Typed query — scoring algorithm

Applied against a snapshot of the cache (lock held briefly to clone the `Vec<PathBuf>`, then released before scoring).

- **Match condition**: every character of `query` (lowercased) appears as a subsequence in the **filename component** of the path (lowercased). Paths that do not match are excluded (score 0, not returned).
- **Scoring bonuses** (exact values implementation-defined; relative ordering must satisfy tests):
  - **Consecutive run bonus**: runs of ≥2 consecutively matched characters in the filename contribute a bonus proportional to run length.
  - **Prefix bonus**: matched characters beginning at position 0 of the filename earn a large flat bonus.
  - **Shorter filename bonus**: among equivalent matches, shorter filenames score higher.
- **Result ordering**: descending score; ties broken alphabetically by path.
- Recency is **not** a scoring factor for non-empty queries; scoring is purely textual. The user typed something specific — match it accurately.

### Exclusions double-check in `query()`

Before returning, filter out any result whose relative path has a component starting with `.` or named `target` / `node_modules`, as a guard against walk/watch races.

### Cache version and streaming

`cache_version()` returns the current value of an `Arc<AtomicU64>` that is incremented (with `Relaxed` ordering is sufficient — it is used only for "has anything changed?" polling, not for synchronisation of the cache contents themselves; cache reads go through the `Mutex`).

The file picker (documented in `file_picker`) stores the `cache_version` value at the time of its last `query()` call. On each display-link tick while the picker is open, it compares the stored version against the current `cache_version()`. If the version has advanced, it calls `query()` again with the current query string and refreshes the item list. This makes results stream in naturally during the initial walk and after filesystem events, with no extra threading on the consumer side.

### Rejected Ideas

#### One-shot synchronous walk on each keystroke

Even on a small project a recursive walk takes tens to hundreds of milliseconds. Rejected.

#### Async / Tokio

The editor has no async runtime. Plain `std::thread` with `Arc<Mutex<_>>` is sufficient. Rejected.

#### Full path matching instead of filename-only matching

Scoring against the full relative path surfaces results driven by directory names rather than the file itself. The filename component is the right signal; the full path is shown in the picker for disambiguation. Rejected.

#### Recency as a scoring bonus for non-empty queries

Mixing recency into scored results for a typed query makes the ranking harder to reason about. If a user types `"main"`, they want the best textual match for `"main"`, not a recently-opened file called `"domain.rs"` that happens to score slightly higher. Recency applies only to the empty-query "browse" mode. Rejected.

### Unit tests

- **Empty query, no recency**: returns all cached paths alphabetically.
- **Empty query with recency**: recently-selected files appear first in recency order, followed by the rest alphabetically. A recently-selected path that no longer exists in the cache is omitted.
- **Non-empty query ignores recency**: a recently-selected file that does not match the query does not appear; one that matches scores purely on text.
- **`record_selection` deduplication**: selecting the same file twice results in it appearing only once, at the front.
- **`record_selection` persistence**: after calling `record_selection`, the `.lite-edit-recent` file in the root contains the path; a new `FileIndex::start` on the same root sees it in `query("")` results.
- **`record_selection` cap**: after 51 selections of distinct files, the list contains exactly 50 entries.
- **`cache_version` increments**: version is higher after the walk adds paths than immediately after `start`.
- **Scoring order**: `query("main")` ranks `src/main.rs` above `src/domain.rs`.
- **Consecutive-character bonus**: `query("sr")` ranks `src/lib.rs` above `sensors/data.rs`.
- **Case-insensitivity**: `query("buf")` matches `TextBuffer.rs`.
- **Dotfiles excluded**: `.gitignore` and `.git/config` never appear.
- **`target/` excluded**: `target/debug/editor` never appears.
- **Non-existent root**: `start` does not panic; `query("")` returns empty; `is_indexing()` returns `false` promptly.
- **`is_indexing()` transitions**: `true` immediately after `start()`, eventually `false`.
- **FS watch create**: write a new file, sleep briefly, assert it appears in `query("")`.
- **FS watch remove**: delete a cached file, sleep briefly, assert it no longer appears.
