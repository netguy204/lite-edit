<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements a `FileIndex` that provides instant fuzzy-file matching against an in-memory cache populated by a background directory walk. The design follows these principles:

1. **Threading model**: Two dedicated `std::thread::spawn` threads—one for the initial recursive walk, one for consuming `notify` filesystem events after the walk completes. All shared state is protected by `Arc<Mutex<_>>` for the cache and `Arc<Atomic*>` for simple counters/flags.

2. **Non-blocking queries**: `query()` briefly acquires the cache mutex to clone the path list, then releases it and scores against the local copy. This ensures the picker never blocks on the walk or watcher threads.

3. **Recency persistence**: The recency list is stored in-memory as a `VecDeque<PathBuf>` and persisted to `<root>/.lite-edit-recent` on every selection. This file is read on startup to restore cross-session recency.

4. **TDD approach per TESTING_PHILOSOPHY.md**: Each behavioral contract (scoring, exclusions, recency, cache versioning, watch events) will have failing tests written first, followed by implementation to make them pass. Integration tests for filesystem watching will use `tempfile` crate for isolated directories.

5. **Humble view architecture**: `FileIndex` is pure Rust with no platform dependencies. It does not touch UI, Metal, or macOS APIs. This keeps it fully testable per the project's testing philosophy.

## Sequence

### Step 1: Add dependencies

Add `notify` (filesystem watching) and `tempfile` (test helper) to `crates/editor/Cargo.toml`:

```toml
notify = "6"

[dev-dependencies]
tempfile = "3"
```

Location: `crates/editor/Cargo.toml`

### Step 2: Create file_index module with struct definitions

Create `crates/editor/src/file_index.rs` with the public API surface and internal state:

```rust
// Chunk: docs/chunks/fuzzy_file_matcher - File index and fuzzy matching

pub struct FileIndex { /* ... */ }
pub struct MatchResult { pub path: PathBuf, pub score: u32 }
```

Internal state:
- `Arc<Mutex<Vec<PathBuf>>>` — the path cache (relative paths)
- `Arc<Mutex<VecDeque<PathBuf>>>` — the recency list
- `Arc<AtomicU64>` — cache version counter
- `Arc<AtomicBool>` — indexing complete flag
- `root: PathBuf` — the index root (owned, for persistence path computation)

Expose in `main.rs` via `mod file_index; pub use file_index::FileIndex;`.

Location: `crates/editor/src/file_index.rs`, `crates/editor/src/main.rs`

### Step 3: Implement exclusion rules helper

Write a `fn is_excluded(path: &Path) -> bool` that returns true if:
- Any component starts with `.`
- Any component equals `target` or `node_modules`

Write tests first:
- `.git/config` → excluded
- `.gitignore` → excluded
- `src/main.rs` → not excluded
- `target/debug/editor` → excluded
- `foo/node_modules/bar.js` → excluded

Location: `crates/editor/src/file_index.rs`

### Step 4: Implement recency list persistence

Write helpers:
- `fn recency_path(root: &Path) -> PathBuf` — returns `<root>/.lite-edit-recent`
- `fn load_recency(root: &Path) -> VecDeque<PathBuf>` — reads file, one path per line, ignores blank lines, returns empty if file missing
- `fn save_recency(root: &Path, list: &VecDeque<PathBuf>)` — overwrites file with one path per line

Write tests first:
- Round-trip: save, load, verify content matches
- Missing file returns empty deque
- Blank lines and trailing whitespace ignored

Location: `crates/editor/src/file_index.rs`

### Step 5: Implement `FileIndex::start()` and background walk

Implement `FileIndex::start(root: PathBuf) -> Self`:

1. Load recency list from `<root>/.lite-edit-recent`
2. Initialize shared state (cache, version, indexing flag)
3. Spawn walker thread that:
   - Recursively walks `root` depth-first using `std::fs::read_dir`
   - Skips unreadable directories silently
   - Skips excluded paths (and their subtrees for directories)
   - Batches paths by directory (lock acquired once per directory)
   - Increments `cache_version` after each batch
   - Sets `indexing_complete` flag when done

Write tests first:
- Non-existent root: `start()` does not panic, `query("")` returns empty, `is_indexing()` returns `false` promptly
- `is_indexing()` transitions: true immediately after start, eventually false
- `cache_version` increments: version is higher after walk adds paths than immediately after start

Location: `crates/editor/src/file_index.rs`

### Step 6: Implement `FileIndex::query()` — empty query case

Implement the empty-query path for `query(&self, query: &str) -> Vec<MatchResult>`:

1. Clone the cache under lock
2. Clone the recency list under lock
3. Filter recency list to paths present in cache
4. Prepend recency paths (score = `u32::MAX`) in recency order
5. Append remaining cache paths (score = 1) sorted alphabetically
6. Filter out any excluded paths as a final guard

