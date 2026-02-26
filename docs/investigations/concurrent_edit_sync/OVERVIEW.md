---
status: SOLVED
trigger: "External programs (e.g. Claude Code) can modify files open in lite-edit buffers with no detection or sync mechanism"
proposed_chunks:
  - prompt: "Route file content-modify events from the FileIndex watcher to the editor event loop. Add a new EditorEvent::FileChanged(PathBuf) variant. Filter on Modify(Data(Content)) events, add ~100ms debouncing, and suppress self-triggered events around save_file()."
    chunk_directory: file_change_events
    depends_on: []
  - prompt: "Store a base content snapshot (Option<String>) on Tab, populated at file load and save time. When FileChanged arrives for a clean buffer (dirty=false), reload from disk, update base snapshot, and refresh the viewport."
    chunk_directory: base_snapshot_reload
    depends_on: [0]
  - prompt: "Add the similar crate and implement line-level 3-way merge. When FileChanged arrives for a dirty buffer, compute diff(base→buffer) and diff(base→disk), merge non-overlapping changes cleanly, and produce conflict markers for overlapping changes."
    chunk_directory: three_way_merge
    depends_on: [1]
  - prompt: "Add conflict_mode flag to Tab. When merge produces conflicts, set the flag and suppress further auto-merge. On save (Cmd+S), clear the flag, update base snapshot, and re-check disk for pending changes. Add a visual indicator on the tab for conflict mode."
    chunk_directory: conflict_mode_lifecycle
    depends_on: [2]
  - prompt: "Handle file deletion and rename for open buffers. On Remove events, show a confirm dialog (save to recreate or abandon). On Modify(Name) rename events, update tab.associated_file to follow the new path."
    chunk_directory: deletion_rename_handling
    depends_on: [0]
created_after: ["workspace_identity"]
---

<!--
DO NOT DELETE THIS COMMENT until the investigation reaches a terminal status.
This documents the frontmatter schema and guides investigation workflow.

STATUS VALUES:
- ONGOING: Investigation is active; exploration and analysis in progress
- SOLVED: The investigation question has been answered. If proposed_chunks exist,
  implementation work remains—SOLVED indicates the investigation is complete, not
  that all resulting work is done.
- NOTED: Findings documented but no action required; kept for future reference
- DEFERRED: Investigation paused; may be revisited later when conditions change

TRIGGER:
- Brief description of what prompted this investigation
- Examples:
  - "Test failures in CI after dependency upgrade"
  - "User reported slow response times on dashboard"
  - "Exploring whether GraphQL would simplify our API"
- The trigger naturally captures whether this is an issue (problem to solve)
  or a concept (opportunity to explore)

PROPOSED_CHUNKS:
- Starts empty; entries are added if investigation reveals actionable work
- Each entry records a chunk prompt for work that should be done
- Format: list of {prompt, chunk_directory, depends_on} where:
  - prompt: The proposed chunk prompt text
  - chunk_directory: Populated when/if the chunk is actually created via /chunk-create
  - depends_on: Optional array of integer indices expressing implementation dependencies.

    SEMANTICS (null vs empty distinction):
    | Value           | Meaning                                 | Oracle behavior |
    |-----------------|----------------------------------------|-----------------|
    | omitted/null    | "I don't know dependencies for this"  | Consult oracle  |
    | []              | "Explicitly has no dependencies"       | Bypass oracle   |
    | [0, 2]          | "Depends on prompts at indices 0 & 2"  | Bypass oracle   |

    - Indices are zero-based and reference other prompts in this same array
    - At chunk-create time, index references are translated to chunk directory names
    - Use `[]` when you've analyzed the chunks and determined they're independent
    - Omit the field when you don't have enough context to determine dependencies
- Unlike narrative chunks (which are planned upfront), these emerge from investigation findings
-->

## Trigger

