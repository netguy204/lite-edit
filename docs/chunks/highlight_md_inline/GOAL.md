---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/Cargo.toml
- crates/syntax/src/highlighter.rs
- crates/syntax/src/registry.rs
- crates/syntax/src/theme.rs
code_references:
  - ref: crates/syntax/src/registry.rs#LanguageRegistry::new
    implements: "Register markdown_inline and yaml language configs for injection targets"
  - ref: crates/syntax/src/registry.rs#LanguageRegistry::config_for_language_name
    implements: "Map injection language names (markdown_inline, yaml, yml) to configs"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::identify_injection_regions_impl
    implements: "Defensive skip for unregistered injection languages to prevent phantom regions"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures
    implements: "Build injection ranges from actual captures instead of regions to preserve host captures"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures_impl
    implements: "Build injection ranges from actual captures instead of regions to preserve host captures"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_spans_with_external_text
    implements: "Build injection ranges from actual captures instead of regions to preserve host captures"
  - ref: crates/syntax/src/theme.rs#SyntaxTheme::catppuccin_mocha
    implements: "Add text.emphasis and text.strong styles for markdown inline formatting"
  - ref: crates/syntax/src/highlighter.rs#tests::test_markdown_heading_styled
    implements: "Verify heading text receives text.title styling through markdown_inline injection"
  - ref: crates/syntax/src/highlighter.rs#tests::test_markdown_yaml_frontmatter_styled
    implements: "Verify YAML frontmatter receives syntax highlighting via yaml injection"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- gotodef_status_render
---

# Chunk Goal

## Minor Goal

Fix markdown syntax highlighting so that headings, paragraph text, and YAML frontmatter are properly styled. Currently these elements appear unstyled because the `tree-sitter-md` injection query marks all `(inline)` nodes as injection regions for `markdown_inline`, and frontmatter as `yml`/`yaml` injection regions. Since neither language is registered in the editor's `LanguageRegistry`, these regions produce no injection highlights — but they DO cause host captures (like `@text.title` for headings) to be suppressed via the `overlaps_injection` check in `build_line_from_captures`.

### Root Cause

In `crates/syntax/src/highlighter.rs`, the `build_line_from_captures` method skips host captures that overlap with injection regions (to let injection highlights take precedence). The `tree-sitter-md` injection query (`tree-sitter-markdown/queries/injections.scm`) contains:

```
((inline) @injection.content (#set! injection.language "markdown_inline"))
([(minus_metadata) (plus_metadata)] @injection.content (#set! injection.language "yml"))
```

Every `(inline)` node (heading content, paragraph text, etc.) becomes an injection region for `markdown_inline`. Since that language isn't registered, no injection captures are produced — but the region still causes host captures to be skipped.

### Approach

Two complementary fixes:

1. **Register missing injection languages**: Add `markdown_inline` (using `tree_sitter_md::LANGUAGE_INLINE` + `HIGHLIGHT_QUERY_INLINE`) and `yaml` (using `tree_sitter_yaml`) to the `LanguageRegistry`. This makes the injections actually produce highlights — inline styling (bold, italic, etc.) within markdown text, and proper YAML highlighting in frontmatter.

2. **Defensive: skip unregistered injection languages**: In `identify_injection_regions_impl`, skip injection regions whose language is not found in the `LanguageRegistry`. This prevents any future unregistered injection language from creating phantom regions that suppress host highlights. This is a safety net so the same class of bug can't recur.

## Success Criteria

- `markdown_inline` and `yaml` are registered in `LanguageRegistry` and available for injection
- Markdown headings (`# H1`, `## H2`, etc.) render with `text.title` styling (mauve, bold) per the Catppuccin theme
- Heading markers (`#`, `##`) render with `punctuation.special` styling (subtext0)
- YAML frontmatter (`---` delimited) renders with yaml syntax highlighting via injection
- Unregistered injection languages are defensively skipped in `identify_injection_regions_impl` to prevent phantom region suppression
- Fenced code block injection highlighting (rust, python, etc.) continues to work correctly
- A test verifies heading text receives the `text.title` style
- All existing `test_markdown_*` tests continue to pass