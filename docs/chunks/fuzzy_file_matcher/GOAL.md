---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: file_buffer_association
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after: ["delete_to_line_start", "ibeam_cursor"]
---

<!--
╔══════════════════════════════════════════════════════════════════════════════╗
║  DO NOT DELETE THIS COMMENT BLOCK until the chunk complete command is run.   ║
║                                                                              ║
║  AGENT INSTRUCTIONS: When editing this file, preserve this entire comment    ║
║  block. Only modify the frontmatter YAML and the content sections below      ║
║  (Minor Goal, Success Criteria, Relationship to Parent). Use targeted edits  ║
║  that replace specific sections rather than rewriting the entire file.       ║
╚══════════════════════════════════════════════════════════════════════════════╝

This comment describes schema information that needs to be adhered
to throughout the process.

STATUS VALUES:
- FUTURE: This chunk is queued for future work and not yet being implemented
- IMPLEMENTING: This chunk is in the process of being implemented.
- ACTIVE: This chunk accurately describes current or recently-merged work
- SUPERSEDED: Another chunk has modified the code this chunk governed
- HISTORICAL: Significant drift; kept for archaeology only

FUTURE CHUNK APPROVAL REQUIREMENT:
ALL FUTURE chunks require operator approval before committing or injecting.
After refining this GOAL.md, you MUST present it to the operator and wait for
explicit approval. Do NOT commit or inject until the operator approves.
This applies whether triggered by "in the background", "create a future chunk",
or any other mechanism that creates a FUTURE chunk.

COMMIT BOTH FILES: When committing a FUTURE chunk after approval, add the entire
chunk directory (both GOAL.md and PLAN.md) to the commit, not just GOAL.md. The
`ve chunk create` command creates both files, and leaving PLAN.md untracked will
cause merge conflicts when the orchestrator creates a worktree for the PLAN phase.

PARENT_CHUNK:
- null for new work
- chunk directory name (e.g., "006-segment-compaction") for corrections or modifications

CODE_PATHS:
- Populated at planning time
- List files you expect to create or modify
- Example: ["src/segment/writer.rs", "src/segment/format.rs"]

CODE_REFERENCES:
- Populated after implementation, before PR
- Uses symbolic references to identify code locations

- Format: {file_path}#{symbol_path} where symbol_path uses :: as nesting separator
- Example:
  code_references:
    - ref: src/segment/writer.rs#SegmentWriter
      implements: "Core write loop and buffer management"
    - ref: src/segment/writer.rs#SegmentWriter::fsync
      implements: "Durability guarantees"
    - ref: src/utils.py#validate_input
      implements: "Input validation logic"


NARRATIVE:
- If this chunk was derived from a narrative document, reference the narrative directory name.
- When setting this field during /chunk-create, also update the narrative's OVERVIEW.md
  frontmatter to add this chunk to its `chunks` array with the prompt and chunk_directory.
- If this is the final chunk of a narrative, the narrative status should be set to COMPLETED
  when this chunk is completed.

INVESTIGATION:
- If this chunk was derived from an investigation's proposed_chunks, reference the investigation
  directory name (e.g., "memory_leak" for docs/investigations/memory_leak/).
- This provides traceability from implementation work back to exploratory findings.
- When implementing, read the referenced investigation's OVERVIEW.md for context on findings,
  hypotheses tested, and decisions made during exploration.
- Validated by `ve chunk validate` to ensure referenced investigations exist.


SUBSYSTEMS:
- Optional list of subsystem references that this chunk relates to
- Format: subsystem_id is the subsystem directory name, relationship is "implements" or "uses"
- "implements": This chunk directly implements part of the subsystem's functionality
- "uses": This chunk depends on or uses the subsystem's functionality
- Example:
  subsystems:
    - subsystem_id: "validation"
      relationship: implements
    - subsystem_id: "frontmatter"
      relationship: uses
- Validated by `ve chunk validate` to ensure referenced subsystems exist
- When a chunk that implements a subsystem is completed, a reference should be added to
  that chunk in the subsystems OVERVIEW.md file front matter and relevant section.

FRICTION_ENTRIES:
- Optional list of friction entries that this chunk addresses
- Provides "why did we do this work?" traceability from implementation back to accumulated pain points
- Format: entry_id is the friction entry ID (e.g., "F001"), scope is "full" or "partial"
  - "full": This chunk fully resolves the friction entry
  - "partial": This chunk partially addresses the friction entry
- When to populate: During /chunk-create if this chunk addresses known friction from FRICTION.md
- Example:
  friction_entries:
    - entry_id: F001
      scope: full
    - entry_id: F003
      scope: partial
- Validated by `ve chunk validate` to ensure referenced friction entries exist in FRICTION.md
- When a chunk addresses friction entries and is completed, those entries are considered RESOLVED

