---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/buffer/src/text_buffer.rs
code_references:
  - ref: crates/buffer/src/text_buffer.rs#CharClass
    implements: "Three-class enum (Whitespace, Letter, Symbol) for word boundary detection"
  - ref: crates/buffer/src/text_buffer.rs#char_class
    implements: "Character classification function mapping chars to CharClass"
  - ref: crates/buffer/src/text_buffer.rs#word_boundary_left
    implements: "Left boundary scan using char_class equality comparison"
  - ref: crates/buffer/src/text_buffer.rs#word_boundary_right
    implements: "Right boundary scan using char_class equality comparison"
narrative: word_nav
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- word_boundary_primitives
created_after:
- file_search_path_matching
---

# Chunk Goal

## Minor Goal

Refine the word boundary character classification from a two-class model (whitespace
vs non-whitespace) to a three-class model: **whitespace** (including newlines),
**letters** (`a-zA-Z0-9_`), and **symbols** (all other non-whitespace characters).

The current `word_boundary_left` and `word_boundary_right` helpers in
`crates/buffer/src/text_buffer.rs` use `char::is_whitespace()` as their sole
classifier. This means `foo.bar` is treated as a single word, and `foo + bar`
treats `+ ` as one run. Standard editor behavior (VS Code, Xcode, Sublime Text)
distinguishes letters from punctuation/symbols so that word-oriented operations
stop at class transitions like `foo|.bar` or `result|+|=|value`.

This chunk introduces a `CharClass` enum with three variants (`Whitespace`, `Letter`,
`Symbol`) and a classification function. It then updates `word_boundary_left` and
`word_boundary_right` to compare `CharClass` values instead of a boolean
`is_whitespace` flag. All call sites — `delete_backward_word`, `delete_forward_word`,
`move_word_left`, `move_word_right`, and `select_word_at` — automatically inherit the
new behavior without modification because they delegate to these two helpers.

Classification rules:
- **Whitespace**: any character where `char::is_whitespace()` returns true (this
  already includes `\n`, `\r`, `\t`, etc.)
- **Letter**: `a-z`, `A-Z`, `0-9`, `_`
- **Symbol**: everything else (`.`, `+`, `-`, `(`, `)`, `{`, `}`, `:`, `;`, `"`, etc.)

## Success Criteria

- A `CharClass` enum with variants `Whitespace`, `Letter`, `Symbol` exists in
  `crates/buffer/src/text_buffer.rs` (or a sub-module), along with a
  `fn char_class(c: char) -> CharClass` classifier.
- `char_class` classifies: whitespace → `Whitespace`, `a-zA-Z0-9_` → `Letter`,
  everything else → `Symbol`.
- `word_boundary_left` and `word_boundary_right` use `char_class` equality instead
  of `is_whitespace` boolean comparison to determine run boundaries.
- All existing word-oriented operations respect the new classification:
  - **Double-click select**: double-clicking `bar` in `foo.bar` selects only `bar`,
    not `foo.bar`. Double-clicking `.` selects only `.`.
  - **Delete backward word** (Alt+Backspace): with cursor after `foo.bar|`, deletes
    `bar` leaving `foo.|`.
  - **Delete forward word** (Alt+D): with cursor at `|foo.bar`, deletes `foo` leaving
    `.bar`.
  - **Move word left** (Alt+Left): cursor jumps to the start of the current
    letter/symbol/whitespace run, not to the start of all non-whitespace.
  - **Move word right** (Alt+Right): cursor jumps to the end of the current
    letter/symbol/whitespace run.
- Existing unit tests for `word_boundary_left` and `word_boundary_right` are updated
  to reflect the new classification.
- New unit tests cover class transitions: letter→symbol (`foo.bar`), symbol→letter
  (`..abc`), mixed sequences (`fn(x) + y`), underscore as letter (`my_var`), digits
  as letter (`x42`).



