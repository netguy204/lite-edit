---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/highlighter.rs
code_references:
  - ref: crates/syntax/src/highlighter.rs#safe_char_boundary
    implements: "UTF-8 safe byte offset adjustment helper function"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::line_byte_range
    implements: "Line byte range with safe char boundary validation"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- terminal_spawn_reliability
- treesitter_gotodef_type_resolution
---

# Chunk Goal

## Minor Goal

Eliminate panics caused by slicing strings at non-char-boundary byte indices in the syntax highlighter. The editor crashes when editing files containing multi-byte UTF-8 characters (e.g., box-drawing characters like `╔`) because `SyntaxHighlighter` methods use raw byte offsets for `&str` slicing without validating they fall on character boundaries.

**Root cause:** `line_byte_range()` at `crates/syntax/src/highlighter.rs:1315` computes line end as `self.line_offsets[line_idx + 1] - 1`, which can land inside a multi-byte character. Additionally, after edits that change content length (e.g., replacing "IMPLEMENTING" with "F"), stale capture byte offsets shift and can point into the middle of multi-byte characters in the updated source text.

**Primary crash site:** Line 970: `let line_text = &self.source[line_start..line_end]` inside `build_line_from_captures()`.

There are ~13 locations in the file that perform unchecked `&self.source[byte..byte]` slicing that are all vulnerable to this class of bug.

## Success Criteria

- Editing files containing multi-byte UTF-8 characters (box-drawing, CJK, emoji, etc.) no longer causes panics in the highlighter
- All `&self.source[start..end]` slicing in `highlighter.rs` validates char boundaries before slicing (using `str::is_char_boundary()` or equivalent)
- `line_byte_range()` returns byte ranges that are guaranteed to be valid char boundaries
- The fix handles stale capture offsets gracefully (clamp to nearest valid boundary or skip the span) rather than panicking
- Regression test covering multi-byte characters in highlighted content