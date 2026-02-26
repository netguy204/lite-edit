---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/merge.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/lib.rs
  - crates/editor/Cargo.toml
code_references:
  - ref: crates/editor/src/merge.rs#MergeResult
    implements: "Result type distinguishing clean merges from conflicts"
  - ref: crates/editor/src/merge.rs#three_way_merge
    implements: "Core diff3 algorithm using similar::TextDiff for line-level merge"
  - ref: crates/editor/src/editor_state.rs#EditorState::merge_file_tab
    implements: "Integration point that performs merge on dirty buffers when FileChanged arrives"
  - ref: crates/editor/src/drain_loop.rs#EventLoop::handle_file_changed
    implements: "Event handler that routes to merge_file_tab for dirty tabs"
narrative: null
investigation: concurrent_edit_sync
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- base_snapshot_reload
created_after:
- emacs_keybindings
- terminal_close_guard
- welcome_file_backed
---

# Chunk Goal

## Minor Goal

Implement line-level three-way merge for dirty buffers when the file changes on disk. This is the core of the concurrent-edit experience — when the user has unsaved edits and an external program modifies the same file, the editor intelligently merges both sets of changes, only flagging true conflicts.

Add the `similar` crate to the workspace. Implement the diff3 merge algorithm: given base (stored snapshot), ours (buffer content), and theirs (new disk content), compute line-level diffs from base to each side, walk both edit maps, apply non-overlapping changes cleanly, and produce git-style conflict markers (`<<<<<<< buffer` / `=======` / `>>>>>>> disk`) for overlapping changes.

Wire this into the `FileChanged` handler: when the event arrives for a tab with `dirty == true`, run the merge instead of reloading.

The merge algorithm prototype is available at `docs/investigations/concurrent_edit_sync/prototypes/three_way_merge_test.rs` — the production implementation should follow the same approach (edit maps built from `similar::TextDiff::from_lines`, per-line action tracking, insertion/replacement/deletion handling).

## Success Criteria

- The `similar` crate is added to the workspace dependencies
- A merge module exists (e.g., `crates/editor/src/merge.rs` or `crates/buffer/src/merge.rs`) with a `three_way_merge(base, ours, theirs) -> MergeResult` function
- `MergeResult` distinguishes clean merges from conflicts
- When `FileChanged` arrives for a dirty tab:
  - Three-way merge runs with `base_content` as base, buffer content as ours, disk content as theirs
  - On clean merge: buffer content is replaced with the merged result, cursor position is preserved or adjusted to the first changed line, dirty flag remains true, `base_content` is updated to the new disk content
  - On conflict: buffer content is replaced with the merged result (including conflict markers), dirty flag remains true
- Non-overlapping edits at different locations merge cleanly (no false conflicts)
- Adjacent-line edits (line N and line N+1) merge cleanly
- Identical changes from both sides merge cleanly (convergent edits)
- Overlapping edits produce correct conflict markers with buffer content in the top section and disk content in the bottom section
- Unit tests cover the scenarios from the investigation prototype (at minimum: non-overlapping, convergent, same-line conflict, delete-vs-modify conflict, adjacent edits)

## Rejected Ideas

### Use operational transform (OT) or CRDT

We could implement a more sophisticated concurrency model like OT or CRDT that tracks individual operations rather than snapshots.

Rejected because: OT/CRDT are designed for real-time collaborative editing with continuous operation streams. Our use case is periodic file-level syncs — we detect a changed file, merge once, and move on. Three-way merge is the right tool for this granularity, well-understood, and was validated in the investigation prototype.

### Character-level merge

We could diff and merge at character granularity for more precise conflict detection.

Rejected because: Line-level merge was validated against 12 realistic scenarios in the investigation and produced correct results in all cases. Character-level adds significant complexity with no demonstrated benefit for our use case. The `TextBuffer`'s `LineIndex` makes line-level operations natural.