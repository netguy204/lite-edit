# Implementation Plan

## Approach

Wire Alt+Left and Alt+Right word-jump navigation through the existing Command dispatch
pipeline, building on the `word_boundary_left` and `word_boundary_right` helpers
extracted in `word_boundary_primitives`.

The implementation has three layers:

1. **Buffer layer**: Add `move_word_left` and `move_word_right` methods to `TextBuffer`
   in `crates/buffer/src/text_buffer.rs`. These methods implement the navigation
   special case from the word model: if the cursor is on whitespace or at the near
   edge of a run, the jump continues past the adjacent non-whitespace run.

2. **Command layer**: Add `MoveWordLeft` and `MoveWordRight` variants to the `Command`
   enum in `crates/editor/src/buffer_target.rs`.

3. **Keybinding layer**: Wire `Option+Left` → `MoveWordLeft` and `Option+Right` →
   `MoveWordRight` in `resolve_command`, before the plain `Left` / `Right` arms.

Following TDD (per TESTING_PHILOSOPHY.md), write failing tests for the buffer methods
before implementation. The existing `word_boundary_primitives` chunk provides the scan
helpers, so this chunk focuses on the navigation special case logic and command wiring.

## Sequence

### Step 1: Write failing tests for `move_word_right`

Create tests in `crates/buffer/src/text_buffer.rs` covering all success criteria cases:

- Cursor mid-word → lands at word end (right edge of non-whitespace run)
- Cursor at word start → lands at same word's end
- Cursor at word end → jumps past whitespace to next word's end
- Cursor on whitespace between words → jumps to end of next non-whitespace run
- Cursor at line start → jumps to end of first word
- Cursor at line end → stays at line end (no-op)
- Empty line → stays at column 0 (no-op)
- Single-character word → lands at column 1

Also verify that all cases clear any active selection (consistent with `move_right`).

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)] mod tests` block

### Step 2: Implement `move_word_right`

Add the method to `impl TextBuffer`:

```rust
// Chunk: docs/chunks/word_jump_navigation - Word jump navigation
// Spec: docs/trunk/SPEC.md#word-model
/// Moves the cursor to the right edge of the current word.
///
/// If the cursor is on whitespace, or at the right edge of a non-whitespace run,
/// the jump continues past the whitespace to the end of the next non-whitespace run.
/// Stops at line end. Clears any active selection.
pub fn move_word_right(&mut self) {
    self.clear_selection();

    let line_content = self.line_content(self.cursor.line);
    let line_chars: Vec<char> = line_content.chars().collect();

    if self.cursor.col >= line_chars.len() {
        // At line end, no-op
        return;
    }

    // Get the right edge of the run at the cursor
    let boundary = word_boundary_right(&line_chars, self.cursor.col);

    // If we landed on whitespace (or were already there), skip to end of next word
    if boundary < line_chars.len() && line_chars[boundary - 1].is_whitespace() {
        // We're at the end of a whitespace run; skip the following non-whitespace
        let next_boundary = word_boundary_right(&line_chars, boundary);
        self.cursor.col = next_boundary;
    } else if self.cursor.col < line_chars.len() && line_chars[self.cursor.col].is_whitespace() {
        // Started on whitespace, skip past it and to end of following word
        let past_whitespace = word_boundary_right(&line_chars, self.cursor.col);
        if past_whitespace < line_chars.len() && !line_chars[past_whitespace].is_whitespace() {
            // Now on a word, get its end
            let word_end = word_boundary_right(&line_chars, past_whitespace);
            self.cursor.col = word_end;
        } else {
            self.cursor.col = past_whitespace;
        }
    } else {
        self.cursor.col = boundary;
    }
}
```

Note: The exact logic will need refinement during implementation. The key insight is:
- If cursor is on non-whitespace mid-word → `word_boundary_right` gives the word end
- If cursor is on whitespace → skip whitespace, then skip the following word
- If cursor is at word end (next char is whitespace) → skip whitespace, then skip word

All tests from Step 1 should now pass.

Location: `crates/buffer/src/text_buffer.rs`, in `impl TextBuffer` near other movement methods

### Step 3: Write failing tests for `move_word_left`

Create tests covering:

- Cursor mid-word → lands at word start (left edge of non-whitespace run)
- Cursor at word end → lands at same word's start
- Cursor at word start → jumps past preceding whitespace to previous word's start
- Cursor on whitespace between words → jumps to start of preceding non-whitespace run
- Cursor at line start → stays at column 0 (no-op)
- Cursor at line end → jumps to start of last word
- Empty line → stays at column 0 (no-op)
- Single-character word → lands at column 0

Also verify that all cases clear any active selection.

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)] mod tests` block

### Step 4: Implement `move_word_left`

Add the method to `impl TextBuffer`:

