---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/src/highlighter.rs
- crates/syntax/src/registry.rs
- crates/editor/src/highlighted_buffer.rs
code_references:
  - ref: crates/syntax/src/highlighter.rs#HighlightCache
    implements: "Cache structure for viewport highlight results with generation-based invalidation"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::highlight_viewport
    implements: "Viewport-batch highlighting method using QueryCursor against cached tree"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::collect_captures_in_range
    implements: "QueryCursor-based capture collection with byte range scoping"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures
    implements: "StyledLine construction from pre-collected captures"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::highlight_line
    implements: "Cache-aware line highlight with fallback to single-line path"
  - ref: crates/editor/src/highlighted_buffer.rs#HighlightedBufferView::styled_line
    implements: "Viewport pre-population trigger before line retrieval"
  - ref: crates/editor/src/highlighted_buffer.rs#HighlightedBufferViewMut::styled_line
    implements: "Mutable viewport pre-population trigger before line retrieval"
  - ref: crates/syntax/src/registry.rs#LanguageConfig
    implements: "Added highlights_query field for direct QueryCursor usage"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on:
- syntax_highlighting
created_after:
- syntax_highlighting
---

# Chunk Goal

## Minor Goal

Fix critical performance regression where syntax highlighting causes 100% CPU usage and unresponsive scrolling/clicking. The editor must meet its <8ms P99 keypress-to-glyph latency north star.

**Root cause**: `SyntaxHighlighter::highlight_line()` creates a new `Highlighter` and runs a **full-file highlight pass** over the entire source for every single line request. The renderer calls this once per visible line, so a 60-line viewport triggers 60 full-file highlight iterations per render frame. The render timer fires continuously, making this a hot loop that saturates one CPU core. CPU usage and latency scale with file size — larger files are worse.

From the profiling call graph (steady-state, no user interaction):
- 2012/2144 samples (94%) are in the render timer callback
- 958 samples in `update_glyph_buffer` at offset +8292 (one call site) and 710 at +4708 (another), both calling `highlight_line`
- Inside `highlight_line`: `ts_parser_parse` (full reparse per call), `ts_query_cursor_next_capture` (full query traversal), `ts_tree_delete` (tree destruction per call)

**Compounding issue**: `SyntaxHighlighter` correctly caches its `Tree` and passes it to `parser.parse()` in the `edit()` path, which gives tree-sitter's O(edit-size) incremental parsing. However, `highlight_line()` calls `Highlighter::highlight()` which does its **own independent parse** internally — it never uses the cached `self.tree`. This means the profiled `ts_parser_parse` samples are not from the incremental edit path but from the highlight API reparsing from scratch on every line, every frame. The cached tree is maintained but never leveraged for rendering.

**Fix**: Replace per-line full-file highlighting with a **viewport-batch approach that uses the cached tree**:

1. **Query the cached tree directly**: Use `QueryCursor` with `set_byte_range()` against `self.tree` to run highlight queries scoped to the visible byte range. This leverages the incrementally-maintained tree and avoids the `Highlighter::highlight()` API which does its own parse.
2. **Cache highlight results**: After querying, cache the resulting spans per line. Invalidate the cache only when the source changes (via `edit()` or `update_source()`) or the viewport shifts.
3. **Eliminate per-line allocations**: The current code creates a new `Highlighter`, triggers a full reparse, iterates the full file, and discards everything — per line. The fix must batch the viewport into a single query pass against the already-parsed tree.

The investigation (`docs/investigations/syntax_highlighting_scalable`) already validated that viewport-scoped highlighting costs ~170µs for 60 lines (2.1% of budget). The current implementation accidentally bypasses this optimization by calling the full-file `Highlighter::highlight()` API per line.

## Success Criteria

- CPU usage at idle (no user interaction) drops from 100% to <5% with a syntax-highlighted file open.
- Scrolling and clicking respond within the <8ms latency budget for files up to 10K lines.
- Syntax highlighting visual output remains identical (same colors, same captures).
- The `SyntaxHighlighter::edit()` incremental parse path continues to work correctly.
- All existing syntax highlighting tests continue to pass.
- A benchmark or test validates that `highlight_line` for a 60-line viewport completes in <1ms total.
- The incrementally-maintained `self.tree` is used for highlight queries — no redundant reparses in the render path.