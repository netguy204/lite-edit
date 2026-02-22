<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk refines the word boundary character classification from a two-class model
(whitespace vs non-whitespace) to a three-class model: **Whitespace**, **Letter**
(`a-zA-Z0-9_`), and **Symbol** (all other non-whitespace characters). This aligns
lite-edit's word navigation with VS Code, Xcode, and Sublime Text behavior.

The implementation strategy is to:

1. Introduce a `CharClass` enum with three variants and a `char_class(c: char)`
   classification function
2. Update the existing `word_boundary_left` and `word_boundary_right` helpers to
   compare `CharClass` values instead of `is_whitespace()` boolean results
3. All call sites (`delete_backward_word`, `delete_forward_word`, `move_word_left`,
   `move_word_right`, `select_word_at`) automatically inherit the new behavior
   because they delegate to these helpers

Following TDD per `docs/trunk/TESTING_PHILOSOPHY.md`:
- Write failing tests for the new `CharClass` behavior first
- Implement `CharClass` and `char_class`
- Update the boundary helpers
- Verify all existing tests are updated to match new expected behavior
- Add new tests for class transitions

Location: `crates/buffer/src/text_buffer.rs`

## Sequence

### Step 1: Write failing tests for `CharClass` and `char_class`

Add unit tests in the `#[cfg(test)]` module for the `char_class` function:

```rust
#[test]
fn test_char_class_whitespace() {
    assert_eq!(char_class(' '), CharClass::Whitespace);
    assert_eq!(char_class('\t'), CharClass::Whitespace);
    assert_eq!(char_class('\n'), CharClass::Whitespace);
    assert_eq!(char_class('\r'), CharClass::Whitespace);
}

#[test]
fn test_char_class_letter_lowercase() {
    for c in 'a'..='z' {
        assert_eq!(char_class(c), CharClass::Letter);
    }
}

#[test]
fn test_char_class_letter_uppercase() {
    for c in 'A'..='Z' {
        assert_eq!(char_class(c), CharClass::Letter);
    }
}

#[test]
fn test_char_class_letter_digits() {
    for c in '0'..='9' {
        assert_eq!(char_class(c), CharClass::Letter);
    }
}

#[test]
fn test_char_class_letter_underscore() {
    assert_eq!(char_class('_'), CharClass::Letter);
}

#[test]
fn test_char_class_symbol() {
    // Common programming symbols
    for c in ['.', '+', '-', '*', '/', '(', ')', '{', '}', '[', ']', ':', ';', '"', '\'', '!', '@', '#', '$', '%', '^', '&', '=', '<', '>', '?', '|', '\\', '`', '~', ','] {
        assert_eq!(char_class(c), CharClass::Symbol, "Expected Symbol for '{}'", c);
    }
}
```

These tests will fail to compile until `CharClass` and `char_class` are defined.

Location: `crates/buffer/src/text_buffer.rs` in the `#[cfg(test)]` module

### Step 2: Implement `CharClass` enum and `char_class` function

Add the `CharClass` enum and classification function immediately after the imports,
before the existing `word_boundary_left` function:

```rust
// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Character classification for word boundary detection.
///
/// A "word" is a contiguous run of same-class characters. Boundary detection
/// stops when the character class changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    /// Whitespace characters (space, tab, newline, etc.)
    Whitespace,
    /// Letters: a-z, A-Z, 0-9, underscore
    Letter,
    /// Symbols: everything else (punctuation, operators, etc.)
    Symbol,
}

// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Classifies a character into one of three classes for word boundary detection.
///
/// - `Whitespace`: Any character where `char::is_whitespace()` returns true
/// - `Letter`: ASCII letters (a-z, A-Z), digits (0-9), underscore (_)
/// - `Symbol`: Everything else (punctuation, operators, etc.)
fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_ascii_alphanumeric() || c == '_' {
        CharClass::Letter
    } else {
        CharClass::Symbol
    }
}
```

