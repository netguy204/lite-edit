# Implementation Plan

## Approach

This chunk adds Cmd+K kill-line functionality, following the existing patterns for text mutation operations in TextBuffer and command handling in BufferFocusTarget.

The implementation mirrors how `delete_forward` works but extends it to delete multiple characters at once. The key insight from the GOAL.md is that kill-line has two distinct behaviors:
1. **Cursor mid-line**: Delete all characters from cursor to end of line (cursor stays put, returns `DirtyLines::Single`)
2. **Cursor at end of line**: Delete the newline character, joining with the next line (cursor stays put, returns `DirtyLines::FromLineToEnd`)

This matches Emacs `C-k` behavior and the existing `delete_forward` pattern for newline handling.

**Selection handling**: Per GOAL.md, if a selection is active, clear it first before operating from cursor. This follows Emacs semantics where `C-k` ignores the mark. The text_selection_model chunk (a sibling in the narrative) will add selection support, but our implementation should be forward-compatible — currently there's no selection to clear, but when there is, kill-line should clear it.

**Testing strategy**: Following TESTING_PHILOSOPHY.md, we write tests first that verify the semantic goal (text is deleted correctly, cursor stays put, dirty lines are correct). Tests exercise boundary conditions: middle of line, start of line, end of line (joins), empty line, and end of buffer.

## Subsystem Considerations

No subsystems are relevant. This chunk implements a standalone editing command following established patterns.

## Sequence

### Step 1: Add `delete_to_line_end` tests to TextBuffer

Write failing tests in `crates/buffer/src/text_buffer.rs` that verify the success criteria from GOAL.md:

- **Kill from middle of line**: `"hello world"` with cursor at col 5 → `"hello"`
- **Kill from start of line**: `"hello"` with cursor at col 0 → `""`
- **Kill at end of line** (joins next line): `"hello\nworld"` with cursor at col 5 on line 0 → `"helloworld"`
- **Kill on empty line**: `"\n"` or `"\nfoo"` joins with next line
- **Kill at end of buffer**: no-op, returns `DirtyLines::None`
- **Cursor position unchanged**: same (line, col) after kill
- **Dirty lines correct**: `Single(line)` for within-line, `FromLineToEnd(line)` when newline deleted

Location: `crates/buffer/src/text_buffer.rs` (inline `#[cfg(test)]` module)

### Step 2: Implement `delete_to_line_end` on TextBuffer

Add a new method to TextBuffer that:

1. Calculates the current line length and cursor column
2. If cursor is at line end:
   - If this is the last line, return `DirtyLines::None` (no-op)
   - Otherwise, delete the newline character (similar to `delete_forward` at line end)
   - Update line_index via `remove_newline`
   - Return `DirtyLines::FromLineToEnd(cursor.line)`
3. If cursor is mid-line:
   - Calculate `chars_to_delete = line_len - cursor.col`
   - Call `sync_gap_to_cursor()` once
   - Loop `chars_to_delete` times calling `buffer.delete_forward()` and `line_index.remove_char()`
   - Return `DirtyLines::Single(cursor.line)`
4. Cursor position does NOT change

The implementation reuses existing GapBuffer/LineIndex primitives. No new data structures needed.

Location: `crates/buffer/src/text_buffer.rs`, in the `// ==================== Mutations ====================` section

### Step 3: Add `DeleteToLineEnd` command variant

Add a new variant to the `Command` enum in `buffer_target.rs`:

```rust
/// Delete from cursor to end of line (kill-line)
DeleteToLineEnd,
```

Location: `crates/editor/src/buffer_target.rs`

### Step 4: Add Ctrl+K key binding in `resolve_command`

Map `Key::Char('k')` with `mods.control && !mods.command` to `Command::DeleteToLineEnd`:

```rust
// Ctrl+K → kill line (delete to end of line)
Key::Char('k') if mods.control && !mods.command => Some(Command::DeleteToLineEnd),
```

Place this near other Ctrl+key bindings in the match.

Location: `crates/editor/src/buffer_target.rs`, in `resolve_command`

### Step 5: Handle `DeleteToLineEnd` in `execute_command`

Add the command execution case that:
1. Calls `ctx.buffer.delete_to_line_end()`
2. Marks the returned dirty region
3. Ensures cursor visibility

```rust
Command::DeleteToLineEnd => ctx.buffer.delete_to_line_end(),
```

This follows the same pattern as `DeleteBackward` and `DeleteForward`.

Location: `crates/editor/src/buffer_target.rs`, in `execute_command`

### Step 6: Add integration tests for Ctrl+K via BufferFocusTarget

Add tests in `buffer_target.rs` that:
- Send `KeyEvent` for Ctrl+K
- Verify buffer content after kill
- Verify dirty region marking
- Verify `Handled::Yes` is returned

These tests follow the existing pattern in the file (e.g., `test_typing_hello`, `test_cmd_left_moves_to_line_start`).

Location: `crates/editor/src/buffer_target.rs`, in `#[cfg(test)]` module

### Step 7: Run tests and verify

Run:
```bash
cargo test -p lite-edit-buffer
cargo test -p lite-edit
```

All tests should pass. The failing tests from Step 1 should now pass after Step 2.

## Dependencies

This chunk has no blocking dependencies. The `created_after` frontmatter references are for ordering only — editable_buffer, glyph_rendering, metal_surface, and viewport_rendering are all complete.

The text_selection_model chunk (sibling in the narrative) is NOT a dependency. Kill-line operates on cursor position regardless of selection. When selection support is added later, selection should be cleared before kill-line operates, but that's forward-compatible — currently there's no selection to clear.

## Risks and Open Questions

- **Selection interaction**: GOAL.md says to clear selection first if active. Currently there's no selection model, so this is a no-op. When text_selection_model is implemented, we may need to revisit to add `ctx.buffer.clear_selection()` before the kill. However, the current design is forward-compatible — we operate from cursor position, which is correct whether or not a selection exists.

- **Multi-character delete performance**: We loop calling `delete_forward` for each character. This is O(n) in the number of characters deleted, but each delete is O(1) in the gap buffer. For kill-line on typical line lengths (< 200 chars), this is negligible. If needed later, a batch delete could be added to GapBuffer.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION, not at planning time. -->
