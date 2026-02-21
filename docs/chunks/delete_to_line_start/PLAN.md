# Implementation Plan

## Approach

This chunk adds Cmd+Backspace to delete from the cursor to the beginning of the current line, following the established patterns for deletion commands in lite-edit.

The implementation mirrors the existing `delete_to_line_end` method in `TextBuffer` and the `DeleteToLineEnd` command wiring in `buffer_target.rs`. The key difference is:
- `delete_to_line_end`: deletes characters *after* the cursor (forward deletion)
- `delete_to_line_start`: deletes characters *before* the cursor (backward deletion), placing cursor at column 0

Following the testing philosophy in `docs/trunk/TESTING_PHILOSOPHY.md`, we will use TDD:
1. Write failing tests for `delete_to_line_start` behavior
2. Implement the method to make tests pass
3. Wire through the command enum and key resolution

The architecture is:
- **TextBuffer** (buffer crate): Pure Rust state mutation, fully testable
- **Command enum + resolve_command** (editor crate): Stateless mapping from key events to commands
- **execute_command** (editor crate): Dispatches commands to buffer methods

## Sequence

### Step 1: Add failing tests for delete_to_line_start

Add unit tests to `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)]` module. Tests should cover all success criteria from GOAL.md:

1. **delete_to_line_start_from_middle**: Cursor at col 5 in `"hello world"` → deletes `"hello"`, leaves `" world"` with cursor at col 0
2. **delete_to_line_start_from_end**: Cursor at end of `"hello world"` (col 11) → deletes entire line content, leaves `""` with cursor at col 0
3. **delete_to_line_start_at_col_0**: Cursor at col 0 on line 0 → no-op, returns `DirtyLines::None`
4. **delete_to_line_start_with_selection**: Active selection → deletes selection (not line-start), delegates to `delete_selection()`
5. **delete_to_line_start_multiline**: In a multi-line buffer, cursor mid-line → only affects current line content, does not join
6. **delete_to_line_start_at_col_0_joins_prev_line**: Cursor at col 0 on line > 0 → deletes the preceding newline, joining with the previous line; cursor moves to `(prev_line, prev_line_len)`; returns `DirtyLines::FromLineToEnd(prev_line)`
7. **delete_to_line_start_empty_line_joins**: Cursor at col 0 on an empty intermediate line → joins with the previous line, line disappears

Location: `crates/buffer/src/text_buffer.rs`

### Step 2: Implement delete_to_line_start in TextBuffer

Update the `delete_to_line_start` method in `TextBuffer`:

1. If there's an active selection, delegate to `delete_selection()` and return its result.
2. If cursor is at column 0 **and** on line 0 → return `DirtyLines::None` (no-op; already at buffer start).
3. If cursor is at column 0 **and** on line > 0 → join with the previous line:
   - Record `prev_line = cursor.line - 1` and `prev_line_len = self.line_len(prev_line)`
   - Call `self.sync_gap_to_cursor()` then `self.buffer.delete_backward()` (deletes the `\n`)
   - Call `self.line_index.remove_newline(prev_line)`
   - Set `cursor.line = prev_line`, `cursor.col = prev_line_len`
   - Return `DirtyLines::FromLineToEnd(prev_line)`
4. Otherwise (cursor mid-line):
   - Calculate `chars_to_delete = cursor.col`
   - Call `self.sync_gap_to_cursor()`
   - Delete backward `chars_to_delete` times, calling `self.line_index.remove_char(current_line)` each time
   - Set `cursor.col = 0`
   - Return `DirtyLines::Single(cursor.line)`

The method signature is unchanged: `pub fn delete_to_line_start(&mut self) -> DirtyLines`

Location: `crates/buffer/src/text_buffer.rs`

### Step 3: Add DeleteToLineStart command variant

Add a new variant to the `Command` enum:

```rust
/// Delete from cursor to start of line (Cmd+Backspace)
DeleteToLineStart,
```

Location: `crates/editor/src/buffer_target.rs`, in the `Command` enum

### Step 4: Add key binding in resolve_command

Add a match arm for Cmd+Backspace in the `resolve_command` function:

```rust
// Cmd+Backspace → delete to line start
Key::Backspace if mods.command && !mods.control => Some(Command::DeleteToLineStart),
```

This must be placed **before** the generic `Key::Backspace => Some(Command::DeleteBackward)` match arm to take precedence.

Location: `crates/editor/src/buffer_target.rs`, in `resolve_command()`

### Step 5: Add execution wiring in execute_command

Add a match arm in `execute_command` for the new command:

```rust
Command::DeleteToLineStart => ctx.buffer.delete_to_line_start(),
```

This follows the same pattern as `DeleteToLineEnd` — the method returns `DirtyLines` which is then passed to `ctx.mark_dirty()`.

Location: `crates/editor/src/buffer_target.rs`, in `execute_command()`

### Step 6: Verify tests pass and existing behavior unchanged

Run the full test suite to ensure:
1. All new `delete_to_line_start` tests pass
2. Existing `delete_backward` tests still pass (plain Backspace unchanged)
3. Existing `move_to_line_start` tests still pass (Cmd+Left unchanged)

Command: `cargo test --package lite-edit-buffer`

## Dependencies

None. This chunk builds on the existing buffer and command infrastructure.

## Risks and Open Questions

- **Key binding conflict**: Need to verify that Cmd+Backspace doesn't conflict with any existing binding. Current code shows `Key::Backspace` without modifier checks maps to `DeleteBackward`, so adding a `mods.command` guard should not break anything.

## Deviations

<!-- Populated during implementation -->