Verify the Step 1 tests pass.

### Step 3: Write failing tests for triclass word boundary behavior

Before updating the boundary helpers, add tests that verify the new triclass behavior.
These tests will initially fail because the current implementation uses two-class
(whitespace/non-whitespace) classification.

```rust
// ==================== Triclass Boundary Tests ====================
// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification

#[test]
fn test_word_boundary_left_letter_symbol_transition() {
    // "foo.bar" - cursor after 'r' at col 7 → boundary at col 4 (start of "bar")
    let chars: Vec<char> = "foo.bar".chars().collect();
    assert_eq!(word_boundary_left(&chars, 7), 4);
}

#[test]
fn test_word_boundary_left_symbol_letter_transition() {
    // "foo.bar" - cursor after '.' at col 4 → boundary at col 3 (start of ".")
    let chars: Vec<char> = "foo.bar".chars().collect();
    assert_eq!(word_boundary_left(&chars, 4), 3);
}

#[test]
fn test_word_boundary_left_symbol_run() {
    // "..abc" - cursor after second '.' at col 2 → boundary at col 0
    let chars: Vec<char> = "..abc".chars().collect();
    assert_eq!(word_boundary_left(&chars, 2), 0);
}

#[test]
fn test_word_boundary_left_mixed_operators() {
    // "result+=value" - cursor after '=' at col 8 → boundary at col 6 (start of "+=")
    let chars: Vec<char> = "result+=value".chars().collect();
    assert_eq!(word_boundary_left(&chars, 8), 6);
}

#[test]
fn test_word_boundary_left_underscore_as_letter() {
    // "my_var" - cursor at col 6 → boundary at col 0 (underscore is a letter)
    let chars: Vec<char> = "my_var".chars().collect();
    assert_eq!(word_boundary_left(&chars, 6), 0);
}

#[test]
fn test_word_boundary_left_digits_as_letter() {
    // "x42" - cursor at col 3 → boundary at col 0 (digits are letters)
    let chars: Vec<char> = "x42".chars().collect();
    assert_eq!(word_boundary_left(&chars, 3), 0);
}

#[test]
fn test_word_boundary_right_letter_symbol_transition() {
    // "foo.bar" - cursor at col 0 → boundary at col 3 (end of "foo")
    let chars: Vec<char> = "foo.bar".chars().collect();
    assert_eq!(word_boundary_right(&chars, 0), 3);
}

#[test]
fn test_word_boundary_right_symbol_letter_transition() {
    // "foo.bar" - cursor at col 3 → boundary at col 4 (end of ".")
    let chars: Vec<char> = "foo.bar".chars().collect();
    assert_eq!(word_boundary_right(&chars, 3), 4);
}

#[test]
fn test_word_boundary_right_symbol_run() {
    // "fn(x)" - cursor at col 2 → boundary at col 3 (end of "(")
    let chars: Vec<char> = "fn(x)".chars().collect();
    assert_eq!(word_boundary_right(&chars, 2), 3);
}

#[test]
fn test_word_boundary_right_mixed_expression() {
    // "fn(x) + y" - cursor at col 6 → boundary at col 7 (end of "+")
    let chars: Vec<char> = "fn(x) + y".chars().collect();
    assert_eq!(word_boundary_right(&chars, 6), 7);
}

#[test]
fn test_word_boundary_right_underscore_as_letter() {
    // "_foo" - cursor at col 0 → boundary at col 4 (underscore is a letter)
    let chars: Vec<char> = "_foo".chars().collect();
    assert_eq!(word_boundary_right(&chars, 0), 4);
}

#[test]
fn test_word_boundary_right_digits_as_letter() {
    // "var123" - cursor at col 0 → boundary at col 6 (digits are letters)
    let chars: Vec<char> = "var123".chars().collect();
    assert_eq!(word_boundary_right(&chars, 0), 6);
}
```

### Step 4: Update `word_boundary_left` to use `char_class`

