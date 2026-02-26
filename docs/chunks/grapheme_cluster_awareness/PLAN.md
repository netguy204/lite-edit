<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The text buffer (`crates/buffer/src/text_buffer.rs`) currently operates on Rust `char` (Unicode scalar values), but cursor movement, deletion, and selection operations treat each `char` as a single visual unit. This breaks for multi-codepoint grapheme clusters like:

- ZWJ emoji: `👨‍👩‍👧‍👦` (7 chars: 4 codepoints joined by 3 ZWJ)
- Combining marks: `é` (2 chars: `e` + combining acute accent)
- Regional indicators: `🇺🇸` (2 chars: `🇺` + `🇸`)
- Hangul jamo sequences

**Strategy**: Introduce the `unicode-segmentation` crate to detect grapheme cluster boundaries. Add a new internal helper module (`grapheme.rs`) with functions that find grapheme boundaries given a position on a line. Then update all cursor movement and deletion operations to use these helpers instead of moving/deleting by single `char`.

**Key insight**: The buffer stores `char` arrays, but cursor `col` positions should represent grapheme offsets (visual column), not char offsets. However, converting the entire buffer to grapheme-based indexing would be a large change. Instead, we'll keep the char-based storage but make the **operations** grapheme-aware by:

1. For movements: Computing the next/previous grapheme boundary from the current char offset
2. For deletions: Deleting all chars comprising the grapheme cluster before/after cursor

**Existing patterns to follow**:
- The `word_boundary_left`/`word_boundary_right` helpers demonstrate the pattern for char-based boundary detection
- The `delete_backward_word` shows how to delete multiple chars in a loop
- Tests in `editing_sequences.rs` provide a model for integration testing

**TDD approach**: Per TESTING_PHILOSOPHY.md, we'll write failing tests first for each grapheme-aware operation, then implement to make them pass.

## Subsystem Considerations

No existing subsystems are directly relevant to this chunk. The `renderer` and `viewport_scroll` subsystems are downstream consumers that will benefit from grapheme-aware cursor positioning, but this chunk is strictly within the buffer crate and doesn't need to interact with those subsystems.

## Sequence

### Step 1: Add `unicode-segmentation` dependency

Add the `unicode-segmentation` crate to `crates/buffer/Cargo.toml`. This is a pure-Rust, no-std-compatible crate that implements UAX #29 grapheme cluster detection.

Location: `crates/buffer/Cargo.toml`

### Step 2: Create grapheme boundary helper module

Create `crates/buffer/src/grapheme.rs` with helper functions:

```rust
// Chunk: docs/chunks/grapheme_cluster_awareness - Grapheme cluster boundary helpers

/// Returns the char offset of the grapheme cluster boundary immediately before `char_offset`.
/// If `char_offset` is 0 or at the start of a grapheme, returns the start of the previous grapheme.
pub fn grapheme_boundary_left(chars: &[char], char_offset: usize) -> usize;

/// Returns the char offset of the grapheme cluster boundary immediately after `char_offset`.
/// If `char_offset` is at the end or at the start of a grapheme, returns the end of that grapheme.
pub fn grapheme_boundary_right(chars: &[char], char_offset: usize) -> usize;

/// Returns the number of chars in the grapheme cluster ending at `char_offset`.
/// Used by delete_backward to know how many chars to delete.
pub fn grapheme_len_before(chars: &[char], char_offset: usize) -> usize;

/// Returns the number of chars in the grapheme cluster starting at `char_offset`.
/// Used by delete_forward to know how many chars to delete.
pub fn grapheme_len_at(chars: &[char], char_offset: usize) -> usize;
```

The implementation will convert the `&[char]` slice to a `String`, use `UnicodeSegmentation::grapheme_indices()`, and map back to char offsets.

Location: `crates/buffer/src/grapheme.rs`

### Step 3: Write failing tests for grapheme-aware backspace

Add test cases to `crates/buffer/tests/editing_sequences.rs` (or a new `crates/buffer/tests/grapheme.rs`) that verify:

1. Backspace deletes entire ZWJ emoji: `"a👨‍👩‍👧‍👦b"` → backspace at col after emoji → `"ab"`
2. Backspace deletes combining character sequences: `"ae\u{0301}b"` (e + combining acute) → backspace → `"ab"`
3. Backspace deletes regional indicator pairs: `"a🇺🇸b"` → backspace → `"ab"`
4. ASCII behavior unchanged: `"abc"` → backspace → `"ab"`

These tests will fail initially because `delete_backward()` currently deletes only one char.

Location: `crates/buffer/tests/grapheme.rs` (new file)

### Step 4: Update `delete_backward()` to delete by grapheme

Modify `TextBuffer::delete_backward()` in `text_buffer.rs`:

1. Get the current line's chars as a slice
2. Call `grapheme_len_before(chars, cursor.col)` to find how many chars comprise the grapheme
3. Delete that many chars (loop calling `self.buffer.delete_backward()` and updating `line_index`)
4. Update cursor position by the number of chars deleted

The existing `delete_backward()` handles line boundaries (backspace at col 0 joins lines). This behavior stays: if at col 0, the existing newline-join logic applies. The grapheme logic only applies when col > 0.

Location: `crates/buffer/src/text_buffer.rs#delete_backward`

### Step 5: Verify backspace tests pass

Run the grapheme tests from Step 3. All backspace-related tests should now pass.

