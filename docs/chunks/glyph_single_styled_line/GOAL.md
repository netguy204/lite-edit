---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/glyph_buffer.rs
code_references:
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Pre-collects styled lines once per visible buffer line, eliminating 3× redundant styled_line() calls per frame in the non-wrap rendering path"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_wrap
    implements: "Pre-collects styled lines once per visible buffer line, eliminating 3× redundant styled_line() calls per frame in the wrap-enabled rendering path"
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

Eliminate redundant `styled_line()` calls in `GlyphBuffer::update_from_buffer_with_wrap()`.

Currently, each visible buffer line has `view.styled_line(buffer_line)` called **3 separate times** per frame — once in Phase 1 (background quads), once in Phase 3 (glyph quads), and once in Phase 4 (underline quads). Each call goes through `HighlightedBufferView`, which checks the highlight cache and clones a `StyledLine` (containing a `Vec<Span>` with owned `String` text). For 60 visible lines, that's 180 calls where 60 would suffice — 120 unnecessary `StyledLine` clones per frame.

The fix: collect `styled_line()` results once per visible buffer line into a temporary `Vec<StyledLine>` before entering the per-phase loops, then reference that collection in each phase.

Profiling shows this saves ~7µs/frame (from 12.3µs to 5.0µs for the clone path) — a 59% reduction in clone overhead. This is a code quality improvement more than a critical performance fix, but it removes an unnecessary 3× amplification pattern and simplifies future maintenance of the rendering phases.

## Success Criteria

- `view.styled_line(buffer_line)` is called exactly **once** per visible buffer line per frame in `update_from_buffer_with_wrap()`
- The result is stored in a pre-collected `Vec<StyledLine>` (or equivalent) and referenced by all phases (background, glyph, underline)
- No change to rendered output — visual parity with current rendering
- All existing tests pass
- The non-wrap path (`update_from_buffer_with_cursor`) is similarly deduplicated if it has the same pattern