# Implementation Plan

## Approach

This chunk adds Alt+Backspace (Option+Delete) to delete backward by word, following the established patterns for text mutation operations in TextBuffer and command handling in BufferFocusTarget.

The implementation mirrors existing delete operations like `delete_backward` and `delete_to_line_end`, but uses a character-class based word boundary rule:

1. **Non-whitespace class**: If the character immediately before the cursor is non-whitespace, delete backward through contiguous non-whitespace characters until hitting whitespace or line start.
2. **Whitespace class**: If the character immediately before the cursor is whitespace, delete backward through contiguous whitespace characters until hitting non-whitespace or line start.

This is simpler than Unicode word segmentation and matches the behavior users expect from macOS text editors for quick deletion.

**Selection handling**: Per GOAL.md, if a selection is active when Alt+Backspace is pressed, delete the selection instead of performing word deletion. This is consistent with existing delete behavior (`delete_backward`, `delete_forward`) and uses the existing `delete_selection()` method.

**Key binding**: The `option` field on `Modifiers` will be used to detect Alt+Backspace. The resolve_command match will be: `Key::Backspace if mods.option && !mods.command => Some(Command::DeleteBackwardWord)`.

**Testing strategy**: Following TESTING_PHILOSOPHY.md, tests are written first to verify the semantic goal. Tests cover the success criteria from GOAL.md:
- Non-whitespace word deletion
- Whitespace deletion
- No-op at column 0
- Selection deletion takes precedence
- Mid-line word boundary behavior
- Correct DirtyLines return values

## Sequence

### Step 1: Add `delete_backward_word` tests to TextBuffer

Write failing tests in `crates/buffer/src/text_buffer.rs` that verify the success criteria:

- **Delete non-whitespace word**: `"hello world"` with cursor at col 11 → `"hello "` with cursor at col 6
- **Delete whitespace**: `"hello   "` with cursor at col 8 (trailing spaces) → `"hello"` with cursor at col 5
- **No-op at start of line**: cursor at col 0 returns `DirtyLines::None`, buffer unchanged
- **Selection takes precedence**: with active selection, delete selection instead of word
- **Mid-line word boundary**: `"one two three"` with cursor at col 7 (after "two") → `"one  three"` with cursor at col 4

Location: `crates/buffer/src/text_buffer.rs`, in `#[cfg(test)]` module

### Step 2: Implement `delete_backward_word` on TextBuffer

Add a new method to TextBuffer that:

1. If there's an active selection, delegate to `delete_selection()` and return its result
2. If cursor is at column 0, return `DirtyLines::None` (no-op)
3. Get the character immediately before the cursor to determine the character class:
   - Use `line_content()` to get the current line
   - Check if `line[cursor.col - 1]` is whitespace or non-whitespace
4. Scan backward from cursor.col - 1 to find the word start:
   - For non-whitespace class: scan while chars are non-whitespace
   - For whitespace class: scan while chars are whitespace
5. Calculate `chars_to_delete = cursor.col - word_start_col`
6. Call `sync_gap_to_cursor()` once
7. Loop `chars_to_delete` times calling `buffer.delete_backward()` and `line_index.remove_char(cursor.line)`
8. Update `cursor.col -= chars_to_delete`
9. Return `DirtyLines::Single(cursor.line)` (word deletion stays within single line)

Add backreference comment: `// Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion`

Location: `crates/buffer/src/text_buffer.rs`, in the `// ==================== Mutations ====================` section

### Step 3: Add `DeleteBackwardWord` command variant

Add a new variant to the `Command` enum:

```rust
/// Delete backward by one word (Alt+Backspace)
DeleteBackwardWord,
```

Add backreference comment for the variant.

Location: `crates/editor/src/buffer_target.rs`, in `enum Command`

### Step 4: Add Alt+Backspace key binding in `resolve_command`

Add match arm for `Key::Backspace` with `option` modifier:

```rust
// Option+Backspace → delete backward by word
Key::Backspace if mods.option && !mods.command => Some(Command::DeleteBackwardWord),
```

This must come BEFORE the generic `Key::Backspace => Some(Command::DeleteBackward)` match, since Rust matches top-to-bottom. The current generic Backspace match has no guard, so it must remain as the fallback.

Location: `crates/editor/src/buffer_target.rs`, in `resolve_command`, near the existing Backspace handling

### Step 5: Handle `DeleteBackwardWord` in `execute_command`

Add the command execution case:

```rust
Command::DeleteBackwardWord => ctx.buffer.delete_backward_word(),
```

This follows the same pattern as `DeleteBackward`, `DeleteForward`, and `DeleteToLineEnd` — the dirty region is returned from the buffer method and marked via `ctx.mark_dirty()`.

Location: `crates/editor/src/buffer_target.rs`, in `execute_command`

### Step 6: Add integration tests for Alt+Backspace via BufferFocusTarget

Add tests in `buffer_target.rs` that:
- Construct `KeyEvent` with `Key::Backspace` and `modifiers.option = true`
- Send through `BufferFocusTarget::handle_key()`
- Verify buffer content after word deletion
- Verify `Handled::Yes` is returned

These follow the existing test patterns in the file.

Location: `crates/editor/src/buffer_target.rs`, in `#[cfg(test)]` module

### Step 7: Update code_paths in GOAL.md

Update the `code_paths` frontmatter field to list the files touched:
- `crates/buffer/src/text_buffer.rs`
- `crates/editor/src/buffer_target.rs`

Location: `docs/chunks/delete_backward_word/GOAL.md`

### Step 8: Run tests and verify

Run:
```bash
cargo test -p lite-edit-buffer
cargo test -p lite-edit
```

All tests should pass. The failing tests from Step 1 should now pass after Step 2.

## Risks and Open Questions

- **Unicode handling**: The character-class rule (whitespace vs non-whitespace) uses `char::is_whitespace()` which handles Unicode whitespace correctly. Non-whitespace is simply `!is_whitespace()`. This should work correctly for multi-byte UTF-8 characters since Rust's `char` type represents Unicode scalar values.

- **Performance**: We loop calling `delete_backward()` for each character. This is O(n) in the number of characters deleted, but each delete is O(1) in the gap buffer. For word deletion (typically < 20 chars), this is negligible. If needed later, a batch delete could be added to GapBuffer.

- **Empty line edge case**: At column 0, we return no-op. This differs from some editors that join with the previous line on Alt+Backspace at line start. The GOAL.md explicitly says "Alt+Backspace at the start of a line (col 0) is a no-op", so this is the correct behavior.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION, not at planning time. -->
