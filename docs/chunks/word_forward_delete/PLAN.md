# Implementation Plan

## Approach

This chunk adds Alt+D forward word deletion, the forward complement to the existing
Alt+Backspace (`delete_backward_word`). The implementation follows the established
pattern for deletion commands in this codebase:

1. **TextBuffer method** — Add `delete_forward_word()` to `TextBuffer` that uses
   `word_boundary_right` (from `word_boundary_primitives`) to determine the deletion
   range. The method is symmetrical to `delete_backward_word()` but operates forward
   from the cursor position.

2. **Command enum** — Add `DeleteForwardWord` variant to the `Command` enum in
   `buffer_target.rs`.

3. **Key binding** — Wire `Option+'d'` → `DeleteForwardWord` in `resolve_command`,
   placing it before the plain `Key::Char('d')` arm to ensure the modifier is
   checked first.

4. **Execution** — Call `ctx.buffer.delete_forward_word()` in `execute_command`,
   mark dirty, and ensure cursor visible.

The word model is defined in `docs/trunk/SPEC.md#word-model` (whitespace vs
non-whitespace runs). This chunk delegates boundary computation to `word_boundary_right`
rather than reimplementing scan logic.

Per `docs/trunk/TESTING_PHILOSOPHY.md`, tests are goal-driven. Each test verifies
a success criterion from the GOAL.md: cursor mid-word, cursor on whitespace, cursor
at line end, cursor at line start, active selection, whitespace-only line.

## Sequence

### Step 1: Write unit tests for `delete_forward_word` (TDD red phase)

Add tests to the `mod tests` block in `crates/buffer/src/text_buffer.rs` that
exercise the success criteria. These tests will fail initially since the method
does not exist.

**Test cases:**
1. Cursor mid-word on non-whitespace (deletes from cursor to word end)
2. Cursor on whitespace between words (deletes whitespace run only)
3. Cursor at line end (no-op, returns `DirtyLines::None`)
4. Cursor at line start (deletes first run)
5. Active selection (deletes selection, not word)
6. Line containing only whitespace (deletes whitespace run)

Each test follows the pattern of existing `delete_backward_word` tests: construct
buffer, set cursor, call method, assert content/cursor/dirty.

**Location:** `crates/buffer/src/text_buffer.rs` (test module at end of file)

### Step 2: Implement `TextBuffer::delete_forward_word`

Add the method to `TextBuffer` impl block, immediately after `delete_backward_word`.

**Algorithm:**
1. If selection is active, delegate to `delete_selection()` (consistent with all
   other deletion operations)
2. Get line content as `Vec<char>`
3. If `cursor.col >= line_len`, return `DirtyLines::None` (no-op at line end)
4. Call `word_boundary_right(chars, cursor.col)` to find the right edge
5. Compute `chars_to_delete = boundary - cursor.col`
6. Sync gap to cursor, then call `buffer.delete_forward()` in a loop
   (or use `delete_forward` N times with line_index updates)
7. Update line_index via `remove_char` calls
8. Return `DirtyLines::Single(cursor.line)`

**Backreference comments:**
- `// Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion`
- `// Spec: docs/trunk/SPEC.md#word-model`

**Location:** `crates/buffer/src/text_buffer.rs`

### Step 3: Verify tests pass (TDD green phase)

Run `cargo test -p lite-edit-buffer` and confirm all new tests pass. The existing
tests must also remain green.

### Step 4: Add `DeleteForwardWord` to `Command` enum

Add the new variant to the `Command` enum in `crates/editor/src/buffer_target.rs`,
placed near `DeleteBackwardWord` for conceptual grouping.

```rust
// Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
/// Delete forward by one word (Alt+D)
DeleteForwardWord,
```

**Location:** `crates/editor/src/buffer_target.rs` (Command enum around line 36)

### Step 4b: Fix `convert_key` in `metal_view.rs` to use `charactersIgnoringModifiers` for Option

**Root cause of the "inserts whitespace" bug:** On macOS, `event.characters()` for
Option+D returns the *composed* Unicode character — on a US keyboard layout this is
`'ð'` (eth, U+00F0), which the macOS text system treats as a whitespace-adjacent glyph
and renders as something whitespace-like. The `Key::Char('d') if mods.option` arm in
`resolve_command` therefore never fires; instead `InsertChar('ð')` matches first.

The fix mirrors the existing Control modifier handling: when `NSEventModifierFlags::Option`
is set, use `event.charactersIgnoringModifiers()` to recover the base key character `'d'`.

```rust
let characters = if flags.contains(NSEventModifierFlags::Control)
    || flags.contains(NSEventModifierFlags::Option)
{
    event.charactersIgnoringModifiers()?
} else {
    event.characters()?
};
```

**Location:** `crates/editor/src/metal_view.rs` (`convert_key` method, after the
special-key match block)

### Step 5: Wire `Option+'d'` in `resolve_command`

The `Key::Char('d') if mods.option && !mods.command` arm already exists in
`resolve_command`. With the `convert_key` fix in place, Option+D now correctly
arrives as `Key::Char('d')` with `mods.option=true`, and this arm fires as intended.

The arm must appear **before** the generic `Key::Char(ch)` arm (it already does).

```rust
// Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
// Option+D → delete forward by word (must come before generic Char)
Key::Char('d') if mods.option && !mods.command => Some(Command::DeleteForwardWord),
```

**Location:** `crates/editor/src/buffer_target.rs` (resolve_command function)

### Step 6: Execute `DeleteForwardWord` in `execute_command`

Add a match arm in `execute_command` that calls `ctx.buffer.delete_forward_word()`.
Follow the pattern of `DeleteBackwardWord`: return the dirty lines and let the
common tail code call `ctx.mark_dirty(dirty)` and `ctx.ensure_cursor_visible()`.

```rust
// Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
Command::DeleteForwardWord => ctx.buffer.delete_forward_word(),
```

**Location:** `crates/editor/src/buffer_target.rs` (execute_command method)

### Step 7: Run full test suite

Run `cargo test` for the entire workspace to verify no regressions. The key
test targets are:
- `cargo test -p lite-edit-buffer` — unit tests for `delete_forward_word`
- `cargo test -p lite-edit-editor` — integration tests if any

### Step 8: Update GOAL.md code_paths

Update the `code_paths` frontmatter in `docs/chunks/word_forward_delete/GOAL.md`
to list the files touched:
- `crates/buffer/src/text_buffer.rs`
- `crates/editor/src/buffer_target.rs`
- `crates/editor/src/metal_view.rs`

## Dependencies

- **word_boundary_primitives** — This chunk declares `depends_on: [word_boundary_primitives]`
  in the GOAL.md frontmatter. The `word_boundary_right` helper must exist before
  `delete_forward_word` can be implemented.

## Risks and Open Questions

- **None identified.** The implementation is straightforward and follows the exact
  pattern of `delete_backward_word`. The word boundary helper already exists and
  is tested.
