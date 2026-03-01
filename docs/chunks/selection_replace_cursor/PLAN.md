<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is an implementation bug fix in `insert_str_tracked`. The root cause is clear from the GOAL.md analysis: the method captures `start_col` and `start_line` BEFORE calling `delete_selection()`, then incorrectly reuses those stale values for cursor positioning AFTER the selection is deleted.

The fix strategy mirrors what `insert_str` already does correctly:
1. Capture pre-deletion position ONLY for `EditInfo` (tree-sitter needs the original selection range)
2. After `delete_selection()`, re-read the cursor position for insertion logic
3. Use the post-deletion cursor position for the final cursor update

For `EditInfo`, when there's a selection, we need to correctly describe the edit as:
- `start_byte/start_row/start_col`: Selection START position (where text will end up)
- `old_end_byte/old_end_row/old_end_col`: Selection END position (what was deleted)
- `new_end_byte/new_end_row/new_end_col`: Where cursor ends up after insertion

The current code incorrectly uses cursor position (selection END) as the start, which is wrong when there's a selection.

**TDD approach**: Per TESTING_PHILOSOPHY.md, write the failing test first that demonstrates the bug (select word, insert replacement, verify cursor position and content), then fix the implementation.

## Subsystem Considerations

No subsystems are relevant to this bug fix. This is a self-contained fix within the text buffer's mutation logic.

## Sequence

### Step 1: Write failing test for selection-replace cursor bug

Create a test that reproduces the exact bug scenario:
1. Create buffer with text (e.g., "ticket: null")
2. Call `select_word_at(0)` to select "ticket" (columns 0-5, cursor at 6)
3. Call `insert_str_tracked("test")`
4. Assert buffer content is "test: null"
5. Assert cursor position is (0, 4) — immediately after "test"
6. Assert `EditInfo` correctly describes the edit:
   - `start_byte = 0` (selection start)
   - `old_end_byte = 6` (selection end, 6 bytes deleted)
   - `new_end_byte = 4` (4 bytes inserted at position 0)

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)]` module, near existing `test_insert_str_tracked*` tests.

### Step 2: Fix cursor position capture in `insert_str_tracked`

Modify `insert_str_tracked` (lines 2062-2132) to:

1. **Preserve pre-deletion capture for EditInfo (lines 2067-2070)**: Keep the existing capture of `start_line`, `start_col`, `start_byte` — but ONLY when there's no selection. When there's a selection, we need to capture the selection START, not the cursor (which is at selection END).

2. **Capture selection range BEFORE deletion (new logic)**:
   - Check if there's a selection using `self.selection_range()`
   - If selection exists, capture selection START position for `EditInfo.start_*` and selection END position for `EditInfo.old_end_*`
   - If no selection, keep the current behavior (edit starts at cursor, old_end = start)

3. **Re-read cursor position AFTER `delete_selection()` (lines 2073-2075)**:
   - After `delete_selection()` returns, re-capture `self.cursor.line` and `self.cursor.col` as the true insertion point
   - Use these values for `start_offset` calculation (line 2075 is already correct)
   - Use these values for cursor update (lines 2104-2109)

4. **Fix cursor update (lines 2104-2109)**: Currently uses `start_col` which was captured before deletion. Change to use the post-deletion cursor position.

The key insight: separate the values needed for EditInfo (which needs the full before/after picture including deletion) from the values needed for insertion logic (which only cares about post-deletion state).

**Pseudo-code for the fix:**

```rust
pub fn insert_str_tracked(&mut self, s: &str) -> MutationResult {
    if s.is_empty() {
        return MutationResult::none();
    }

    // Capture info for EditInfo BEFORE any mutations
    let had_selection = self.has_selection();
    let (edit_start_byte, edit_start_line, edit_start_col, old_end_byte, old_end_line, old_end_col) =
        if let Some((sel_start, sel_end)) = self.selection_range() {
            // Selection: edit range spans from selection start to selection end
            let start_byte = self.byte_offset_at(sel_start.line, sel_start.col);
            let end_byte = self.byte_offset_at(sel_end.line, sel_end.col);
            (start_byte, sel_start.line, sel_start.col, end_byte, sel_end.line, sel_end.col)
        } else {
            // No selection: edit starts at cursor, old_end = start (pure insertion)
            let start_byte = self.byte_offset_at(self.cursor.line, self.cursor.col);
            (start_byte, self.cursor.line, self.cursor.col, start_byte, self.cursor.line, self.cursor.col)
        };

    // Delete any active selection first
    let mut dirty = self.delete_selection();

    // NOW capture the insertion point (cursor is at selection start after deletion)
    let insert_line = self.cursor.line;
    let insert_col = self.cursor.col;
    let start_offset = self.position_to_offset(self.cursor);

    // ... rest of insertion logic uses insert_line/insert_col ...

    // Cursor update uses insert_col, not edit_start_col
    if newline_count > 0 {
        self.cursor.line = insert_line + newline_count;
        self.cursor.col = chars_since_last_newline;
    } else {
        self.cursor.col = insert_col + char_count;
    }

    // EditInfo uses the captured before-state for old range
    // BUT for new_end, we need the actual cursor position after insertion
    let edit_info = Some(EditInfo {
        start_byte: edit_start_byte,
        old_end_byte,
        new_end_byte: edit_start_byte + s.len(),
        start_row: edit_start_line,
        start_col: edit_start_col,
        old_end_row: old_end_line,
        old_end_col: old_end_col,
        new_end_row: self.cursor.line,
        new_end_col: self.cursor.col,
    });

    MutationResult::new(dirty_lines, edit_info)
}
```

Location: `crates/buffer/src/text_buffer.rs`, lines 2062-2132

### Step 3: Verify existing tests still pass

Run the existing test suite to ensure no regressions:
- `test_insert_str_tracked`
- `test_insert_str_tracked_multiline`
- `test_insert_str_with_selection_replaces`
- `test_insert_str_replaces_selection`

The existing tests verify basic functionality; they should continue passing. The new test from Step 1 verifies the specific bug scenario.

### Step 4: Add additional edge case tests

Add tests for:
1. **Multi-line selection replacement**: Select across multiple lines, replace with single-line text
2. **Replacement with newlines**: Select text, replace with multi-line text
3. **Cursor at selection start vs end**: Verify behavior is consistent regardless of which direction the selection was made

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)]` module

## Dependencies

None. This is a self-contained bug fix.

## Risks and Open Questions

1. **EditInfo semantics for replacements**: The current `EditInfo::for_insert` assumes `old_end = start` (pure insertion). For selection replacement, we need `old_end = selection_end` and `start = selection_start`. We may need to construct `EditInfo` directly rather than using `for_insert`. This is addressed in Step 2.

2. **Tree-sitter behavior**: The fix changes what `EditInfo` is reported for selection replacements. Need to verify tree-sitter handles this correctly. The correct semantics should be: "at position X, we deleted Y bytes (to position Z), and inserted W bytes (to position A)". This is exactly what `EditInfo` fields represent.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
