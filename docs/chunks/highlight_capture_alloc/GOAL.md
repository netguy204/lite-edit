---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/highlighter.rs
code_references:
  - ref: crates/syntax/src/highlighter.rs#CaptureEntry
    implements: "Type alias for (start_byte, end_byte, capture_index) tuples using u32 index instead of String"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::captures_buffer
    implements: "RefCell<Vec<CaptureEntry>> field for reusable capture buffer across frames"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::collect_captures_in_range
    implements: "Stores u32 capture index instead of allocating String; clears and reuses captures_buffer"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures
    implements: "Binary search via partition_point() to find first relevant capture; lazy capture name lookup"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_styled_line_from_query
    implements: "Updated for u32 capture index with lazy name resolution via Query::capture_names()"
narrative: null
investigation: scroll_perf_deep
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- highlight_line_offset_index
created_after:
- welcome_screen
- syntax_highlight_perf
---

# Chunk Goal

## Minor Goal

Reduce per-frame allocations in the syntax highlighting hot path within `SyntaxHighlighter`.

After the line offset index fix (chunk `highlight_line_offset_index`) eliminates the dominant O(n) scanning bottleneck, the residual ~1.2ms viewport highlight cost includes allocation overhead from `collect_captures_in_range()` and linear scanning in `build_line_from_captures()`. This chunk addresses three secondary inefficiencies:

1. **String allocation per capture**: `collect_captures_in_range()` calls `(*name).to_string()` for every tree-sitter capture in the viewport, creating hundreds of small heap allocations. The capture name is only used for theme lookup — storing a `u32` capture index and resolving the name only at theme lookup time eliminates these allocations.

2. **Linear scan of captures per line**: `build_line_from_captures()` iterates ALL viewport captures for each line, checking byte range overlap. Since captures are sorted by start byte, a binary search (`partition_point`) can skip to the first relevant capture in O(log n).

3. **Fresh Vec allocation per viewport highlight**: `collect_captures_in_range()` allocates a new `Vec` every call. A `RefCell<Vec>` member can be cleared and reused across frames, avoiding repeated allocation.

This is a low-priority polish chunk. Profiling shows the base highlight cost at line 0 is ~490µs; this chunk aims to shave 100-300µs off that residual.

## Success Criteria

- `collect_captures_in_range()` stores `(usize, usize, u32)` tuples (start_byte, end_byte, capture_index) instead of `(usize, usize, String)`
- Capture name is resolved to a theme style via the `u32` index at lookup time in `build_line_from_captures()` and `build_styled_line_from_query()`
- `build_line_from_captures()` uses `partition_point()` or equivalent to find the first capture overlapping each line, instead of scanning from index 0
- The captures `Vec` is stored as a `RefCell<Vec<...>>` field and reused (cleared, not reallocated) across `highlight_viewport()` calls
- All existing highlighter tests pass
- No change to rendered output (visual parity with current highlighting)