---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/src/highlighter.rs
- crates/syntax/src/registry.rs
- crates/editor/src/workspace.rs
code_references:
  - ref: crates/syntax/src/highlighter.rs#InjectionRegion
    implements: "Struct tracking byte range, language name, and lazily-parsed tree for embedded language regions"
  - ref: crates/syntax/src/highlighter.rs#InjectionLayer
    implements: "Manages compiled injection query and cached injection regions"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::new_with_registry
    implements: "Constructor enabling injection support via shared LanguageRegistry"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::refresh_injection_regions
    implements: "Re-identifies injection regions when host tree changes"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::identify_injection_regions_impl
    implements: "Runs injection query to identify embedded language regions and extract language names"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::collect_injection_captures
    implements: "Lazily parses injection trees and collects captures for viewport"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::ensure_injection_tree_for_region
    implements: "Lazy parsing of injection tree with graceful fallback for unknown languages"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures
    implements: "Merges host and injection captures with injection precedence"
  - ref: crates/syntax/src/registry.rs#LanguageRegistry::config_for_language_name
    implements: "Language name lookup for injection support (e.g., 'rust' -> rs config)"
  - ref: crates/syntax/src/registry.rs#LanguageConfig::language_name
    implements: "Canonical language name field for same-language injection filtering"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- terminal_cursor_shading
---

# Chunk Goal

## Minor Goal

Implement tree-sitter injection-based highlighting so that fenced code blocks
in Markdown (and embedded languages in HTML) are syntax highlighted with the
appropriate language's grammar. Currently the `injections_query` field on
`LanguageConfig` is stored but unused (`#[allow(dead_code)]`), and the
`QueryCursor`-based highlighting pipeline has no injection support. A ` ```rust `
block in a `.md` file renders with only Markdown-level captures (fences as
`punctuation.special`, body as plain text).

This is a distinct mechanism from highlight-query layering (C/C++, JS/TS),
which concatenates queries for a single parser. Injection requires:
1. Compiling the host language's injection query to identify embedded regions
   and their target language names.
2. Parsing each injected region with the target language's parser.
3. Running the target language's highlight query over the injected sub-tree.
4. Merging the injected spans into the host document's styled output, with
   injected spans taking precedence within their byte range.

The performance constraint is critical: highlighting must stay within the <8ms
keypress-to-glyph budget. The injection parse trees can be cached alongside the
host tree and updated incrementally.

## Success Criteria

- Opening a Markdown file with fenced code blocks (` ```rust `, ` ```python `,
  ` ```javascript `, etc.) renders the code block contents with language-
  appropriate syntax highlighting, not just Markdown-level captures.
- HTML files with `<script>` and `<style>` tags highlight embedded JS and CSS.
- The `injections_query` field on `LanguageConfig` is no longer `dead_code`.
- Injection parse trees are cached and updated incrementally on edit, matching
  the existing host-tree incremental update pattern.
- Viewport highlighting with injections stays under 1ms for typical files
  (consistent with the <8ms budget and current ~170µs baseline).
- Editing inside an injected region (e.g., typing inside a code block)
  re-highlights correctly without full reparse.
- Languages not present in the registry gracefully fall back to no highlighting
  for that region (no panic, no visual glitch).


## Rejected Ideas

### Use `tree-sitter-highlight` crate's `Highlighter::highlight()` with injection_callback

The high-level `Highlighter::highlight()` API natively supports injections via
its `injection_callback` parameter. However, the `syntax_highlight_perf` chunk
intentionally moved away from this API to direct `QueryCursor` usage for
performance (full-file highlight was 14.5ms, blowing the 8ms budget). We should
extend the `QueryCursor` approach to handle injections rather than reverting to
the slower API.

### Concatenate all possible language queries into the Markdown highlight query

Query layering (the C/C++ and JS/TS pattern) doesn't apply here. The embedded
language isn't known statically — it depends on the info string after the fence
(` ```rust ` vs ` ```python `). Injection queries identify the regions and
target language names dynamically at parse time.