Modify `word_boundary_left` to compare `CharClass` values instead of boolean
`is_whitespace()` results:

```rust
// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Returns the start column of the contiguous run containing `chars[col - 1]`.
///
/// Uses the three-class model (Whitespace, Letter, Symbol). A "run" is a maximal
/// contiguous sequence of same-class characters.
///
/// Returns `col` unchanged when `col == 0` or `chars` is empty.
fn word_boundary_left(chars: &[char], col: usize) -> usize {
    if col == 0 || chars.is_empty() {
        return col;
    }
    let col = col.min(chars.len()); // clamp to valid range
    let target_class = char_class(chars[col - 1]);
    let mut boundary = col;
    while boundary > 0 {
        if char_class(chars[boundary - 1]) != target_class {
            break;
        }
        boundary -= 1;
    }
    boundary
}
```

### Step 5: Update `word_boundary_right` to use `char_class`

Modify `word_boundary_right` similarly:

```rust
// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Returns the first column past the end of the contiguous run starting at `chars[col]`.
///
/// Uses the three-class model (Whitespace, Letter, Symbol). A "run" is a maximal
/// contiguous sequence of same-class characters.
///
/// Returns `col` unchanged when `col >= chars.len()` or `chars` is empty.
fn word_boundary_right(chars: &[char], col: usize) -> usize {
    if col >= chars.len() || chars.is_empty() {
        return col;
    }
    let target_class = char_class(chars[col]);
    let mut boundary = col;
    while boundary < chars.len() {
        if char_class(chars[boundary]) != target_class {
            break;
        }
        boundary += 1;
    }
    boundary
}
```

### Step 6: Update existing unit tests for new classification

Update the existing `word_boundary_*` tests that now have different expected behavior
due to the triclass model. Key tests to update:

1. **`test_word_boundary_left_full_line_non_whitespace`**: "hello" is all letters,
   behavior unchanged (still returns 0)

2. **`test_word_boundary_left_non_whitespace_surrounded_by_whitespace`**: "  hello  "
   behavior unchanged (letters surrounded by whitespace)

3. **`test_word_boundary_right_non_whitespace_surrounded_by_whitespace`**: "  hello  "
   behavior unchanged (letters surrounded by whitespace)

Most existing tests should pass unchanged because they use simple letter-only or
whitespace-only sequences. Tests that mix letters and symbols may need updating.

### Step 7: Update `move_word_left` and `move_word_right` for new classification

The `move_word_*` methods currently check `is_whitespace()` to decide whether to
skip whitespace before jumping. With triclass, the logic becomes:

- **move_word_right**: If on whitespace, skip to next non-whitespace boundary.
  With triclass, this is unchanged—we still skip whitespace runs.
- **move_word_left**: Same logic—skip whitespace runs to reach preceding word.

The existing implementation should work correctly because:
- It calls `word_boundary_right/left` which now use triclass
- It checks `is_whitespace()` to detect whitespace runs, which is correct—whitespace
  is still a distinct class

However, we need to update the check from `is_whitespace()` to `char_class() == Whitespace`
for consistency:

```rust
// In move_word_right:
let cursor_on_whitespace = char_class(line_chars[self.cursor.col]) == CharClass::Whitespace;

// In move_word_left:
let prev_char_is_whitespace = char_class(line_chars[self.cursor.col - 1]) == CharClass::Whitespace;
```

### Step 8: Add integration tests for word operations with triclass

Add tests that verify the high-level word operations work correctly with mixed
letter/symbol sequences:

