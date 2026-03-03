---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/buffer/src/text_buffer.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::insert_str_tracked
    implements: "Fixed cursor position capture to read post-deletion position for insertion logic"
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

Fix `insert_str_tracked` capturing cursor position BEFORE `delete_selection()`, causing the cursor to jump to the wrong position after replacing a selection. This makes "double-click a word, then type replacement text" insert the first character correctly but put subsequent characters at a completely wrong location in the file.

## Bug Details

**Root cause:** `insert_str_tracked` (line 2062) captures `start_col = self.cursor.col` at line 2069 BEFORE calling `delete_selection()` at line 2073. When there's an active selection, the cursor is at the selection END. After `delete_selection()` moves the cursor to the selection START, the stale `start_col` is used at line 2108 to compute the post-insertion cursor:

```rust
self.cursor.col = start_col + char_count;  // start_col is selection END, not START
```

The non-tracked `insert_str` (line 1992) does this correctly — it captures `start_col` AFTER `delete_selection()`.

The pre-deletion capture exists because `insert_str_tracked` needs the old position for tree-sitter `EditInfo`. But the cursor update erroneously reuses these stale values.

**Reproduction:**

1. Double-click a word (e.g., "ticket" at columns 0-5) — word is correctly selected and highlighted
2. Type "test" to replace — first char "t" replaces the word correctly, but cursor jumps to column 7 (old word_end + 1) instead of column 1
3. Remaining chars "est" are inserted at column 7, 8, 9 — appearing at the END of the line instead of after "t"
4. Result: `t: nullest` instead of `test: null`

**Key code locations:**

- `crates/buffer/src/text_buffer.rs:2062-2132` — `insert_str_tracked` (the buggy method)
- `crates/buffer/src/text_buffer.rs:2067-2070` — Pre-deletion cursor capture (for EditInfo)
- `crates/buffer/src/text_buffer.rs:2073` — `delete_selection()` call that moves cursor
- `crates/buffer/src/text_buffer.rs:2104-2109` — Cursor update using stale `start_col`/`start_line`
- `crates/buffer/src/text_buffer.rs:1992-2055` — `insert_str` (correct version for comparison)

**Fix approach:** Capture pre-deletion position for EditInfo only, then re-read `self.cursor.line`/`self.cursor.col` after `delete_selection()` for the insertion logic and cursor update. The `start_byte` for EditInfo may also need adjustment — when there's a selection, the edit's old range spans from selection start to selection end, not just the cursor position.

## Success Criteria

- Double-clicking a word and typing a replacement correctly replaces the entire word with the typed text — all characters appear at the selection position, not scattered
- The cursor position after replacing a selection is immediately after the inserted text
- The `EditInfo` returned for tree-sitter still correctly describes the edit (old range covers the deleted selection, new range covers the inserted text)
- The existing `insert_str` (non-tracked) behavior is unchanged
- A test case covers: select a word via `select_word_at`, call `insert_str_tracked` with replacement text, verify buffer content and cursor position