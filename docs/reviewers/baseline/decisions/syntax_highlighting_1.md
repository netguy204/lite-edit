---
decision: FEEDBACK
summary: "Core syntax crate is complete and well-tested, but wiring to editor is incomplete — highlighter never instantiated on file open"
operator_review: null
---

## Criteria Assessment

### Criterion 1: Opening a `.rs`, `.cpp`, `.c`, `.py`, `.ts`, `.js`, `.go`, `.json`, `.toml`, `.md`, `.html`, `.css`, or `.sh` file displays syntax-highlighted text with Catppuccin Mocha colors.

- **Status**: gap
- **Evidence**: The `SyntaxHighlighter`, `SyntaxTheme`, and `LanguageRegistry` are implemented in `crates/syntax/` and tested. However, the `associate_file()` method in `crates/editor/src/editor_state.rs` does NOT create a `SyntaxHighlighter`. The `Tab.highlighter` field exists but is always `None`. Opening any file will show plain text.

### Criterion 2: Keywords, strings, comments, types, functions, operators, and punctuation are visually distinct.

- **Status**: gap
- **Evidence**: `SyntaxTheme::catppuccin_mocha()` correctly maps 22 capture names to Catppuccin Mocha colors. Tests verify keyword→Mauve, string→Green, comment→Overlay0+italic. However, since the highlighter is never wired into the render path, users won't see these colors.

### Criterion 3: Typing a character in a highlighted file updates highlighting correctly (incremental reparse) without visible flicker or delay.

- **Status**: gap
- **Evidence**: `SyntaxHighlighter::edit()` correctly applies `InputEdit` and incrementally reparses (~120µs). Tests verify incremental edits work. However, no buffer mutation handlers call `highlighter.edit()`. The PLAN (Step 7) specifies "On buffer mutations (in key handlers): If highlighter exists, call `highlighter.edit()`" — this is not implemented.

### Criterion 4: Files with unrecognized extensions render as plain unstyled text (same as today).

- **Status**: satisfied
- **Evidence**: `LanguageRegistry::config_for_extension()` returns `None` for unknown extensions (tests verify `.xyz`, `.txt` return None). `HighlightedBufferView` falls back to `StyledLine::plain()` when no highlighter. Since highlighter is always None currently, this criterion is vacuously satisfied.

### Criterion 5: The existing test suite continues to pass — `TextBuffer` without a highlighter attached still returns `StyledLine::plain()`.

- **Status**: satisfied
- **Evidence**: `cargo test -p lite-edit-syntax` passes (60 tests). `TextBuffer` is unchanged. `HighlightedBufferView::new(buffer, None)` returns plain styled lines (tested). Existing editor tests pass.

### Criterion 6: No changes to the `Style`, `Color`, `Span`, `StyledLine`, or `BufferView` types.

- **Status**: satisfied
- **Evidence**: No modifications to `crates/buffer/`. The `lit-edit-syntax` crate imports types from `lite-edit-buffer` without changing them. `git diff` shows no changes to these types.

## Feedback Items

### Issue 1: Highlighter not created on file open

- **id**: issue-hlcreate
- **location**: crates/editor/src/editor_state.rs:1795 (associate_file)
- **concern**: The `associate_file()` method loads file content into `TextBuffer` but never instantiates a `SyntaxHighlighter`. The PLAN Step 7 requires: "In `associate_file()`: Extract extension from path, query `LanguageRegistry` for config, if found construct `SyntaxHighlighter` with the loaded content, store in tab state."
- **suggestion**: After loading file content, extract extension with `path.extension()`, query `LanguageRegistry::new().config_for_extension(ext)`, and if found, create `SyntaxHighlighter::new(config, &contents, SyntaxTheme::catppuccin_mocha())` and set `tab.highlighter = Some(hl)`.
- **severity**: functional
- **confidence**: high

### Issue 2: Incremental edit not wired to key handlers

- **id**: issue-incedit
- **location**: crates/editor/src/editor_state.rs (key handlers)
- **concern**: Buffer mutations (insert_char, delete_char, insert_newline, etc.) do not call `SyntaxHighlighter::edit()`. The PLAN Step 7 requires: "On every buffer mutation, call `SyntaxHighlighter::edit()` with appropriate `EditEvent`."
- **suggestion**: After each buffer mutation in key handlers, if the active tab has a highlighter, compute the `EditEvent` using the helpers in `lite_edit_syntax::edit` and call `highlighter.edit(event, new_source)`.
- **severity**: functional
- **confidence**: high

### Issue 3: HighlightedBufferView not used in render path

- **id**: issue-hlrender
- **location**: crates/editor/src/glyph_buffer.rs (render path, Step 9)
- **concern**: The `HighlightedBufferView` wrapper exists but is never used. The render path still uses plain `BufferView` (via `Tab::buffer()`). The PLAN Step 9 requires integrating `HighlightedBufferView` with `GlyphBuffer::update()`.
- **suggestion**: Wherever the renderer calls `styled_line()` on a file buffer, wrap the TextBuffer with `HighlightedBufferView::new(buffer, tab.highlighter.as_ref())` to get highlighted output.
- **severity**: functional
- **confidence**: high

### Issue 4: highlighted_buffer module not declared

- **id**: issue-modmissing
- **location**: crates/editor/src/main.rs
- **concern**: The `highlighted_buffer.rs` file exists but is not declared as a module in main.rs. The module won't compile into the binary.
- **suggestion**: Add `mod highlighted_buffer;` to main.rs module declarations.
- **severity**: functional
- **confidence**: high

### Issue 5: Integration tests not added

- **id**: issue-inttests
- **location**: crates/editor/tests/
- **concern**: PLAN Step 11 specifies creating `crates/editor/tests/syntax_highlighting.rs` with integration tests. This file does not exist.
- **suggestion**: Add integration tests verifying: opening .rs file returns highlighted spans, typing updates highlighting, opening .txt returns plain spans, behavioral tests for specific capture colors.
- **severity**: style
- **confidence**: high
