<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds a text selection model to `TextBuffer` using the anchor-cursor approach described in the goal. The implementation follows the existing patterns in `text_buffer.rs`:

**Selection Model**: An optional `selection_anchor: Option<Position>` field represents the start of a selection. When `Some`, the range between anchor and cursor defines the selected text. The selection can be in either direction (anchor before or after cursor) — `selection_range()` normalizes this to document order.

**Integration with Mutations**: Following the principle that mutations are the critical path, all mutation operations (`insert_char`, `insert_newline`, `insert_str`, `delete_backward`, `delete_forward`) are modified to delete the selection first when one exists. This uses a `delete_selection()` helper that removes the selected range, places the cursor at the start, and clears the anchor.

**Clearing Selection on Movement**: All `move_*` methods and `set_cursor` clear the selection when called. This is standard editor behavior where arrow keys collapse the selection. Shift+movement for extending selection is explicitly out of scope (future chunk).

**TDD Approach**: Per TESTING_PHILOSOPHY.md, we write failing tests first for meaningful behavior. Selection operations have clear semantics that can be tested without platform dependencies.

**Dirty Line Tracking**: `delete_selection()` returns `DirtyLines` covering the affected range. Multi-line deletions return `FromLineToEnd(start_line)` since subsequent lines shift up.

## Subsystem Considerations

No documented subsystems exist yet. The `text_*` cluster is small (2 chunks), so no subsystem documentation is warranted at this stage.

## Sequence

### Step 1: Write tests for selection anchor and basic API

**TDD red phase.** Before implementing, add tests to the existing `#[cfg(test)] mod tests` in `text_buffer.rs`:

- `test_set_selection_anchor` — set anchor, verify it's stored
- `test_set_selection_anchor_at_cursor` — convenience method sets anchor to current cursor
- `test_clear_selection` — clears the anchor
- `test_has_selection_false_when_no_anchor` — `has_selection()` returns false when anchor is None
- `test_has_selection_false_when_anchor_equals_cursor` — edge case: anchor == cursor means no selection
- `test_has_selection_true_when_selection_exists` — anchor != cursor returns true

Location: `crates/buffer/src/text_buffer.rs`

### Step 2: Add selection_anchor field and basic methods

**TDD green phase.** Add the field and methods to make the tests pass:

```rust
pub struct TextBuffer {
    // ... existing fields ...
    selection_anchor: Option<Position>,
}
```

Methods:
- `set_selection_anchor(pos: Position)` — clamps pos to valid bounds and sets anchor
- `set_selection_anchor_at_cursor()` — sets anchor to current cursor position
- `clear_selection()` — sets anchor to None
- `has_selection() -> bool` — returns true if anchor is Some and differs from cursor

Location: `crates/buffer/src/text_buffer.rs`

### Step 3: Write tests for selection_range and selected_text

**TDD red phase.** Add tests:

- `test_selection_range_forward` — anchor at (0,0), cursor at (0,5), returns Some((start, end)) in order
- `test_selection_range_backward` — anchor after cursor, still returns (start, end) in document order
- `test_selection_range_multiline` — selection spans multiple lines
- `test_selection_range_none_when_no_anchor` — returns None
- `test_selected_text_single_line` — returns the correct substring
- `test_selected_text_multiline` — returns text including newlines across lines
- `test_selected_text_empty_when_anchor_equals_cursor` — returns None

Location: `crates/buffer/src/text_buffer.rs`

### Step 4: Implement selection_range and selected_text

**TDD green phase.**

- `selection_range() -> Option<(Position, Position)>` — returns `(start, end)` in document order by comparing anchor and cursor
- `selected_text() -> Option<String>` — extracts text between the two positions using the gap buffer

For `selected_text`, we need to convert positions to offsets and extract the slice. This may require using `position_to_offset` (already exists) and the gap buffer's `slice` method.

Location: `crates/buffer/src/text_buffer.rs`

### Step 5: Write tests for select_all

**TDD red phase.** Add tests:

- `test_select_all_empty_buffer` — anchor at (0,0), cursor at (0,0), has_selection is false
- `test_select_all_single_line` — anchor at buffer start, cursor at buffer end
- `test_select_all_multiline` — anchor at (0,0), cursor at last line end

Location: `crates/buffer/src/text_buffer.rs`

### Step 6: Implement select_all

**TDD green phase.**

`select_all()` — sets anchor to Position(0, 0), then moves cursor to buffer end using `move_to_buffer_end` logic but without clearing selection.

Note: `move_to_buffer_end` will be modified in Step 11 to clear selection, so `select_all` should directly set both anchor and cursor.

Location: `crates/buffer/src/text_buffer.rs`

### Step 7: Write tests for delete_selection helper

**TDD red phase.** Add tests:

- `test_delete_selection_single_line` — deletes selected chars within one line
- `test_delete_selection_multiline` — deletes across lines, joining remaining content
- `test_delete_selection_backward_selection` — anchor after cursor, same result
- `test_delete_selection_clears_anchor` — after deletion, anchor is None
- `test_delete_selection_cursor_at_start` — cursor moves to start of former selection
- `test_delete_selection_no_op_when_no_selection` — returns DirtyLines::None if no selection

