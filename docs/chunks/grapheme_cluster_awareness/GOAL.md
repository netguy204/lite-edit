---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/Cargo.toml
- crates/buffer/src/lib.rs
- crates/buffer/src/grapheme.rs
- crates/buffer/src/text_buffer.rs
- crates/buffer/tests/grapheme.rs
code_references:
- ref: crates/buffer/src/grapheme.rs
  implements: "Grapheme cluster boundary detection helpers (grapheme_boundary_left/right, grapheme_len_before/at, is_grapheme_boundary)"
- ref: crates/buffer/src/text_buffer.rs#TextBuffer::move_left
  implements: "Grapheme-aware left cursor movement"
- ref: crates/buffer/src/text_buffer.rs#TextBuffer::move_right
  implements: "Grapheme-aware right cursor movement"
- ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_backward
  implements: "Grapheme-aware backspace deletion"
- ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_forward
  implements: "Grapheme-aware forward deletion"
- ref: crates/buffer/src/text_buffer.rs#TextBuffer::select_word_at
  implements: "Grapheme-aware word selection"
- ref: crates/buffer/tests/grapheme.rs
  implements: "Integration tests for grapheme-aware editing operations"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- typescript_highlight_layering
---

# Chunk Goal

## Minor Goal

The text buffer operates on Rust `char` (Unicode scalar values), but cursor movement and deletion ignore grapheme cluster boundaries. A user pressing backspace should delete one visible character — but for emoji like `👨‍👩‍👧‍👦` (7 chars: 4 codepoints + 3 ZWJ), combining characters like `é` (e + combining acute), and regional indicators like `🇺🇸` (2 chars), the editor currently requires multiple backspaces and can leave orphaned combining marks.

Add grapheme cluster boundary detection to all cursor movement and deletion operations in the buffer crate. Use the `unicode-segmentation` crate to determine cluster boundaries. This is a correctness fix that affects every non-ASCII user.

**Key files**: `crates/buffer/src/text_buffer.rs` (delete/movement operations), `crates/buffer/src/gap_buffer.rs` (may need multi-char delete helper)

**Origin**: Architecture review recommendation #1 (P0 — Correctness). See `ARCHITECTURE_REVIEW.md`.

## Success Criteria

- Backspace deletes one grapheme cluster (not one char) — verified for: multi-codepoint emoji (ZWJ sequences), combining characters, regional indicators
- Arrow keys move by grapheme cluster, not by char
- Selection expansion (Shift+Arrow) selects by grapheme cluster
- Double-click word selection respects grapheme boundaries
- Existing ASCII editing behavior is unchanged (grapheme = char for ASCII)
- Unit tests cover: ZWJ emoji, combining marks, regional flags, Hangul jamo