```rust
#[test]
fn test_delete_backward_word_stops_at_symbol() {
    // "foo.bar" with cursor at col 7 → deletes "bar" → "foo."
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 7));
    buf.delete_backward_word();
    assert_eq!(buf.content(), "foo.");
    assert_eq!(buf.cursor_position(), Position::new(0, 4));
}

#[test]
fn test_delete_backward_word_deletes_symbol_run() {
    // "foo.bar" with cursor at col 4 → deletes "." → "foobar"
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 4));
    buf.delete_backward_word();
    assert_eq!(buf.content(), "foobar");
    assert_eq!(buf.cursor_position(), Position::new(0, 3));
}

#[test]
fn test_delete_forward_word_stops_at_symbol() {
    // "foo.bar" with cursor at col 0 → deletes "foo" → ".bar"
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 0));
    buf.delete_forward_word();
    assert_eq!(buf.content(), ".bar");
    assert_eq!(buf.cursor_position(), Position::new(0, 0));
}

#[test]
fn test_move_word_right_stops_at_symbol() {
    // "foo.bar" with cursor at col 0 → moves to col 3 (end of "foo")
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 0));
    buf.move_word_right();
    assert_eq!(buf.cursor_position(), Position::new(0, 3));
}

#[test]
fn test_move_word_left_stops_at_symbol() {
    // "foo.bar" with cursor at col 7 → moves to col 4 (start of "bar")
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 7));
    buf.move_word_left();
    assert_eq!(buf.cursor_position(), Position::new(0, 4));
}

#[test]
fn test_select_word_at_selects_letter_only() {
    // "foo.bar" double-click on 'b' at col 4 → selects "bar" only
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 4));
    buf.select_word_at(4);
    assert_eq!(buf.selected_text(), Some("bar".to_string()));
}

#[test]
fn test_select_word_at_selects_symbol_only() {
    // "foo.bar" double-click on '.' at col 3 → selects "." only
    let mut buf = TextBuffer::from_str("foo.bar");
    buf.set_cursor(Position::new(0, 3));
    buf.select_word_at(3);
    assert_eq!(buf.selected_text(), Some(".".to_string()));
}

#[test]
fn test_underscore_included_in_word() {
    // "my_var" with cursor at col 6 → delete backward deletes entire "my_var"
    let mut buf = TextBuffer::from_str("my_var");
    buf.set_cursor(Position::new(0, 6));
    buf.delete_backward_word();
    assert_eq!(buf.content(), "");
}

#[test]
fn test_digits_included_in_word() {
    // "x42" with cursor at col 3 → delete backward deletes entire "x42"
    let mut buf = TextBuffer::from_str("x42");
    buf.set_cursor(Position::new(0, 3));
    buf.delete_backward_word();
    assert_eq!(buf.content(), "");
}
```

### Step 9: Update backreference comments

Update the backreference comments on `word_boundary_left` and `word_boundary_right`
to reference this chunk instead of (or in addition to) `word_boundary_primitives`:

```rust
// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
```

### Step 10: Run full test suite and verify

Run `cargo test -p buffer` to verify:
1. All new `CharClass` tests pass
2. All new triclass boundary tests pass
3. All integration tests for word operations pass
4. No regressions in unrelated tests

## Dependencies

- **word_boundary_primitives**: This chunk depends on the existence of
  `word_boundary_left` and `word_boundary_right` from the `word_boundary_primitives`
  chunk. That chunk must be complete (ACTIVE) before this chunk can be implemented.

## Risks and Open Questions

- **Existing test expectations**: Some existing tests for `move_word_*` and
  `delete_*_word` may have expectations based on two-class behavior. These need
  to be audited and updated to reflect triclass behavior. The plan accounts for
  this in Steps 6 and 8.

- **Performance**: The `char_class` function adds a branch compared to
  `is_whitespace()`. This is unlikely to be measurable—word boundary scanning
  is already O(word length) and words are typically short.

- **Unicode considerations**: The current classification uses `is_ascii_alphanumeric()`,
  which excludes non-ASCII letters (é, ñ, 中, etc.). These will be classified as
  `Symbol`. This matches VS Code's default word separator behavior but may surprise
  users editing non-English text. A future enhancement could use `is_alphabetic()`
  for full Unicode letter support, but that's out of scope for this chunk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->