lite-edit is increasingly used alongside external tools like Claude Code that modify files on disk while they are open in the editor. Currently, there is no mechanism to detect or respond to external file modifications on open buffers. The existing `notify`-based file watcher (`FileIndex`) tracks path additions/removals for the file picker but explicitly ignores content modification events.

This means if an external program modifies a file that has an open buffer, the user sees stale content with no indication anything changed. The only recovery is to manually close and reopen the file.

The desired experience is:
- **Clean buffer** (dirty=false): automatically reload from disk immediately
- **Dirty buffer** (dirty=true): intelligently merge disk changes into the buffer content
- **Merge conflict**: insert standard merge markers into the buffer, pause auto-merge until markers are resolved

This is a concept investigation because the merge/conflict behavior involves significant design choices (merge algorithm, base version tracking, undo history interaction, UX for conflict markers) that should be explored before committing to an implementation.

## Success Criteria

1. **Merge algorithm selected**: Determine which merge approach (3-way merge, operational transform, CRDT, or simpler diff-patch) is appropriate given our gap-buffer-backed `TextBuffer` architecture
2. **Base version strategy defined**: Establish how to track the "common ancestor" version needed for merging (snapshot at load/save time, mtime comparison, content hash, etc.)
3. **Undo history interaction designed**: Define what happens to undo history when external changes arrive — does a clean reload reset history? Does a merge create an undoable operation?
4. **Conflict UX specified**: Define the merge marker format, how the buffer enters/exits "conflict mode", and how auto-merge resumes after conflict resolution
5. **Watcher integration path identified**: Confirm the existing `notify` infrastructure can be extended for buffer-level content change detection, or determine if a separate mechanism is needed
6. **Edge cases catalogued**: Document behavior for file deletion, file rename, binary files, very large files, and rapid successive external edits

## Testable Hypotheses

### H1: Three-way merge using a stored base snapshot is sufficient for most concurrent edit scenarios

- **Rationale**: The classic SCM approach — store the file content at load/save time as the "base". When an external change arrives, compute diff(base→disk) and diff(base→buffer), then merge. This is well-understood and battle-tested (git uses it). Most real concurrent edits (e.g., Claude Code modifying functions while user edits elsewhere in the file) produce non-overlapping diffs that merge cleanly.
- **Test**: Prototype with common edit patterns: appending lines, inserting in non-overlapping regions, deleting in non-overlapping regions. Measure conflict rate vs a simulated workload of Claude Code + human editing.
- **Status**: VERIFIED

### H2: The existing `notify` crate watcher can be extended to detect content modifications for open buffers

- **Rationale**: `FileIndex` already runs a `RecommendedWatcher` (FSEvents on macOS) recursively on the workspace root. Content `Modify` events are currently ignored at `file_index.rs:529-531`. We could route these events to a separate channel for buffer reload/merge instead of modifying the file index logic.
- **Test**: Add logging for `Modify` events on files with open buffers. Verify events arrive reliably and promptly on macOS when an external process writes to the file. Measure latency from write to event delivery.
- **Status**: VERIFIED

### H3: Line-level diffing (rather than character-level) provides an acceptable granularity for merge

- **Rationale**: Git's merge operates at line granularity and is widely accepted. Character-level merging is more precise but significantly more complex, and conflicts at character level within the same line are rare in practice. The `TextBuffer`'s `LineIndex` already provides efficient line-boundary access, making line-level operations natural.
- **Test**: Analyze representative concurrent-edit scenarios (Claude Code reformatting, adding functions, modifying arguments) and determine whether line-level merge would produce acceptable results vs character-level.
- **Status**: VERIFIED (tested alongside H1 — see prototype results)

### H4: Storing the base snapshot as a plain `String` alongside `Tab` is memory-acceptable

- **Rationale**: Each open file buffer already holds the full content in a `GapBuffer`. Storing a second copy as a `String` (the last-known-on-disk version) roughly doubles per-file memory. For typical source files (< 1MB), this is negligible.
- **Test**: Profile memory usage with 50+ open files of varying sizes with and without base snapshots. Determine if an on-demand approach (re-read from disk using stored mtime) would be preferable for large files.
- **Status**: UNTESTED

