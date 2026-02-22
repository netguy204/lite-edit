<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds tree-sitter-based syntax highlighting for 13 languages while preserving the <8ms P99 keypress-to-glyph latency. The key insight from the investigation (`docs/investigations/syntax_highlighting_scalable`) is that viewport-scoped highlighting is essential — full-file highlighting at 14.5ms exceeds the budget, but incremental parse (~120µs) + viewport highlight (~170µs) = 286µs fits comfortably (3.6% of budget).

**Strategy:**

1. **New `crates/syntax/` crate**: A dedicated crate for syntax highlighting, keeping the `buffer` crate dependency-free. This crate will contain:
   - `SyntaxHighlighter`: Owns tree-sitter `Parser` and `Tree`, exposes `edit()` for incremental updates
   - `SyntaxTheme`: `HashMap<&'static str, Style>` mapping capture names to Catppuccin Mocha styles
   - `LanguageRegistry`: Extension-to-grammar mapping for 13 languages

2. **Integration via composition, not inheritance**: `TextBuffer::styled_line()` currently returns `StyledLine::plain()`. Rather than modifying `TextBuffer` to own a highlighter (which would add tree-sitter as a dependency to `buffer` crate), the highlighter will be owned by `EditorState` (or a tab-level wrapper) and used to transform plain lines into highlighted lines before rendering.

3. **Viewport-scoped highlighting**: The renderer already only calls `styled_line()` for visible lines. The highlighter provides a `highlight_line(line_idx, source) -> StyledLine` method that queries tree-sitter for just that line's byte range.

4. **Incremental parsing**: On every buffer mutation, `EditorState` calls `SyntaxHighlighter::edit()` with the `InputEdit` describing the change. Tree-sitter incrementally updates the parse tree in ~120µs.

**Patterns used:**
- Composition over modification: `TextBuffer` stays unchanged; highlighting wraps it
- Visitor pattern: tree-sitter `HighlightEvent` stream converted to `Span` vector
- Factory pattern: `LanguageRegistry` constructs highlighters by file extension

**Testing approach (per `TESTING_PHILOSOPHY.md`):**
- Unit tests for `SyntaxTheme` mapping completeness
- Unit tests for `LanguageRegistry` extension lookups
- Integration tests for `SyntaxHighlighter::edit()` incremental correctness
- Behavioral tests verifying specific capture-to-style mappings on known code snippets

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem indirectly. The `GlyphBuffer::update()` method already iterates only over `visible_range` and calls `view.styled_line(line)`. This viewport-scoped iteration is the critical property enabling our performance budget. No changes needed to the viewport subsystem; we rely on its existing behavior.

## Sequence

### Step 1: Create `crates/syntax/` crate with dependencies

Create a new `crates/syntax/` crate to house syntax highlighting logic. This keeps the `buffer` crate dependency-free while centralizing tree-sitter integration.

**Files:**
- `crates/syntax/Cargo.toml` — dependencies: `tree-sitter`, `tree-sitter-highlight`, 13 grammar crates, `lite-edit-buffer` (for `Style`, `Color`, `Span`, `StyledLine` types)
- `crates/syntax/src/lib.rs` — module structure and re-exports

**Grammar crates to add:**
- `tree-sitter-rust`
- `tree-sitter-cpp`
- `tree-sitter-c`
- `tree-sitter-python`
- `tree-sitter-typescript`
- `tree-sitter-javascript`
- `tree-sitter-go`
- `tree-sitter-json`
- `tree-sitter-toml`
- `tree-sitter-md` (for Markdown)
- `tree-sitter-html`
- `tree-sitter-css`
- `tree-sitter-bash`

**Backreference:** Add module-level chunk reference.

### Step 2: Implement `SyntaxTheme` with Catppuccin Mocha mapping

Create `crates/syntax/src/theme.rs` with a `SyntaxTheme` struct holding a `HashMap<&'static str, Style>`.

**Capture-to-style mapping (from H5 audit in investigation):**

