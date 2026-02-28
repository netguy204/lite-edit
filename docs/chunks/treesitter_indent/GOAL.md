---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/indent.rs
  - crates/syntax/src/registry.rs
  - crates/syntax/src/highlighter.rs
  - crates/syntax/src/lib.rs
  - crates/syntax/queries/rust/indents.scm
  - crates/syntax/queries/python/indents.scm
  - crates/editor/src/workspace.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/tests/indent_test.rs
code_references:
  - ref: crates/syntax/src/indent.rs#IndentConfig
    implements: "Configuration type for indentation behavior (tab width, spaces vs tabs)"
  - ref: crates/syntax/src/indent.rs#IndentComputer
    implements: "Core tree-sitter indent computation using Helix-style hybrid heuristic"
  - ref: crates/syntax/src/indent.rs#IndentComputer::compute_indent
    implements: "Main entry point combining reference line lookup, delta computation, and indent generation"
  - ref: crates/syntax/src/indent.rs#IndentComputer::compute_indent_delta
    implements: "Query-based indent delta algorithm walking @indent/@outdent captures"
  - ref: crates/syntax/src/indent.rs#CaptureIndices
    implements: "Capture index caching for @indent/@outdent/@extend/@indent.ignore"
  - ref: crates/syntax/src/registry.rs#LanguageConfig::indents_query
    implements: "indents_query field on LanguageConfig for per-language indent queries"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::compute_indent
    implements: "Exposes indent computation through SyntaxHighlighter API"
  - ref: crates/editor/src/workspace.rs#Tab::compute_indent_for_line
    implements: "Tab-level API exposing indent computation to editor"
  - ref: crates/editor/src/editor_state.rs#EditorState::apply_auto_indent
    implements: "Enter-key integration: computes indent and inserts at cursor after newline"
  - ref: crates/syntax/queries/rust/indents.scm
    implements: "Rust indent query ported from Helix"
  - ref: crates/syntax/queries/python/indents.scm
    implements: "Python indent query ported from Helix with @extend support"
narrative: null
investigation: treesitter_editing
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- incremental_parse
created_after:
- pty_wakeup_reliability
---

# Chunk Goal

## Minor Goal

Add tree-sitter-based intelligent auto-indentation using Helix-style indent queries. When a user presses Enter, the editor should compute the correct indent level for the new line based on the parse tree's structure rather than simply copying the previous line's indentation.

This chunk adds a new query type (`indents.scm`) to `LanguageConfig` alongside existing highlight and injection queries. It ports Helix's indent computation algorithm — which walks ancestors from the cursor position, collects `@indent`/`@outdent`/`@extend` captures with scope awareness, and applies a hybrid heuristic for resilience to incomplete expressions. Indent query files are ported from Helix's `runtime/queries/{lang}/indents.scm` for all 13 supported languages.

This directly improves the daily editing experience. Currently lite-edit has no auto-indentation — users must manually type whitespace after every newline. Smart indent is a table-stakes feature for a code editor (see GOAL.md: "features needed for daily coding work").

## Success Criteria

- `LanguageConfig` loads an `indents.scm` query alongside highlights/injections for each supported language
- When the user presses Enter in a file buffer, the new line is automatically indented to the correct level based on the parse tree (e.g., +1 indent after opening `{`, matching indent for `}`, continued indent for incomplete expressions)
- The hybrid heuristic is implemented: indent is computed as a delta relative to a nearby reference line's actual indentation, not as an absolute level — this ensures resilience to ERROR nodes from incomplete expressions
- `@indent`, `@outdent`, `@indent.always`, `@outdent.always`, `@extend`, and `@extend.prevent-once` captures are supported
- Scope modifiers (`tail` vs `all`) are respected per Helix's convention
- Indent query files are present and tested for at minimum Rust and Python (remaining languages may use simpler queries or be added incrementally)
- Indentation does not introduce latency perceptible to the user (must stay within the 8ms keystroke-to-glyph budget per GOAL.md)
- Typing inside multiline strings or comments does not produce incorrect indentation (either `@indent.ignore`-style handling or no indent change)

## Rejected Ideas

### Use nvim-treesitter's indent convention instead of Helix's

nvim-treesitter uses `@indent.begin`/`@indent.branch`/`@indent.dedent` capture names. Helix uses `@indent`/`@outdent`/`@extend`.

Rejected because: Helix is a Rust editor using the same `tree-sitter` crate (v0.24). Its implementation (`helix-core/src/indent.rs`) is the most directly portable reference. Helix's documentation is better (docs.helix-editor.com/guides/indent.html vs reverse-engineering nvim-treesitter Lua). The two conventions are incompatible — we must pick one, and Helix minimizes porting effort.

### Simple "count block ancestor nodes" approach

Walk the ancestor chain, count block-like nodes, use that as indent level.

Rejected because: This fails for Python (no delimiters), multiline expressions (chained method calls), and `else`/`elif` constructs. The Helix query-based approach handles all of these with well-tested query files across many languages.