---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/highlighter.rs
code_references:
  - ref: crates/syntax/src/highlighter.rs#build_line_offsets
    implements: "O(n) one-time pass to build Vec<usize> of line start byte offsets"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::line_offsets
    implements: "Field storing the precomputed line offset index"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::update_line_offsets_for_edit
    implements: "Incremental update of line offsets during edit() without full rebuild"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::line_byte_range
    implements: "O(1) line byte range lookup using offset index"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::line_count
    implements: "O(1) line count via line_offsets.len()"
narrative: null
investigation: scroll_perf_deep
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- welcome_screen
- syntax_highlight_perf
---

# Chunk Goal

## Minor Goal

Build a precomputed line offset index in `SyntaxHighlighter` to make `line_byte_range()` O(1) instead of O(n), and `line_count()` O(1) instead of scanning the full source.

Currently, `line_byte_range(line_idx)` scans from byte 0 of the source string on every call using `char_indices()`. During `highlight_viewport()`, this is called 62+ times (2 for viewport byte bounds + 1 per line in `build_line_from_captures`). At deep scroll positions in large files, this dominates CPU cost — profiling on a 5,911-line file shows viewport highlighting at line 4000 takes **6,032µs (75% of the 8ms frame budget)**, of which **~4,432µs is line scanning alone**.

The fix: store a `Vec<usize>` of byte offsets for each line start, built during initial parse and rebuilt on edits. This makes every `line_byte_range()` call a simple index lookup. Profiling shows the index build cost is **94µs** (one-time per parse) and 62 O(1) lookups cost **0.02µs** total — a 220,000× improvement over the current approach.

This is the highest-impact performance fix identified by the `scroll_perf_deep` investigation, expected to reduce viewport highlight cost from 6ms to ~1.5ms at any scroll position.

## Success Criteria

- `SyntaxHighlighter` stores a `Vec<usize>` of line-start byte offsets, built during `new()` and updated during `edit()` / `update_source()`
- `line_byte_range()` uses the index for O(1) lookup instead of scanning from byte 0
- `line_count()` returns `self.line_offsets.len()` instead of scanning the source with `chars().filter()`
- `highlight_viewport()` cost is position-independent: highlighting at line 4000 takes roughly the same time as at line 0 (within 2× tolerance)
- Existing tests pass — `test_line_byte_range_first_line`, `test_line_byte_range_second_line`, `test_line_byte_range_out_of_bounds`, `test_line_count_single_line`, `test_line_count_multiple_lines`
- Incremental edit correctly updates the offset index (offsets after the edit point are adjusted)
- The profiling test (`docs/investigations/scroll_perf_deep/prototypes/profile_scroll.rs`) shows viewport highlight at line 4000 under 2,000µs (down from 6,032µs)