| Capture | Color (Hex) | Italic |
|---------|-------------|--------|
| `keyword` | Mauve `#cba6f7` | no |
| `function` | Blue `#89b4fa` | no |
| `function.method` | Blue `#89b4fa` | no |
| `function.macro` | Mauve `#cba6f7` | no |
| `type` | Yellow `#f9e2af` | no |
| `type.builtin` | Yellow `#f9e2af` | yes |
| `constructor` | Sapphire `#74c7ec` | no |
| `string` | Green `#a6e3a1` | no |
| `escape` | Pink `#f5c2e7` | no |
| `constant` | Peach `#fab387` | no |
| `constant.builtin` | Peach `#fab387` | no |
| `number` | Peach `#fab387` | no |
| `comment` | Overlay0 `#6c7086` | yes |
| `comment.documentation` | Overlay0 `#6c7086` | yes |
| `variable.parameter` | Maroon `#eba0ac` | yes |
| `variable.builtin` | Red `#f38ba8` | no |
| `property` | Lavender `#b4befe` | no |
| `label` | Sapphire `#74c7ec` | yes |
| `punctuation.bracket` | Subtext0 `#a6adc8` | no |
| `punctuation.delimiter` | Subtext0 `#a6adc8` | no |
| `operator` | Sky `#89dceb` | no |
| `attribute` | Yellow `#f9e2af` | no |

**API:**
```rust
impl SyntaxTheme {
    pub fn catppuccin_mocha() -> Self { ... }
    pub fn style_for_capture(&self, name: &str) -> Option<&Style> { ... }
    pub fn capture_names(&self) -> &[&'static str] { ... }
}
```

**Tests:**
- All 22 capture names return `Some(style)`
- Returned styles use `Color::Rgb` for `fg`
- Italic captures (comment, type.builtin, variable.parameter, label) have `italic: true`

### Step 3: Implement `LanguageRegistry` with extension mapping

Create `crates/syntax/src/registry.rs` with a `LanguageRegistry` that maps file extensions to tree-sitter language configurations.

**Extension mappings:**
- `.rs` → Rust
- `.cpp`, `.cc`, `.cxx`, `.hpp`, `.h` → C++ (`.h` is ambiguous but C++ is more common in mixed codebases)
- `.c` → C
- `.py` → Python
- `.ts`, `.tsx` → TypeScript
- `.js`, `.jsx`, `.mjs` → JavaScript
- `.go` → Go
- `.json` → JSON
- `.toml` → TOML
- `.md`, `.markdown` → Markdown
- `.html`, `.htm` → HTML
- `.css` → CSS
- `.sh`, `.bash`, `.zsh` → Bash

**API:**
```rust
pub struct LanguageConfig {
    pub language: Language,
    pub highlights_query: &'static str,
    pub injections_query: &'static str,
}

impl LanguageRegistry {
    pub fn new() -> Self { ... }
    pub fn config_for_extension(&self, ext: &str) -> Option<&LanguageConfig> { ... }
    pub fn supported_extensions(&self) -> impl Iterator<Item = &str> { ... }
}
```

**Tests:**
- Each documented extension returns `Some(config)`
- Unknown extensions (e.g., `.xyz`) return `None`
- Both with and without leading dot work (`.rs` and `rs`)

### Step 4: Implement `SyntaxHighlighter` with incremental parsing

Create `crates/syntax/src/highlighter.rs` with the core `SyntaxHighlighter` struct.

**Structure:**
```rust
pub struct SyntaxHighlighter {
    parser: Parser,
    tree: Tree,
    hl_config: HighlightConfiguration,
    highlighter: Highlighter,  // tree-sitter-highlight's highlighter
    theme: SyntaxTheme,
    source_snapshot: String,   // needed for tree-sitter-highlight
}
```

**API:**
```rust
impl SyntaxHighlighter {
    /// Create highlighter from language config and initial source
    pub fn new(config: &LanguageConfig, source: &str, theme: SyntaxTheme) -> Self { ... }

    /// Apply an edit to the parse tree incrementally
    /// Takes byte offsets and (row, col) positions for old/new ranges
    pub fn edit(&mut self,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        start_position: (usize, usize),
        old_end_position: (usize, usize),
        new_end_position: (usize, usize),
        new_source: &str,
    ) { ... }

    /// Return highlighted spans for a single line
    pub fn highlight_line(&self, line_idx: usize, source: &str) -> StyledLine { ... }

    /// Update source snapshot (call after edit)
    pub fn update_source(&mut self, source: String) { ... }
}
```

**Implementation notes:**
- `edit()` calls `tree.edit()` with `InputEdit` struct, then `parser.parse(new_source, Some(&tree))`
- `highlight_line()` extracts the byte range for the requested line, runs `highlighter.highlight()`, and converts `HighlightEvent` stream to `Vec<Span>`
- For viewport efficiency, `highlight_line()` extracts just the line's bytes and highlights that substring (as validated by H2 benchmark showing 60-line viewport at 169µs)

