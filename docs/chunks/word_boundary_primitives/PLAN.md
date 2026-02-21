# Implementation Plan

## Approach

Extract the inline character-class scan logic from `delete_backward_word` into two
private helper functions (`word_boundary_left` and `word_boundary_right`) in
`crates/buffer/src/text_buffer.rs`. Following TDD (per TESTING_PHILOSOPHY.md),
write failing tests for each helper before implementation, then refactor
`delete_backward_word` to use the new `word_boundary_left` helper.

The word model is simple: `char::is_whitespace()` is the sole classifier. A "run"
is a maximal contiguous sequence of same-class characters. The helpers operate on
a `&[char]` slice and a column index, returning the boundary column. This keeps
them pure, stateless, and independently testable — no buffer access, no cursor
state.

## Sequence

### Step 1: Write failing tests for `word_boundary_left`

Create tests covering all cases from the success criteria:
- Empty slice → returns `col`
- `col == 0` → returns 0
- Single-character run (both whitespace and non-whitespace)
- Full-line run of one class
- Non-whitespace run surrounded by whitespace (cursor mid-run)
- Whitespace run surrounded by non-whitespace (cursor mid-run)
- `col` at end of slice (boundary condition)

Since the function doesn't exist yet, the tests will fail to compile. This is the
TDD "red" phase.

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)] mod tests` block

### Step 2: Implement `word_boundary_left`

Add the private function:

```rust
// Spec: docs/trunk/SPEC.md#word-model
fn word_boundary_left(chars: &[char], col: usize) -> usize {
    if col == 0 || chars.is_empty() {
        return col;
    }
    let col = col.min(chars.len());  // clamp to valid range
    let target_is_whitespace = chars[col - 1].is_whitespace();
    let mut boundary = col;
    while boundary > 0 {
        if chars[boundary - 1].is_whitespace() != target_is_whitespace {
            break;
        }
        boundary -= 1;
    }
    boundary
}
```

All tests from Step 1 should now pass.

Location: `crates/buffer/src/text_buffer.rs`, before the `impl TextBuffer` block

### Step 3: Write failing tests for `word_boundary_right`

Create tests mirroring Step 1's coverage but for rightward scanning:
- Empty slice → returns `col`
- `col >= chars.len()` → returns `col`
- Single-character run
- Full-line run of one class
- Non-whitespace run surrounded by whitespace
- Whitespace run surrounded by non-whitespace
- `col` at start of slice

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)] mod tests` block

### Step 4: Implement `word_boundary_right`

Add the private function:

```rust
// Spec: docs/trunk/SPEC.md#word-model
fn word_boundary_right(chars: &[char], col: usize) -> usize {
    if col >= chars.len() || chars.is_empty() {
        return col;
    }
    let target_is_whitespace = chars[col].is_whitespace();
    let mut boundary = col;
    while boundary < chars.len() {
        if chars[boundary].is_whitespace() != target_is_whitespace {
            break;
        }
        boundary += 1;
    }
    boundary
}
```

All tests from Step 3 should now pass.

Location: `crates/buffer/src/text_buffer.rs`, immediately after `word_boundary_left`

### Step 5: Refactor `delete_backward_word` to use `word_boundary_left`

Replace the inline scan loop in `delete_backward_word` (lines 599-615 in the
current implementation) with a call to `word_boundary_left`:

```rust
let word_start = word_boundary_left(&line_chars, self.cursor.col);
let chars_to_delete = self.cursor.col - word_start;
```

Remove the now-unused `char_before` and `delete_whitespace` variables.

Verify that all existing `delete_backward_word` tests pass unchanged.

Location: `crates/buffer/src/text_buffer.rs`, `delete_backward_word` method

### Step 6: Add chunk backreference and spec comment

Add a chunk backreference comment above `word_boundary_left`:

```rust
// Chunk: docs/chunks/word_boundary_primitives - Word boundary scanning primitives
// Spec: docs/trunk/SPEC.md#word-model
fn word_boundary_left(chars: &[char], col: usize) -> usize { ... }
```

And similarly for `word_boundary_right`.

### Step 7: Run full test suite and verify

Run `cargo test` in the `crates/buffer` directory. All tests (both the new helper
tests and the existing `delete_backward_word` tests) should pass.

## Risks and Open Questions

- **SPEC.md word-model section**: The GOAL.md references `docs/trunk/SPEC.md#word-model`
  but that section doesn't exist yet in SPEC.md (it's a template). The helpers will
  carry the comment pointing to where the spec *should* define the word model.
  A future chunk may need to populate that section.

- **Unicode edge cases**: Using `char::is_whitespace()` handles Unicode whitespace
  correctly, but multi-codepoint grapheme clusters are not considered. This matches
  the current behaviour and is consistent with character-level cursor movement.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->