BUG_TYPE:
- Optional field for bug fix chunks that guides agent behavior at completion
- Values: semantic | implementation | null (for non-bug chunks)
  - "semantic": The bug revealed new understanding of intended behavior
    - Code backreferences REQUIRED (the fix adds to code understanding)
    - On completion, search for other chunks that may need updating
    - Status → ACTIVE (the chunk asserts ongoing understanding)
  - "implementation": The bug corrected known-wrong code
    - Code backreferences MAY BE SKIPPED (they don't add semantic value)
    - Focus purely on the fix
    - Status → HISTORICAL (point-in-time correction, not an ongoing anchor)
- Leave null for feature chunks and other non-bug work

CHUNK ARTIFACTS:
- Single-use scripts, migration tools, or one-time utilities created for this chunk
  should be stored in the chunk directory (e.g., docs/chunks/foo/migrate.py)
- These artifacts help future archaeologists understand what the chunk did
- Unlike code in src/, chunk artifacts are not expected to be maintained long-term
- Examples: data migration scripts, one-time fixups, analysis tools used during implementation

CREATED_AFTER:
- Auto-populated by `ve chunk create` - DO NOT MODIFY manually
- Lists the "tips" of the chunk DAG at creation time (chunks with no dependents yet)
- Tips must be ACTIVE chunks (shipped work that has been merged)
- Example: created_after: ["auth_refactor", "api_cleanup"]

IMPORTANT - created_after is NOT implementation dependencies:
- created_after tracks CAUSAL ORDERING (what work existed when this chunk was created)
- It does NOT mean "chunks that must be implemented before this one can work"
- FUTURE chunks can NEVER be tips (they haven't shipped yet)

COMMON MISTAKE: Setting created_after to reference FUTURE chunks because they
represent design dependencies. This is WRONG. If chunk B conceptually depends on
chunk A's implementation, but A is still FUTURE, B's created_after should still
reference the current ACTIVE tips, not A.

WHERE TO TRACK IMPLEMENTATION DEPENDENCIES:
- Investigation proposed_chunks ordering (earlier = implement first)
- Narrative chunk sequencing in OVERVIEW.md
- Design documents describing the intended build order
- The `created_after` field will naturally reflect this once chunks ship

DEPENDS_ON:
- Declares explicit implementation dependencies that affect orchestrator scheduling
- Format: list of chunk directory name strings, or null
- Default: [] (empty list - explicitly no dependencies)

VALUE SEMANTICS (how the orchestrator interprets this field):

| Value             | Meaning                              | Oracle behavior   |
|-------------------|--------------------------------------|-------------------|
| `null` or omitted | "I don't know my dependencies"       | Consult oracle    |
| `[]` (empty list) | "I explicitly have no dependencies"  | Bypass oracle     |
| `["chunk_a"]`     | "I depend on these specific chunks"  | Bypass oracle     |

CRITICAL: The default `[]` means "I have analyzed this chunk and it has no dependencies."
This is an explicit assertion, not a placeholder. If you haven't analyzed dependencies yet,
change the value to `null` (or remove the field entirely) to trigger oracle consultation.

WHEN TO USE EACH VALUE:
- Use `[]` when you have analyzed the chunk and determined it has no implementation dependencies
  on other chunks in the same batch. This tells the orchestrator to skip conflict detection.
- Use `null` when you haven't analyzed dependencies yet and want the orchestrator's conflict
  oracle to determine if this chunk conflicts with others.
- Use `["chunk_a", "chunk_b"]` when you know specific chunks must complete before this one.

WHY THIS MATTERS:
The orchestrator's conflict oracle adds latency and cost to detect potential conflicts.
When you declare `[]`, you're asserting independence and enabling the orchestrator to
schedule immediately. When you declare `null`, you're requesting conflict analysis.

PURPOSE AND BEHAVIOR:
- When a list is provided (empty or not), the orchestrator uses it directly for scheduling
- When null, the orchestrator consults its conflict oracle to detect dependencies heuristically
- Dependencies express order within a single injection batch (intra-batch scheduling)
- The chunks listed in depends_on will be scheduled to complete before this chunk starts

CONTRAST WITH created_after:
- `created_after` tracks CAUSAL ORDERING (what work existed when this chunk was created)
- `depends_on` tracks IMPLEMENTATION DEPENDENCIES (what must complete before this chunk runs)
- `created_after` is auto-populated at creation time and should NOT be modified manually
- `depends_on` is agent-populated based on design requirements and may be edited

WHEN TO DECLARE EXPLICIT DEPENDENCIES:
- When you know chunk B requires chunk A's implementation to exist before B can work
- When the conflict oracle would otherwise miss a subtle dependency
- When you want to enforce a specific execution order within a batch injection
- When a narrative or investigation explicitly defines chunk sequencing

EXAMPLE:
  # Chunk has no dependencies (explicit assertion - bypasses oracle)
  depends_on: []

  # Chunk dependencies unknown (triggers oracle consultation)
  depends_on: null

  # Chunk B depends on chunk A completing first
  depends_on: ["auth_api"]

  # Chunk C depends on both A and B completing first
  depends_on: ["auth_api", "auth_client"]

VALIDATION:
- `null` is valid and triggers oracle consultation
- `[]` is valid and means "explicitly no dependencies" (bypasses oracle)
- Referenced chunks should exist in docs/chunks/ (warning if not found)
- Circular dependencies will be detected at injection time
- Dependencies on ACTIVE chunks are allowed (they've already completed)
-->

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