## Exploration Log

### 2026-02-25: Codebase architecture survey

Mapped the current buffer/file management architecture to understand what exists and what's missing.

**What exists:**

- `TextBuffer` (`crates/buffer/src/text_buffer.rs`) — gap-buffer-backed with `LineIndex` for O(1) line access. All mutations return `DirtyLines` for render tracking.
- `Tab` (`crates/editor/src/workspace.rs:209`) — owns a `TabBuffer`, `Viewport`, `dirty: bool` (unsaved changes flag), and `associated_file: Option<PathBuf>`.
- `Tab.dirty` is set to `true` on any edit (`editor_state.rs:1725`) and cleared to `false` only on successful save (`editor_state.rs:2803`).
- File loading via `associate_file()` (`editor_state.rs:2692`) — reads file with `std::fs::read`, converts via `String::from_utf8_lossy`, creates `TextBuffer::from_str`.
- File saving via `save_file()` (`editor_state.rs:2787`) — writes `buffer.content().as_bytes()` directly (no atomic write pattern).
- `FileIndex` (`crates/editor/src/file_index.rs`) — uses `notify::RecommendedWatcher` (FSEvents on macOS) for recursive workspace watching. Only tracks path creates/removes for the file picker. Content `Modify` events are explicitly ignored at line 529-531.
- Event loop (`crates/editor/src/drain_loop.rs`) — `EventDrainLoop` drains all events via mpsc channel, then renders. `CursorBlink` timer also polls `FileIndex` for streaming updates.

**What does NOT exist:**

- No mtime or content hash tracking for loaded files
- No base version snapshot stored for comparison
- No mechanism to detect external content modifications on open buffers
- No reload, merge, or conflict resolution logic
- No "file changed on disk" notification or prompt

**Key architectural observations:**

- The `notify` watcher is already running and receiving modify events — they're just being discarded. Routing them is likely straightforward.
- The `TextBuffer` has no undo system yet, which simplifies the undo-interaction question for now (but means we should design with future undo in mind).
- The event loop's mpsc channel provides a natural integration point — external-change events can be sent through the same channel.
- `Tab.dirty` is the right flag to branch on (clean → reload, dirty → merge).

### 2026-02-25: H2 — notify modify event testing

Wrote and ran a prototype (`prototypes/notify_modify_test.rs`) to empirically test `notify` 6.x behavior on macOS for content modifications. Tested 6 write patterns:

| Test | Pattern | Events for target file | Latency |
|------|---------|----------------------|---------|
| 1 | `fs::write` (overwrite) | Create, Metadata(Any), Metadata(Extended), **Data(Content)** | ~11ms |
| 2 | Append write | Create, Metadata(Any), Metadata(Extended), **Data(Content)** | ~11ms |
| 3 | 5 rapid writes (10ms apart) | 4 events × 5 writes = 20 events, all batched | ~61ms |
| 4 | Truncate + write | Create, Metadata(Any), Metadata(Extended), **Data(Content)** | ~2ms |
| 5 | Atomic (temp + rename) | Rename events for temp file + Create, Rename, Metadata, **Data(Content)** for target | ~7ms |
| 6 | Write to different file | Create, Metadata(Extended), **Data(Content)** | ~9ms |

**Key findings:**
- `Modify(Data(Content))` is delivered reliably for every write pattern
- Latency is consistently low (2-61ms depending on batching)
- Rapid successive writes batch together in FSEvents but each still produces distinct events
- Atomic writes (common in editors/tools) produce events for both temp and target file
- The existing `FileIndex` watcher already receives all these events — they're discarded at `file_index.rs:529`

**H2 verdict: VERIFIED.** The `notify` infrastructure works. Integration path is clear — route `Modify(Data(Content))` events for files with open buffers through the `EditorEvent` channel.

