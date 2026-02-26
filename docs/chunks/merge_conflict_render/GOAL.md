---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/merge.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/merge.rs#two_way_merge
    implements: "Fallback merge algorithm for stale/empty base_content - preserves common lines between ours and theirs"
  - ref: crates/editor/src/merge.rs#three_way_merge
    implements: "Empty base detection and fallback trigger - lines 309-311 detect degenerate case and invoke two_way_merge"
  - ref: crates/editor/src/editor_state.rs#EditorState::merge_file_tab
    implements: "Defensive handling when base_content is None - logs warning and uses empty string to trigger two-way fallback"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- buffer_file_watching
- highlight_injection
---

# Chunk Goal

## Minor Goal

Fix the three-way merge so that merge conflicts always produce the full file content with conflict markers only around the conflicting region. Currently, when a merge conflict occurs, the buffer frequently renders only the conflict markers (e.g., `<<<<<<< buffer` / `=======` / `>>>>>>> disk` lines) without the surrounding file content, even though the backing file on disk contains the complete file.

The expected behavior is that the buffer always shows the entire file, with git-style conflict markers inserted only around the lines that actually conflict — identical to what `git merge` produces.

### Suspected Root Cause

The `three_way_merge` function in `crates/editor/src/merge.rs` (called from `merge_file_tab` in `editor_state.rs`) depends on `base_content` being an accurate common ancestor. If `base_content` is stale, empty, or out of sync with the actual file history, the diff from base→ours and base→theirs will treat the *entire* file as changed rather than just the conflicting region. When both sides appear to have inserted all their content (because the base was empty/wrong), the merge produces only conflict markers wrapping everything, with no "Keep" lines to preserve surrounding content.

The intermittent nature (sometimes correct, sometimes only markers) suggests a race condition or timing issue in how `base_content` is captured or updated — possibly related to rapid successive file change events, or `base_content` being set at the wrong point in the reload/merge lifecycle.

### Key Code Paths

- `crates/editor/src/merge.rs` — `three_way_merge()`: the merge algorithm
- `crates/editor/src/editor_state.rs:3599` — `merge_file_tab()`: orchestrates the merge, reads `base_content`, updates buffer
- `crates/editor/src/editor_state.rs:3506` — `reload_file_tab()`: clean reload path, updates `base_content`
- `crates/editor/src/drain_loop.rs:249` — `handle_file_changed()`: routes to reload vs merge

## Success Criteria

- When a three-way merge produces conflicts, the buffer contains the **entire file content** with conflict markers only around the conflicting lines — never just the conflict markers alone
- Add a test in `merge.rs` that explicitly verifies non-conflicting lines are preserved in the merge output (e.g., a 20-line file with a conflict on line 10 should still show all 20+ lines)
- Investigate and fix the `base_content` lifecycle to ensure it always represents a valid common ancestor at the time `three_way_merge` is called
- The fix handles rapid successive file change events without producing truncated merge output