### Step 6: Write failing tests for grapheme-aware delete forward

Add test cases for forward deletion:

1. Delete key removes entire ZWJ emoji when cursor is before it
2. Delete key removes combining sequences
3. Delete key removes regional indicator pairs
4. ASCII behavior unchanged

Location: `crates/buffer/tests/grapheme.rs`

### Step 7: Update `delete_forward()` to delete by grapheme

Modify `TextBuffer::delete_forward()` similarly to Step 4, using `grapheme_len_at()`.

Location: `crates/buffer/src/text_buffer.rs#delete_forward`

### Step 8: Write failing tests for grapheme-aware cursor movement

Add test cases for arrow key movement:

1. Right arrow moves past entire ZWJ emoji in one keypress
2. Left arrow moves past entire combining sequence in one keypress
3. Up/down arrows preserve column position in grapheme terms (or clamp correctly)

Location: `crates/buffer/tests/grapheme.rs`

### Step 9: Update `move_left()` and `move_right()` for grapheme boundaries

Modify cursor movement in `text_buffer.rs`:

- `move_left()`: Use `grapheme_boundary_left()` to find the target column
- `move_right()`: Use `grapheme_boundary_right()` to find the target column

The existing line-boundary logic (moving to previous/next line) stays unchanged.

Location: `crates/buffer/src/text_buffer.rs#move_left`, `crates/buffer/src/text_buffer.rs#move_right`

### Step 10: Write failing tests for grapheme-aware selection expansion

Add test cases for Shift+Arrow selection:

1. Shift+Right selects entire grapheme cluster
2. Shift+Left selects entire grapheme cluster
3. Double-click word selection respects grapheme boundaries

Note: The current `select_word_at()` uses `word_boundary_left/right` which operate on chars. We need to ensure word boundaries don't split grapheme clusters.

Location: `crates/buffer/tests/grapheme.rs`

### Step 11: Review and update selection-related methods

Methods that may need grapheme awareness:
- `move_cursor_preserving_selection()` — if it's used with arrow keys, the calling code should use grapheme-aware movement
- `select_word_at()` — ensure word boundaries don't fall inside grapheme clusters

The key principle: wherever we compute a cursor column, that column should land on a grapheme boundary, not in the middle of one.

Location: `crates/buffer/src/text_buffer.rs`

### Step 12: Add grapheme module to lib.rs exports

Update `crates/buffer/src/lib.rs` to include the new grapheme module (as private, since helpers are internal).

Location: `crates/buffer/src/lib.rs`

### Step 13: Integration testing with Hangul jamo

Add specific tests for Hangul jamo sequences (composed vs decomposed forms) to ensure the UAX #29 implementation handles Korean text correctly.

Location: `crates/buffer/tests/grapheme.rs`

### Step 14: Final review and cleanup

- Ensure all existing tests still pass (ASCII behavior unchanged)
- Add backreference comments to new/modified code
- Run `cargo test` for the buffer crate
- Run `cargo clippy` to catch any issues

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments to help future agents trace
code back to its governing documentation.

**Valid backreference types:**
- `# Subsystem: docs/subsystems/<name>` - For architectural patterns
- `# Chunk: docs/chunks/<name>` - For implementation work

Place comments at the appropriate level:
- **Module-level**: If this code implements the subsystem/chunk's core functionality
- **Class-level**: If this class is part of the pattern
- **Method-level**: If this method implements a specific behavior

Format (place immediately before the symbol):
```
// Chunk: docs/chunks/grapheme_cluster_awareness - Grapheme cluster boundary helpers
```

## Dependencies

**External library to add:**
- `unicode-segmentation` crate (latest stable, currently 1.10.x) — implements UAX #29 grapheme cluster segmentation. Pure Rust, no-std compatible, well-maintained.

**No chunk dependencies.** This chunk operates entirely within the buffer crate and doesn't depend on other chunks being complete first.

## Risks and Open Questions

1. **Char-to-grapheme offset mapping cost**: Converting a `&[char]` slice to a `String` and iterating with `grapheme_indices()` is O(n) in line length. For very long lines (>10K chars), this could add noticeable latency to each keystroke. Mitigation: Profile after implementation; if problematic, cache grapheme boundaries or limit the scan window.

2. **Column semantics shift**: Currently `cursor.col` is a char offset. After this change, movements will land on grapheme boundaries, but `cursor.col` still stores a char offset. This is intentional (keeps storage and indexing simple), but callers that display column numbers to users may need to convert to "grapheme column" for display. This chunk doesn't address display — only internal operations.

3. **Word boundary interaction**: The existing `word_boundary_left/right` helpers operate on `CharClass` (Whitespace/Letter/Symbol). A grapheme cluster like `é` (e + combining acute) would have the base `e` as Letter and the combining mark classified... actually, combining marks are not alphanumeric, so they'd be Symbol. This could cause word selection to split in the middle of graphemes. Need to verify and potentially adjust `char_class` to treat combining marks as part of the preceding character's class.

4. **Regional indicator edge cases**: Regional indicators come in pairs (`🇺🇸` = `🇺` + `🇸`). If a user somehow has an odd number of regional indicator chars (e.g., from a bad paste), the grapheme segmentation will treat the orphan as its own cluster. This is correct per UAX #29 but may look weird. No mitigation needed — it's the correct behavior.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION, not at planning time. -->