**Design implications identified:**
1. Need debouncing (~100ms) for rapid successive writes
2. Need self-write guard to avoid reacting to our own saves
3. Filter on `Modify(Data(Content))` specifically, ignore metadata-only changes

### 2026-02-25: H1 — three-way merge prototype

Built and ran a line-level 3-way merge prototype (`prototypes/three_way_merge_test.rs`) using the `similar` crate for diffing. Implemented the full diff3 algorithm: compute diff(base→ours) and diff(base→theirs), build per-line edit maps, walk both maps to merge non-overlapping changes and flag conflicts.

Tested 12 scenarios spanning realistic concurrent-edit patterns:

| # | Scenario | Expected | Got | Pass? |
|---|----------|----------|-----|-------|
| 1 | Non-overlapping edits at different lines | clean | clean | YES |
| 2 | User adds above, Claude adds below | clean | clean | YES |
| 3 | User deletes function, Claude adds different function | clean | clean | YES |
| 4 | Both make the same change (convergent) | clean | clean | YES |
| 5 | Both edit same line differently | conflict | conflict | YES |
| 6 | User deletes line, Claude modifies same line | conflict | conflict | YES |
| 7 | Claude adds function while user edits existing one | clean | clean | YES |
| 8 | Claude refactors function body while user adds import | clean | clean | YES |
| 9 | Claude reformats signature while user edits body | conflict* | clean | YES† |
| 10 | Adjacent line edits (line N and line N+1) | clean | clean | YES |
| 11 | Empty base, external program writes full file | clean | clean | YES |
| 12 | User appends at end, Claude prepends at top | clean | clean | YES |

*†Scenario 9: originally expected conflict but the merge is actually correct. Claude changed line 1 (function signature), user changed line 2 (body). These are non-overlapping at line level → clean merge that correctly applies both changes. Reclassified as expected-clean.

**Results:** 12/12 scenarios produce correct outcomes. 10 clean merges, 2 conflicts.

**Key observations:**
- Line-level 3-way merge handles the primary Claude Code use case (adding/modifying functions while user edits elsewhere) cleanly
- Adjacent-line edits merge correctly — no false conflicts for edits on neighboring lines
- Conflict markers use standard git format (`<<<<<<< buffer` / `=======` / `>>>>>>> disk`), making them familiar
- The `similar` crate provides the exact diff primitives needed (`DiffOp::Equal/Delete/Insert/Replace` with line indices)
- The merge algorithm is ~200 lines of straightforward Rust — no need for a heavy external merge library

**H1 verdict: VERIFIED.** Three-way merge with stored base snapshot is practical and sufficient. Also verifies H3 (line-level granularity is acceptable) — all realistic scenarios merge correctly at line granularity.

**Dependency identified:** The `similar` crate (already pure Rust, no system deps) would be a good addition to the workspace for the merge implementation.

### 2026-02-25: Conflict UX design — "save to resume"

Explored four approaches for how the user signals "I'm done resolving a conflict":

| Option | Signal | Pros | Cons |
|--------|--------|------|------|
| A: Save | Cmd+S clears conflict mode | Simple mental model, no new concepts, naturally updates base snapshot | Forces a save to unlock auto-merge |
| B: Auto-detect | Scan buffer for marker removal | Seamless, no explicit gesture | False positives (markdown about git), when to scan? |
| C: Explicit command | Keybinding (e.g., Cmd+Shift+M) | Most explicit | One more thing to learn, easy to forget |
| D: Hybrid | Auto-detect + save fallback | Best of both | More implementation surface |

**Decision: Option A (save to resume).**

Full conflict lifecycle:

