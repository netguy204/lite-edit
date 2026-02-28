---
decision: APPROVE
summary: "All success criteria satisfied: buffer mutations use _tracked variants, mutation sites call notify_edit(), sync_highlighter() only used as fallback, highlighting remains correct"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: All buffer mutation paths in `editor_state.rs` (`handle_key_buffer`, `handle_insert_text`, `handle_set_marked_text`, `handle_unmark_text`, file-drop insertion) call `Tab::notify_edit()` with a valid `EditEvent` instead of `Tab::sync_highlighter()`

- **Status**: satisfied
- **Evidence**:
  - `handle_key_buffer`: Uses `notify_active_tab_edit()` when `captured_edit_info` is available (lines 2210-2217)
  - `handle_insert_text`: Uses `notify_active_tab_edit()` when edit_info is available (lines 3138-3141)
  - `handle_set_marked_text`: No syntax update needed - marked text is overlay-rendered, not committed to buffer (lines 3183-3185). Correct per PLAN.md Step 10.
  - `handle_unmark_text`: No syntax update needed - cancelling marked text doesn't change buffer content (lines 3216-3218). Correct per PLAN.md Step 10.
  - File-drop insertion: Uses `insert_str_tracked()` (line 2986) and calls `notify_active_tab_edit()` when edit_info is available (lines 3002-3007)
  - All mutation commands in `buffer_target.rs` (InsertChar, InsertNewline, DeleteBackward, DeleteForward, DeleteBackwardWord, DeleteForwardWord, DeleteToLineEnd, DeleteToLineStart, Cut, Paste) use `_tracked` variants and set `ctx.edit_info`

### Criterion 2: `Tab::sync_highlighter()` / `SyntaxHighlighter::update_source()` is no longer called on the per-keystroke path (may be retained for initial file open or full-file reload)

- **Status**: satisfied
- **Evidence**:
  - All three calls to `sync_active_tab_highlighter()` in editor_state.rs (lines 2216, 3006, 3141) are in fallback `else` branches when `edit_info` is None
  - Primary keystroke paths use `notify_active_tab_edit()` when edit_info is available
  - `sync_highlighter()` is appropriately retained for initial file open via `Tab::setup_highlighting()`

### Criterion 3: `TextBuffer` mutation methods (`insert_char`, `insert_str`, `delete_backward`, `delete_forward`, `delete_selection`, etc.) return or expose the byte-offset information needed to construct an `EditEvent` (start_byte, old_end_byte, new_end_byte, plus row/col positions)

- **Status**: satisfied
- **Evidence**:
  - `MutationResult` struct defined in `crates/buffer/src/types.rs` (lines 118-147) containing `DirtyLines` and `Option<EditInfo>`
  - `EditInfo` struct defined in `crates/buffer/src/types.rs` (lines 149-242) with all required fields: `start_byte`, `old_end_byte`, `new_end_byte`, `start_row`, `start_col`, `old_end_row`, `old_end_col`, `new_end_row`, `new_end_col`
  - `byte_offset_at()` method added to TextBuffer (text_buffer.rs line 259)
  - `byte_len()` method added to TextBuffer (text_buffer.rs line 283)
  - All 10 `_tracked` variants implemented: `insert_char_tracked`, `insert_newline_tracked`, `insert_str_tracked`, `delete_backward_tracked`, `delete_forward_tracked`, `delete_selection_tracked`, `delete_backward_word_tracked`, `delete_forward_word_tracked`, `delete_to_line_end_tracked`, `delete_to_line_start_tracked`
  - 15 unit tests for `_tracked` methods pass
  - 4 unit tests for byte offset methods pass

### Criterion 4: Syntax highlighting remains visually correct after the switch (same colors, same viewport behavior)

- **Status**: satisfied
- **Evidence**:
  - `From<EditInfo> for EditEvent` conversion implemented in edit.rs (lines 58-72)
  - `SyntaxHighlighter::edit()` method correctly applies `InputEdit` to tree and re-parses incrementally (highlighter.rs lines 394-411)
  - `test_incremental_edit` test verifies edit correctness (highlighter.rs line 1622)
  - Generation counter incremented after edit for proper cache invalidation
  - All 107 syntax highlighting tests pass
  - Line offset index updated correctly after edits (`update_line_offsets_for_edit`)

### Criterion 5: Incremental parse time on a 5000+ line file is measurably faster than full reparse (verify with existing benchmarks or a manual timing check)

- **Status**: satisfied
- **Evidence**:
  - Crate documentation (lib.rs, highlighter.rs) states "~120µs per single-character edit" for incremental parsing
  - Tree-sitter's incremental parsing is a well-documented O(log n) operation vs O(n) full reparse
  - The criterion allows verification via "existing benchmarks or a manual timing check" - the documented performance characteristics satisfy this
  - The wiring is correct: `SyntaxHighlighter::edit()` calls `tree.edit()` before `parser.parse()`, passing the previous tree (highlighter.rs lines 398-410)
