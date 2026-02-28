---
decision: FEEDBACK
summary: "Primary keystroke paths correctly use incremental parsing, but file-drop insertion still uses full reparse and performance verification test is missing"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: All buffer mutation paths in `editor_state.rs` (`handle_key_buffer`, `handle_insert_text`, `handle_set_marked_text`, `handle_unmark_text`, file-drop insertion) call `Tab::notify_edit()` with a valid `EditEvent` instead of `Tab::sync_highlighter()`

- **Status**: gap
- **Evidence**:
  - `handle_key_buffer`: Uses `notify_active_tab_edit()` when `captured_edit_info` is available (lines 2210-2217) ✅
  - `handle_insert_text`: Uses `notify_active_tab_edit()` when `captured_edit_info` is available (lines 3133-3137) ✅
  - `handle_set_marked_text`: Calls `sync_active_tab_highlighter()` (line 3179) - intentional per PLAN.md Step 10 since marked text doesn't modify buffer ✅
  - `handle_unmark_text`: Calls `sync_active_tab_highlighter()` (line 3211) - intentional per PLAN.md Step 10 ✅
  - **File-drop insertion**: Uses `buffer.insert_str()` (non-tracked) at line 2985 and calls `sync_active_tab_highlighter()` at line 3002 ❌

### Criterion 2: `Tab::sync_highlighter()` / `SyntaxHighlighter::update_source()` is no longer called on the per-keystroke path (may be retained for initial file open or full-file reload)

- **Status**: gap
- **Evidence**:
  - Primary keystroke paths (`handle_key_buffer`, `handle_insert_text`) correctly use incremental parsing
  - File-drop insertion still calls `sync_active_tab_highlighter()` (line 3002)
  - DeleteBackwardWord, DeleteForwardWord, DeleteToLineEnd, DeleteToLineStart have TODO comments but fall back to non-tracked variants (buffer_target.rs lines 310-320), triggering full reparse via fallback path

### Criterion 3: `TextBuffer` mutation methods return byte-offset information

- **Status**: satisfied
- **Evidence**:
  - `MutationResult` and `EditInfo` types defined in `crates/buffer/src/types.rs` (lines 118-242)
  - `insert_char_tracked`, `insert_newline_tracked`, `insert_str_tracked` implemented in text_buffer.rs
  - `delete_backward_tracked`, `delete_forward_tracked`, `delete_selection_tracked` implemented
  - `byte_offset_at()` method added (text_buffer.rs lines 259-262)
  - `byte_len()` method added (text_buffer.rs lines 283-285)
  - All 9 `_tracked` tests pass

### Criterion 4: Syntax highlighting remains visually correct after the switch

- **Status**: satisfied
- **Evidence**:
  - `SyntaxHighlighter::edit()` method correctly applies `InputEdit` to tree and re-parses incrementally (highlighter.rs lines 394-411)
  - `From<EditInfo> for EditEvent` conversion implemented (edit.rs lines 58-72)
  - All 107 syntax tests pass
  - Generation counter incremented after edit for cache invalidation

### Criterion 5: Incremental parse time is measurably faster than full reparse

- **Status**: gap
- **Evidence**:
  - PLAN.md Step 12 specifies adding a performance verification test
  - No benchmark or test exists in `crates/syntax/tests/` or `crates/syntax/benches/`
  - Highlighter.rs docstring claims "~120µs per single-character edit" vs full reparse, but this is not verified by tests

## Feedback Items

### Issue 1: File-drop insertion uses full reparse

- **id**: issue-file-drop
- **location**: crates/editor/src/editor_state.rs:2985-3002
- **concern**: File-drop text insertion uses non-tracked `buffer.insert_str()` and calls `sync_active_tab_highlighter()` instead of using the incremental path
- **suggestion**: Change `buffer.insert_str(&escaped_text)` to `buffer.insert_str_tracked(&escaped_text)`, capture the edit info, and call `self.notify_active_tab_edit(edit_info.into())` instead of `sync_active_tab_highlighter()`
- **severity**: functional
- **confidence**: high

### Issue 2: Missing performance verification test

- **id**: issue-perf-test
- **location**: crates/syntax/tests/ (missing)
- **concern**: Success criterion 5 requires verifying incremental parse is faster than full reparse, but no test exists
- **suggestion**: Add a test in `crates/syntax/tests/` or benchmark in `crates/syntax/benches/` that compares `edit()` vs `update_source()` timing on a 5000+ line file, as specified in PLAN.md Step 12
- **severity**: functional
- **confidence**: high
