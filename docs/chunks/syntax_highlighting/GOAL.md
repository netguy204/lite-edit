---
status: ACTIVE
ticket: null
parent_chunk: null
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
code_references:
  - ref: crates/syntax/src/lib.rs
    implements: "Crate root with public API re-exports for SyntaxHighlighter, SyntaxTheme, LanguageRegistry, and edit helpers"
  - ref: crates/syntax/src/theme.rs#SyntaxTheme
    implements: "Catppuccin Mocha theme mapping tree-sitter capture names to Style values"
  - ref: crates/syntax/src/theme.rs#SyntaxTheme::catppuccin_mocha
    implements: "Factory method creating the Catppuccin Mocha color mapping for 22 capture types"
  - ref: crates/syntax/src/theme.rs#SyntaxTheme::style_for_capture
    implements: "Capture name to style lookup with prefix fallback matching"
  - ref: crates/syntax/src/registry.rs#LanguageConfig
    implements: "Language configuration holding tree-sitter Language, highlights/injections/locals queries (refactored by syntax_highlight_perf to expose queries for direct QueryCursor usage)"
  - ref: crates/syntax/src/registry.rs#LanguageRegistry
    implements: "Extension-to-language mapping for 13 languages with 24 file extension patterns"
  - ref: crates/syntax/src/registry.rs#LanguageRegistry::config_for_extension
    implements: "Lookup language config by file extension (with or without leading dot)"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter
    implements: "Core highlighter owning tree-sitter Parser, Tree, and Query (refactored by syntax_highlight_perf from HighlightConfiguration to direct Query for viewport-batch performance)"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::new
    implements: "Creates highlighter from language config, performs initial parse"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::edit
    implements: "Incremental parse tree update (~120µs per single-char edit)"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::highlight_line
    implements: "Line highlighting with cache lookup (refactored by syntax_highlight_perf to use viewport cache and QueryCursor instead of per-line HighlightEvents)"
  - ref: crates/syntax/src/highlighter.rs#merge_spans
    implements: "Optimizes span list by merging adjacent spans with same style"
  - ref: crates/syntax/src/edit.rs#EditEvent
    implements: "Edit descriptor with byte offsets and row/col positions for tree-sitter InputEdit"
  - ref: crates/syntax/src/edit.rs#position_to_byte_offset
    implements: "Translates (row, col) character position to byte offset in UTF-8 source"
  - ref: crates/syntax/src/edit.rs#insert_event
    implements: "Creates EditEvent for text insertion at a position"
  - ref: crates/syntax/src/edit.rs#delete_event
    implements: "Creates EditEvent for text deletion between positions"
  - ref: crates/editor/src/highlighted_buffer.rs#HighlightedBufferView
    implements: "BufferView wrapper applying SyntaxHighlighter to TextBuffer output"
  - ref: crates/editor/src/highlighted_buffer.rs#HighlightedBufferViewMut
    implements: "Mutable BufferView wrapper with dirty line tracking support"
  - ref: crates/editor/src/workspace.rs#Tab::setup_highlighting
    implements: "Wires highlighter to file tab by detecting extension and creating SyntaxHighlighter"
  - ref: crates/editor/src/workspace.rs#Tab::notify_edit
    implements: "Forwards buffer edit events to highlighter for incremental parsing"
  - ref: crates/editor/src/workspace.rs#Tab::highlighter
    implements: "Accessor for optional SyntaxHighlighter on file tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::setup_active_tab_highlighting
    implements: "Entry point wiring highlighting on file open via language_registry lookup"
narrative: null
investigation: syntax_highlighting_scalable
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- terminal_cmd_backspace
- terminal_paste_render
- terminal_viewport_init
---

# Chunk Goal

## Minor Goal

Add tree-sitter-based syntax highlighting for 13 languages: Rust, C++, C, Python, TypeScript, JavaScript, Go, JSON, TOML, Markdown, HTML, CSS, and Bash. This is a core usability feature — syntax highlighting is table-stakes for a text editor. The implementation must preserve the <8ms P99 keypress-to-glyph latency north star by using incremental parsing and viewport-scoped highlight queries, as validated by the investigation (`docs/investigations/syntax_highlighting_scalable`).

The key components are:

- **`SyntaxHighlighter`**: Owns a tree-sitter `Parser` and `Tree`. Exposes `edit()` to incrementally update the parse tree after buffer mutations (~120µs per single-char edit), and provides highlighted spans for lines queried by the renderer.
- **`SyntaxTheme`**: A `HashMap<&str, Style>` mapping tree-sitter capture names (e.g., `keyword`, `string`, `comment`) to Catppuccin Mocha `Style` values. Only `fg: Color::Rgb` and `italic: bool` are needed (verified by H5 audit).
- **`LanguageRegistry`**: Maps file extensions to `(LanguageFn, highlights_query, injections_query)` tuples. Each grammar crate exports these as constants — adding a language is one line.
- **`TextBuffer` integration**: Override `styled_line()` to return highlighted `StyledLine` with multiple styled `Span`s instead of `StyledLine::plain()`. The renderer already iterates spans with per-span color resolution — no renderer changes needed.
- **File open wiring**: Detect language by extension in `associate_file()`, construct `SyntaxHighlighter` with the matched grammar, perform initial parse.

**What does NOT change**: `Style`, `Color`, `Span`, `StyledLine` types; `BufferView` trait; `GlyphBuffer` / renderer; `ColorPalette`.

## Success Criteria

- Opening a `.rs`, `.cpp`, `.c`, `.py`, `.ts`, `.js`, `.go`, `.json`, `.toml`, `.md`, `.html`, `.css`, or `.sh` file displays syntax-highlighted text with Catppuccin Mocha colors.
- Keywords, strings, comments, types, functions, operators, and punctuation are visually distinct.
- Typing a character in a highlighted file updates highlighting correctly (incremental reparse) without visible flicker or delay.
- Files with unrecognized extensions render as plain unstyled text (same as today).
- The existing test suite continues to pass — `TextBuffer` without a highlighter attached still returns `StyledLine::plain()`.
- No changes to the `Style`, `Color`, `Span`, `StyledLine`, or `BufferView` types.

## Rejected Ideas

### Full-file highlight query on every keystroke

The H2 benchmark measured full-file `tree-sitter-highlight` at 14.5ms for a 5.8K-line Rust file — 182% of the 8ms latency budget. The architecture must rely on the renderer only calling `styled_line()` for visible lines (viewport-scoped), which costs ~170µs for 60 lines.

### Async highlighting with flash-of-unstyled-content

H2/H3 data showed viewport-scoped synchronous highlighting fits comfortably in the budget (286µs = 3.6%). Even a 50-line paste only costs 1.6ms. Async highlighting adds complexity (background thread, dirty notification, potential flicker) for no measurable benefit. Deferred unless a real scenario exceeds the budget.

### syntect (regex-based highlighting)

syntect uses Sublime Text syntax definitions — regex-based, non-incremental. Every edit re-highlights from the start of the changed scope, which is O(file size) in the worst case. Tree-sitter's incremental parsing is O(edit size), making it fundamentally better for per-keystroke highlighting in a latency-sensitive editor.