Write tests first:
- Empty query, no recency: returns all cached paths alphabetically
- Empty query with recency: recently-selected files appear first in recency order, followed by the rest alphabetically
- A recently-selected path that no longer exists in the cache is omitted

Location: `crates/editor/src/file_index.rs`

### Step 7: Implement `FileIndex::query()` — scoring algorithm

Implement the non-empty query path:

1. Clone the cache under lock
2. For each path, extract filename component
3. Match: every character of lowercased query appears as a subsequence in lowercased filename
4. Score matching paths:
   - Base score starts at 1
   - Consecutive run bonus: runs of ≥2 consecutively matched characters add bonus proportional to run length
   - Prefix bonus: matched characters beginning at position 0 earn a large flat bonus
   - Shorter filename bonus: shorter filenames get a small boost
5. Sort descending by score; ties broken alphabetically by path
6. Filter out excluded paths as final guard

Write tests first:
- Scoring order: `query("main")` ranks `src/main.rs` above `src/domain.rs`
- Consecutive-character bonus: `query("sr")` ranks `src/lib.rs` above `sensors/data.rs`
- Case-insensitivity: `query("buf")` matches `TextBuffer.rs`
- Non-empty query ignores recency: a recently-selected file that does not match the query does not appear; one that matches scores purely on text
- Dotfiles excluded: `.gitignore` never appears
- `target/` excluded: `target/debug/editor` never appears

Location: `crates/editor/src/file_index.rs`

### Step 8: Implement `FileIndex::record_selection()`

Implement `record_selection(&self, path: &Path)`:

1. Acquire recency list lock
2. Remove any existing occurrence of `path`
3. Prepend `path` to front
4. Truncate to 50 entries
5. Persist to `<root>/.lite-edit-recent`

Write tests first:
- `record_selection` deduplication: selecting the same file twice results in it appearing only once, at the front
- `record_selection` persistence: after calling `record_selection`, the `.lite-edit-recent` file contains the path; a new `FileIndex::start` on the same root sees it in `query("")` results
- `record_selection` cap: after 51 selections of distinct files, the list contains exactly 50 entries

Location: `crates/editor/src/file_index.rs`

### Step 9: Implement `cache_version()` and `is_indexing()`

Implement the accessor methods:

```rust
pub fn cache_version(&self) -> u64 {
    self.version.load(Ordering::Relaxed)
}

pub fn is_indexing(&self) -> bool {
    !self.indexing_complete.load(Ordering::Relaxed)
}
```

Tests for these are already covered in Step 5. Verify they compile and pass.

Location: `crates/editor/src/file_index.rs`

### Step 10: Implement filesystem watcher

After the walk completes, start a `notify::RecommendedWatcher` on the root. Spawn a thread that consumes watcher events:

- **Create**: if path passes exclusions, add to cache and increment version
- **Remove**: remove from cache and increment version
- **Rename**: remove old path, add new path, increment version
- **Modify**: no-op

The watcher handle is stored in `FileIndex` and dropped when it drops.

Write integration tests (with `tempfile` for isolated directories):
- FS watch create: write a new file, sleep briefly, assert it appears in `query("")`
- FS watch remove: delete a cached file, sleep briefly, assert it no longer appears

Location: `crates/editor/src/file_index.rs`

### Step 11: Register module and verify all tests pass

1. Add `pub mod file_index;` to `crates/editor/src/main.rs`
2. Run `cargo test -p lite-edit` and verify all tests pass
3. Run `cargo clippy -p lite-edit` and fix any warnings

Location: `crates/editor/src/main.rs`

## Dependencies

### External libraries

- **`notify = "6"`**: Filesystem watching crate. Uses FSEvents on macOS for efficient directory monitoring.
- **`tempfile = "3"`** (dev-dependency): Creates isolated temporary directories for filesystem integration tests.

## Risks and Open Questions

1. **Watcher startup timing**: The watcher is started after the walk completes. Events that occur during the walk might be missed if a file is created in an already-walked directory. Mitigation: the exclusion check in `query()` provides a safety net, and real-world usage patterns (user opens picker after project is stable) make this unlikely to cause problems.

2. **Large directory performance**: For very large repositories (e.g., monorepos with 100k+ files), the initial walk may take several seconds. The streaming design mitigates UX impact—results appear progressively. If profiling shows issues, we can add incremental limits.

3. **Cross-platform compatibility**: This chunk targets macOS (per project scope). The `notify` crate abstracts platform differences, but we only test on macOS. If Linux/Windows support is added later, integration tests should be expanded.

4. **File permissions**: Unreadable directories are skipped silently per spec. This is correct behavior but could confuse users who expect to see files they can't access. Documentation or picker UI hints could address this in a future chunk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