**Tests:**
- `new()` produces valid parse tree for Rust code
- `edit()` correctly updates tree for single-char insert, delete, newline
- `highlight_line()` returns styled spans for a line containing keywords, strings, comments
- `highlight_line()` returns `StyledLine::plain()` equivalent for lines with no captures

### Step 5: Create helper for buffer edit-to-InputEdit translation

Create `crates/syntax/src/edit.rs` with a helper to translate buffer mutations into tree-sitter `InputEdit` format.

**The challenge:** `TextBuffer` mutations (insert_char, delete_char, insert_newline, delete_backward_at_line_start) need to be expressed as:
- `start_byte`, `old_end_byte`, `new_end_byte`
- `start_position` (row, col), `old_end_position`, `new_end_position`

**API:**
```rust
pub struct EditEvent {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_row: usize,
    pub start_col: usize,
    pub old_end_row: usize,
    pub old_end_col: usize,
    pub new_end_row: usize,
    pub new_end_col: usize,
}

/// Calculate byte offset for a (row, col) position
pub fn position_to_byte_offset(source: &str, row: usize, col: usize) -> usize { ... }

/// Calculate (row, col) for a byte offset
pub fn byte_offset_to_position(source: &str, byte_offset: usize) -> (usize, usize) { ... }
```

**Tests:**
- `position_to_byte_offset` handles multi-byte UTF-8 characters
- Round-trip: `byte_offset_to_position(position_to_byte_offset(s, r, c)) == (r, c)` for valid positions

### Step 6: Add `crates/syntax` dependency to `crates/editor`

Update `crates/editor/Cargo.toml` to depend on the new syntax crate.

**File:** `crates/editor/Cargo.toml`
```toml
lite-edit-syntax = { path = "../syntax" }
```

### Step 7: Wire `SyntaxHighlighter` into `EditorState`

Modify `EditorState` to optionally hold a `SyntaxHighlighter` for file buffers.

**Changes to `crates/editor/src/editor_state.rs`:**

1. Add field to file tab state: `highlighter: Option<SyntaxHighlighter>`

2. In `associate_file()`:
   - Extract extension from path
   - Query `LanguageRegistry` for config
   - If found, construct `SyntaxHighlighter` with the loaded content
   - Store in tab state

3. On buffer mutations (in key handlers):
   - If highlighter exists, call `highlighter.edit()` with appropriate `EditEvent`
   - Update source snapshot

**Tests:**
- Opening `.rs` file creates highlighter
- Opening `.txt` file does not create highlighter
- After insert_char, highlighter tree is updated (verify via highlight_line behavior)

### Step 8: Create `HighlightedBufferView` wrapper

Create a wrapper that implements `BufferView` and delegates to `TextBuffer`, but overrides `styled_line()` to use the highlighter.

**File:** `crates/editor/src/highlighted_buffer.rs`

```rust
/// A view over TextBuffer that applies syntax highlighting
pub struct HighlightedBufferView<'a> {
    buffer: &'a TextBuffer,
    highlighter: Option<&'a SyntaxHighlighter>,
}

impl<'a> BufferView for HighlightedBufferView<'a> {
    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.buffer.line_count() {
            return None;
        }

        match self.highlighter {
            Some(hl) => {
                let source = self.buffer.content();
                Some(hl.highlight_line(line, &source))
            }
            None => {
                let content = self.buffer.line_content(line);
                Some(StyledLine::plain(content))
            }
        }
    }

    // Delegate other methods to buffer...
}
```

**Tests:**
- With no highlighter, `styled_line()` returns plain text (same as `TextBuffer`)
- With highlighter, `styled_line()` returns styled spans
- All `BufferView` methods work correctly via delegation

### Step 9: Integrate `HighlightedBufferView` with `GlyphBuffer`

Update the render path to use `HighlightedBufferView` instead of raw `TextBuffer` when highlighting is available.

**Changes:**
- Wherever `GlyphBuffer::update()` is called with a `&dyn BufferView`, construct `HighlightedBufferView` if the tab has a highlighter
- This should be a minimal change — the renderer already works with styled spans