Location: `crates/buffer/src/text_buffer.rs`

### Step 8: Implement delete_selection helper

**TDD green phase.**

`delete_selection() -> DirtyLines` — if no selection, return `DirtyLines::None`. Otherwise:
1. Get selection range (start, end) in document order
2. Delete characters from end back to start (or use a batch operation)
3. Set cursor to start position
4. Clear anchor
5. Return appropriate DirtyLines (single line if within one line, FromLineToEnd if multiline)

Implementation detail: Deleting a multi-line selection requires removing the text between two offsets. We can:
- Position cursor at end of selection
- Repeatedly call delete_backward until cursor reaches start offset
- OR implement a batch deletion method on the gap buffer

The simpler approach (repeated delete_backward) may be inefficient for large selections but is correct and matches existing patterns. For this foundational chunk, correctness is primary; optimization can be a follow-up.

Location: `crates/buffer/src/text_buffer.rs`

### Step 9: Write tests for mutations deleting selection first

**TDD red phase.** Add tests:

- `test_insert_char_with_selection_replaces` — select "ell", insert 'X', result is "hXo"
- `test_insert_newline_with_selection_replaces` — select text, insert newline, selection replaced with newline
- `test_insert_str_with_selection_replaces` — select "world", insert "universe", result has "universe"
- `test_delete_backward_with_selection_deletes_selection` — select "ell", backspace once deletes only selection
- `test_delete_forward_with_selection_deletes_selection` — select "ell", delete once deletes only selection

Location: `crates/buffer/src/text_buffer.rs`

### Step 10: Modify mutation methods to delete selection first

**TDD green phase.** Update:

- `insert_char` — at the start, if `has_selection()`, call `delete_selection()`, merge dirty lines
- `insert_newline` — same pattern
- `insert_str` — same pattern
- `delete_backward` — if `has_selection()`, call `delete_selection()` and return its dirty lines (don't delete additional char)
- `delete_forward` — same pattern

Location: `crates/buffer/src/text_buffer.rs`

### Step 11: Write tests for cursor movement clearing selection

**TDD red phase.** Add tests:

- `test_move_left_clears_selection` — has selection, move_left, selection is cleared
- `test_move_right_clears_selection`
- `test_move_up_clears_selection`
- `test_move_down_clears_selection`
- `test_move_to_line_start_clears_selection`
- `test_move_to_line_end_clears_selection`
- `test_move_to_buffer_start_clears_selection`
- `test_move_to_buffer_end_clears_selection`
- `test_set_cursor_clears_selection`

Location: `crates/buffer/src/text_buffer.rs`

### Step 12: Modify movement methods to clear selection

**TDD green phase.** Add `self.clear_selection()` at the start of:

- `move_left`
- `move_right`
- `move_up`
- `move_down`
- `move_to_line_start`
- `move_to_line_end`
- `move_to_buffer_start`
- `move_to_buffer_end`
- `set_cursor`

Location: `crates/buffer/src/text_buffer.rs`

### Step 13: Update constructors to initialize selection_anchor

Ensure `TextBuffer::new()` and `TextBuffer::from_str()` initialize `selection_anchor: None`.

Location: `crates/buffer/src/text_buffer.rs`

### Step 14: Run full test suite and verify

Run `cargo test -p buffer` to confirm all tests pass. Fix any issues.

### Step 15: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the code paths touched:

```yaml
code_paths:
  - crates/buffer/src/text_buffer.rs
```

---

**BACKREFERENCE COMMENTS**

Add a chunk backreference at the selection-related methods:

```rust
// Chunk: docs/chunks/text_selection_model - Selection anchor and range API
```

## Dependencies

This chunk depends on the chunks listed in `created_after`:
- `editable_buffer` — provides the basic TextBuffer with cursor and mutations
- `glyph_rendering` — not directly required but part of the editor foundation
- `metal_surface` — not directly required but part of the editor foundation
- `viewport_rendering` — not directly required but part of the editor foundation

The primary dependency is the existing `TextBuffer` implementation in `crates/buffer/src/text_buffer.rs` which provides:
- `Position` type
- `DirtyLines` type
- Gap buffer backing store with `slice` method
- Cursor tracking and movement
- Mutation methods (`insert_char`, `delete_backward`, etc.)

## Risks and Open Questions

1. **Multi-line deletion efficiency**: The plan uses repeated `delete_backward` calls for multi-line selections. This is O(n) in selection size. For large selections (thousands of lines), this may be slow. Mitigation: Accept this for correctness now; the gap buffer design supports efficient batch operations that could be added later if profiling shows a bottleneck.

2. **Dirty line tracking for multi-line deletions**: When deleting a selection spanning lines 5-10, the dirty region should be `FromLineToEnd(5)` since all subsequent lines shift up. Need to ensure this is correctly computed.

3. **Selection and movement interaction with downstream chunks**: The `mouse_drag_selection` chunk expects `set_selection_anchor_at_cursor()` to exist. The `clipboard_operations` chunk expects `selected_text()` and `select_all()`. This chunk provides the foundation they depend on.

## Deviations

<!-- Populate during implementation -->