```rust
// Chunk: docs/chunks/word_jump_navigation - Word jump navigation
// Spec: docs/trunk/SPEC.md#word-model
/// Moves the cursor to the left edge of the current word.
///
/// If the cursor is on whitespace, or at the left edge of a non-whitespace run,
/// the jump continues past the whitespace to the start of the preceding non-whitespace run.
/// Stops at column 0. Clears any active selection.
pub fn move_word_left(&mut self) {
    self.clear_selection();

    if self.cursor.col == 0 {
        // At line start, no-op
        return;
    }

    let line_content = self.line_content(self.cursor.line);
    let line_chars: Vec<char> = line_content.chars().collect();

    // Get the left edge of the run containing the character before cursor
    let boundary = word_boundary_left(&line_chars, self.cursor.col);

    // If we landed at the start of a whitespace run, continue to previous word
    if boundary > 0 && boundary < self.cursor.col {
        // Check if we were on whitespace
        if line_chars[self.cursor.col - 1].is_whitespace() && boundary > 0 {
            // We skipped whitespace, now skip the preceding word
            let prev_boundary = word_boundary_left(&line_chars, boundary);
            self.cursor.col = prev_boundary;
        } else {
            self.cursor.col = boundary;
        }
    } else if boundary == 0 && self.cursor.col > 0 && line_chars[self.cursor.col - 1].is_whitespace() {
        // Edge case: whitespace run at line start
        self.cursor.col = 0;
    } else {
        self.cursor.col = boundary;
    }
}
```

Note: As with `move_word_right`, the logic may need refinement during implementation.
The key is: skip the current run, and if it was whitespace, skip the preceding word too.

All tests from Step 3 should now pass.

Location: `crates/buffer/src/text_buffer.rs`, in `impl TextBuffer` near `move_word_right`

### Step 5: Add `MoveWordLeft` and `MoveWordRight` to `Command` enum

Add the new variants to the `Command` enum:

```rust
// Chunk: docs/chunks/word_jump_navigation - Word jump navigation
/// Move cursor left by one word (Option+Left)
MoveWordLeft,
/// Move cursor right by one word (Option+Right)
MoveWordRight,
```

Location: `crates/editor/src/buffer_target.rs`, in the `Command` enum after `MoveToBufferEnd`

### Step 6: Wire `Option+Left` and `Option+Right` in `resolve_command`

Add keybindings before the plain `Left` / `Right` arms:

```rust
// Chunk: docs/chunks/word_jump_navigation - Word jump navigation
// Option+Left → move word left (must come before plain Left)
Key::Left if mods.option && !mods.command && !mods.shift => Some(Command::MoveWordLeft),

// Option+Right → move word right (must come before plain Right)
Key::Right if mods.option && !mods.command && !mods.shift => Some(Command::MoveWordRight),
```

These must appear before the existing `Key::Left if !mods.command` and
`Key::Right if !mods.command` arms to take priority when Option is held.

Location: `crates/editor/src/buffer_target.rs`, in `resolve_command` after the
`=== Movement commands (no Shift) ===` comment and before plain arrow handling

### Step 7: Handle `MoveWordLeft` and `MoveWordRight` in `execute_command`

Add the execution handlers following the pattern of other movement commands:

```rust
Command::MoveWordLeft => {
    ctx.buffer.move_word_left();
    ctx.mark_cursor_dirty();
    ctx.ensure_cursor_visible();
    return;
}
Command::MoveWordRight => {
    ctx.buffer.move_word_right();
    ctx.mark_cursor_dirty();
    ctx.ensure_cursor_visible();
    return;
}
```

Location: `crates/editor/src/buffer_target.rs`, in `execute_command` after
`MoveToBufferEnd` handling

### Step 8: Run full test suite

Run `cargo test` in both `crates/buffer` and `crates/editor` directories.
All new tests should pass, and no existing tests should regress.

## Dependencies

- **word_boundary_primitives**: This chunk depends on the `word_boundary_left` and
  `word_boundary_right` helper functions from the `word_boundary_primitives` chunk.
  Per the chunk's GOAL.md frontmatter, this dependency is already declared.

## Risks and Open Questions

- **SPEC.md word-model section**: The GOAL.md references `docs/trunk/SPEC.md#word-model`
  but that section doesn't exist yet (SPEC.md is still a template). The methods will
  carry the comment pointing to where the spec *should* define the word model,
  consistent with the approach taken in `word_boundary_primitives`.

- **Navigation special case complexity**: The "skip past whitespace to the next word"
  logic is non-trivial. The implementation in Steps 2 and 4 is a starting point;
  the actual logic may need adjustment based on test failures. The key invariant:
  the cursor should never land in the middle of a whitespace run (except at line
  boundaries).

- **Consistency with macOS**: The word model uses `char::is_whitespace()` as the sole
  classifier, which differs from macOS TextEdit (which considers punctuation as word
  separators). This is a deliberate simplification per the narrative's scope. Users
  with muscle memory from TextEdit may notice slight differences.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->