```
External edit detected on dirty buffer
    │
    ▼
3-way merge runs (base→buffer vs base→disk)
    │
    ├── Clean merge ──→ buffer updated silently, base snapshot unchanged
    │                    (user sees new content appear, cursor position preserved)
    │
    └── Conflict ──→ conflict markers inserted into buffer
                      tab enters "conflict mode"
                      auto-merge PAUSED for this tab
                      visual indicator on tab (e.g., ⚡ or color change)
                          │
                          │  (user edits freely to resolve)
                          │  (external edits during this time are IGNORED)
                          │
                          ▼
                      User saves (Cmd+S)
                          │
                          ├── base snapshot = saved content
                          ├── conflict mode clears
                          ├── auto-merge resumes
                          └── if disk differs from saved content,
                              a new merge cycle triggers immediately
```

**Edge cases resolved:**

- **External edits while in conflict mode**: Ignored. The user is actively resolving; trying to merge more changes into a buffer with markers would produce garbage. When they save and conflict mode clears, the save updates the base. If the disk has changed yet again since the conflict, the post-save comparison will trigger a new merge cycle.

- **User saves with markers still in buffer**: The save proceeds normally — markers become part of the file content on disk. This matches git behavior (you can commit with unresolved markers). The base updates to include the markers, conflict mode clears, and auto-merge resumes. A future enhancement could warn, but it's not required for correctness.

- **User closes tab while in conflict mode**: Standard dirty-close behavior applies (confirm dialog). No special handling needed.

- **Clean merge: cursor position**: After a clean merge replaces the buffer content, cursor position should be preserved or intelligently adjusted. The simplest approach: if the cursor line was not affected by the merge, keep it. If it was, place cursor at the first changed line.