**Key insight:** The renderer already iterates `styled_line.spans` with per-span style resolution (see `glyph_buffer.rs` lines 560-610, 687-700, etc.). It calls `self.palette.resolve_color(span.style.fg)` for each span. This means:
- No renderer changes needed for color handling
- Syntax highlighting colors (`Color::Rgb`) pass through directly
- The `ColorPalette` already handles `Color::Rgb` variants

**Tests:**
- Visual verification that Rust code displays with colors
- Existing tests continue to pass (plain text rendering unchanged)

### Step 10: Handle initial parse on file open

The investigation found initial parse costs ~8.5ms for a 5.8K-line file. This happens once at file open and is acceptable since file I/O typically dominates.

**Changes to `associate_file()`:**
1. Read file content (already done)
2. Detect language by extension
3. If language recognized, create `SyntaxHighlighter` with full source
4. Initial `parser.parse()` runs synchronously

**No async needed:** 8.5ms initial parse is same order as file read. The first frame renders immediately after both complete.

### Step 11: Add tests for integration scenarios

Create integration tests verifying the full highlighting pipeline.

**File:** `crates/editor/tests/syntax_highlighting.rs`

**Test cases:**
1. Open a `.rs` file → `styled_line()` returns highlighted spans
2. Open a `.txt` file → `styled_line()` returns plain spans
3. Type a character in `.rs` file → highlighting updates correctly
4. Delete a character → highlighting updates
5. Paste multi-line text → highlighting updates
6. Open each of the 13 supported languages → each gets highlighting

**Behavioral tests (per TESTING_PHILOSOPHY.md):**
- Rust keyword `fn` has Mauve color
- Rust string `"hello"` has Green color
- Rust comment `// comment` has Overlay0 color + italic
- Python keyword `def` has Mauve color
- JSON string has Green color

### Step 12: Update GOAL.md with code_paths

After implementation, update `docs/chunks/syntax_highlighting/GOAL.md` frontmatter with actual files modified:

```yaml
code_paths:
  - crates/syntax/src/lib.rs
  - crates/syntax/src/theme.rs
  - crates/syntax/src/registry.rs
  - crates/syntax/src/highlighter.rs
  - crates/syntax/src/edit.rs
  - crates/syntax/Cargo.toml
  - crates/editor/src/editor_state.rs
  - crates/editor/src/highlighted_buffer.rs
  - crates/editor/Cargo.toml
```

## Dependencies

**External libraries (Cargo dependencies):**
- `tree-sitter = "0.24"` — Core incremental parsing library
- `tree-sitter-highlight = "0.24"` — Highlighting event stream API
- Language grammar crates (all latest compatible versions):
  - `tree-sitter-rust`
  - `tree-sitter-cpp`
  - `tree-sitter-c`
  - `tree-sitter-python`
  - `tree-sitter-typescript`
  - `tree-sitter-javascript`
  - `tree-sitter-go`
  - `tree-sitter-json`
  - `tree-sitter-toml`
  - `tree-sitter-md`
  - `tree-sitter-html`
  - `tree-sitter-css`
  - `tree-sitter-bash`

**No chunk dependencies:** This chunk builds on existing infrastructure (`TextBuffer`, `BufferView`, `StyledLine`, `Style`, `Color`) but doesn't require other chunks to be completed first.

## Risks and Open Questions

1. **TypeScript grammar complexity**: TypeScript/TSX use `tree-sitter-typescript` which exports both TypeScript and TSX parsers. Need to determine which extensions map to which parser (`.ts` → TypeScript, `.tsx` → TSX).

2. **Markdown grammar availability**: The `tree-sitter-md` crate may have different export patterns than other grammars. Verify the crate name and query exports during implementation.

3. **Source snapshot management**: `tree-sitter-highlight` requires the source text to be available during highlighting. We store a snapshot in `SyntaxHighlighter`. This doubles memory for file content but is necessary for the API. Alternative: pass source from `TextBuffer` on each `highlight_line()` call.

4. **Edit translation complexity**: Buffer mutations happen at cursor position expressed as `(line, col)` in characters. Tree-sitter needs byte offsets. Need to carefully handle UTF-8 multi-byte characters when translating positions.

5. **Grammar C code compilation**: Tree-sitter grammars are compiled C. The grammar crates handle this via build scripts, but this may slow first compilation. Acceptable for a one-time cost.

6. **Binary size increase**: Bundling 13 grammars will increase binary size. Investigation estimated ~2-5MB. Acceptable for the feature value.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->