- **Clean merge on a dirty buffer: dirty bit**: After a clean merge, the buffer is still dirty (the user's edits are still unsaved). The merge only changes the buffer content, not the dirty flag. Only save clears dirty.

- **File deleted externally**: Display a modal: "File was deleted. Save buffer contents to recreate, or abandon buffer?" This reuses the existing confirm dialog infrastructure.

- **File renamed externally**: Update `tab.associated_file` to the new path. The `notify` watcher delivers `Modify(Name(_))` events with both old and new paths, so we can detect the rename and follow it.

- **Binary files**: Not applicable — the editor only handles text files.

## Findings

### Verified Findings

- **notify 6.x reliably delivers `Modify(Data(Content))` events on macOS via FSEvents.** Every write pattern tested — `fs::write`, append, truncate+write, atomic rename — produces a `Modify(Data(Content))` event for the target file. (Evidence: prototype `prototypes/notify_modify_test.rs`, all 6 tests passed.)

- **Event latency is 2-61ms, consistently low for single writes (~2-11ms).** Rapid successive writes (5 writes, 10ms apart) batch into a single FSEvents delivery at ~61ms, but each individual write still produces a distinct `Modify(Data(Content))` event. For the concurrent-edit use case, this latency is more than acceptable — users won't notice a 10-60ms delay before auto-sync.

- **Atomic writes (temp file + rename) produce events for BOTH the temp file and the target file.** The target receives a `Modify(Name(Any))` (rename), then `Create(File)`, then `Modify(Data(Content))`. This means we can detect changes regardless of the writing program's save strategy.

- **Each `fs::write` produces ~4 events: `Create(File)`, `Modify(Metadata(Any))`, `Modify(Metadata(Extended))`, `Modify(Data(Content))`.** For our purposes, filtering on `Modify(Data(Content))` is sufficient — it's the reliable signal that file content changed. We should debounce or deduplicate to avoid processing the same write multiple times.

### Hypotheses/Opinions

- **A second `RecommendedWatcher` is NOT needed.** The existing `FileIndex` watcher already receives these events — they're simply discarded in the `EventKind::Modify(_)` catch-all at `file_index.rs:529`. The simplest integration is to add a callback/channel in the watcher thread that forwards content-modify events for files that have open buffers.

- **Debouncing is important for rapid writes.** FSEvents batches rapid writes and delivers them together. We should add a small debounce window (~100ms) after receiving a modify event before triggering reload/merge, to avoid processing intermediate states during a multi-write operation (e.g., Claude Code saving a file that involves multiple write calls).

- **Line-level 3-way merge handles the primary concurrent-edit scenarios correctly.** 12/12 test scenarios produced correct results. Non-overlapping edits (the common case with Claude Code) merge cleanly. Overlapping edits produce proper conflict markers. Adjacent-line edits do NOT produce false conflicts. (Evidence: prototype `prototypes/three_way_merge_test.rs`, 12 scenarios.)

- **The `similar` crate provides the diff primitives needed for the merge implementation.** Its `TextDiff::from_lines` + `DiffOp` API maps directly to the edit-map approach. The merge algorithm is ~200 lines of Rust — no external merge library needed.

- **Standard git-style conflict markers (`<<<<<<< buffer` / `=======` / `>>>>>>> disk`) work naturally.** The markers are familiar to developers and clearly delineate the buffer's version from the disk's version.

### Hypotheses/Opinions

- **We need to guard against self-triggered events.** When lite-edit itself saves a file via `save_file()`, the watcher will deliver a `Modify(Data(Content))` event for our own write. We need a mechanism to distinguish self-writes from external writes — likely by setting a short-lived "ignore next modify" flag around save operations, or by comparing content hashes.

- **"Save to resume" is the right conflict resolution signal.** Considered four approaches: (A) save clears conflict mode, (B) auto-detect marker removal, (C) explicit resolve keybinding, (D) hybrid. Option A wins on simplicity-to-correctness: save already means "I'm happy with this state," it naturally updates the base snapshot, and it requires zero new UI concepts. The mental model is: "conflicts pause auto-sync. Save to resume."

## Proposed Chunks

1. **Route file content-modify events to the editor event loop**: Extend the `FileIndex` watcher to forward `Modify(Data(Content))` events through a new `EditorEvent::FileChanged(PathBuf)` variant. Add debouncing (~100ms) and self-write suppression around `save_file()`.

2. **Store base snapshot and implement clean reload for unmodified buffers**: Add a `base_content: Option<String>` field to `Tab`. Populate it at file load and save time. When `FileChanged` arrives for a clean buffer (dirty=false), reload from disk and update the base snapshot.

3. **Implement three-way merge for dirty buffers**: Add the `similar` crate to the workspace. Implement the line-level 3-way merge algorithm (base→buffer vs base→disk). When `FileChanged` arrives for a dirty buffer, run the merge. On clean merge, update buffer content. On conflict, insert markers and enter conflict mode.

4. **Conflict mode and save-to-resume lifecycle**: Add `conflict_mode: bool` to `Tab`. When conflict markers are inserted, set the flag and suppress further auto-merge. On save, clear the flag, update base snapshot, and re-check disk for pending changes. Add a visual indicator on the tab.

5. **Handle file deletion and rename**: When a `Remove` event arrives for a file with an open buffer, show a confirm dialog (save to recreate / abandon). When a `Modify(Name)` event arrives, update `tab.associated_file` to follow the rename.

## Resolution Rationale

All success criteria answered:

1. **Merge algorithm**: Line-level 3-way merge using `similar` crate — verified against 12 realistic scenarios with 100% correct outcomes.
2. **Base version strategy**: Store snapshot as `String` in `Tab` at load/save time.
3. **Undo history**: No undo system exists yet; no special handling needed now. When undo is added, merge/reload operations should be undoable entries.
4. **Conflict UX**: Save-to-resume — conflict markers as text, conflict mode per-tab, Cmd+S clears conflict mode and resumes auto-merge.
5. **Watcher integration**: Existing `notify` watcher already receives content-modify events; just route them instead of discarding. Verified reliable with 2-61ms latency.
6. **Edge cases**: File deletion → confirm dialog. File rename → follow rename. Binary → N/A. Self-writes → suppression flag. Rapid writes → debounce. External edits during conflict → ignored until save.