<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix addresses a bug where markdown headings, paragraph text, and YAML frontmatter
appear unstyled. The root cause is a mismatch between the injection query and the
language registry:

1. **Injection query behavior**: The `tree-sitter-md` block grammar's injection query
   marks every `(inline)` node as an injection region for `markdown_inline`, and
   frontmatter blocks as injection regions for `yml`/`yaml`.

2. **Host capture suppression**: In `build_line_from_captures`, the highlighter skips
   host captures (like `@text.title` for headings) that overlap with injection regions
   to let injection highlights take precedence.

3. **Missing languages**: Neither `markdown_inline` nor `yaml` are registered in
   `LanguageRegistry`, so the injection regions produce no highlights — but they
   still suppress the host captures.

**Two-pronged fix**:

1. **Register missing languages**: Add `markdown_inline` and `yaml` to the registry
   so the injections produce actual highlights. `markdown_inline` provides styling for
   bold, italic, code spans, and links within text. `yaml` provides syntax highlighting
   for frontmatter blocks.

2. **Defensive skip for unregistered injections**: In `identify_injection_regions_impl`,
   skip injection regions whose language is not found in the registry. This prevents
   any future unregistered injection language from creating phantom regions that
   suppress host highlights. This is a safety net so the same bug class can't recur.

**Testing approach**: Per TESTING_PHILOSOPHY.md, write a failing test first that
asserts heading text receives `text.title` styling, then implement the fix.

## Sequence

### Step 1: Add tree-sitter-yaml dependency

Add `tree-sitter-yaml = "0.7"` to `crates/syntax/Cargo.toml`.

Location: `crates/syntax/Cargo.toml`

### Step 2: Write failing test for heading styling

Add a test `test_markdown_heading_styled` that:
1. Creates a markdown highlighter with content `# Hello World\n\nSome text.`
2. Highlights line 0 (the heading)
3. Asserts that "Hello World" has `text.title` styling (mauve, bold) from the theme

This test should fail initially because `markdown_inline` isn't registered.

Location: `crates/syntax/src/highlighter.rs` (test module)

### Step 3: Register `markdown_inline` in LanguageRegistry

Add a new language config for `markdown_inline`:
- Language: `tree_sitter_md::INLINE_LANGUAGE`
- Highlights query: `tree_sitter_md::HIGHLIGHT_QUERY_INLINE`
- Injections query: `tree_sitter_md::INJECTION_QUERY_INLINE` (may inject further)
- Language name: `"markdown_inline"`

Register it under the key `"markdown_inline"` (not an extension — injections use
language names, not extensions).

Update `config_for_language_name` to include the `"markdown_inline"` mapping.

Location: `crates/syntax/src/registry.rs`

### Step 4: Register `yaml` in LanguageRegistry

Add a new language config for `yaml`:
- Language: `tree_sitter_yaml::LANGUAGE`
- Highlights query: `tree_sitter_yaml::HIGHLIGHTS_QUERY`
- Injections query: `""` (empty — yaml doesn't inject other languages)
- Language name: `"yaml"`

Register it under extensions `"yaml"` and `"yml"`.

Update `config_for_language_name` to include `"yaml"`, `"yml"` mappings.

Location: `crates/syntax/src/registry.rs`

### Step 5: Update theme capture names list

The `markdown_inline` highlight query uses `@text.emphasis` and `@text.strong`
which aren't in the current theme. Add these captures to `SyntaxTheme`:
- `text.emphasis` → Italic (existing color, but italic style)
- `text.strong` → Bold (existing color, but bold style)

Location: `crates/syntax/src/theme.rs`

### Step 6: Defensive skip for unregistered injection languages

Modify `identify_injection_regions_impl` to check if the injection language is
registered before adding the region. If `config_for_language_name(&lang)` returns
`None`, skip creating the `InjectionRegion`.

This requires access to the `LanguageRegistry` in `identify_injection_regions_impl`.
The method is called from `refresh_injection_regions` which has access to `self.registry`.
Pass the registry (or a lookup function) to `identify_injection_regions_impl`.

Location: `crates/syntax/src/highlighter.rs`

### Step 7: Verify the test passes

Run `cargo test -p lite-edit-syntax test_markdown_heading_styled` and confirm it passes.

### Step 8: Add test for YAML frontmatter highlighting

Add a test `test_markdown_yaml_frontmatter_styled` that:
1. Creates a markdown highlighter with content containing YAML frontmatter
2. Highlights a line within the frontmatter
3. Asserts that YAML syntax (keys, values) receives appropriate styling

Location: `crates/syntax/src/highlighter.rs` (test module)

### Step 9: Run full test suite

Run `cargo test -p lite-edit-syntax` to verify:
- All existing `test_markdown_*` tests still pass
- Fenced code block injection highlighting (rust, python, etc.) still works
- No regressions in other language highlighting

## Dependencies

- **External library**: `tree-sitter-yaml = "0.7"` — required for YAML frontmatter
  highlighting. Available on crates.io and compatible with tree-sitter 0.24.

## Risks and Open Questions

1. **`markdown_inline` doesn't capture heading text as `@text.title`**: Looking at
   `HIGHLIGHT_QUERY_INLINE`, it captures `emphasis`, `strong_emphasis`, `code_span`,
   `link_text`, etc., but NOT heading text itself. The `@text.title` capture is in the
   **block** grammar's highlight query: `(atx_heading (inline) @text.title)`. This
   means even with `markdown_inline` registered, heading text will get inline styling
   (bold/italic if present) but not the title color.

   **Mitigation**: The defensive skip (Step 6) may be the key fix here. If we skip
   injection regions for `markdown_inline` when evaluating the host `@text.title`
   capture would give us the heading styling. However, this would lose inline
   formatting within headings (e.g., `# Hello *world*` wouldn't italicize "world").

   **Alternative consideration**: Perhaps the correct behavior is:
   - Host captures (`@text.title`) style the overall node
   - Injection captures provide _additional_ styling for specific ranges within

   This would require a more nuanced merge strategy. For now, the defensive skip
   approach should restore heading styling, and inline formatting loss in headings
   is acceptable as a known limitation.

2. **Performance of yaml parsing**: YAML frontmatter is typically small (10-50 lines),
   so parsing overhead should be negligible. Monitor if issues arise with very large
   frontmatter blocks.

3. **`markdown_inline` injection within `markdown_inline`**: The inline grammar's
   injection query may inject further languages (e.g., latex in formulas). We don't
   support this currently, but the defensive skip will prevent phantom regions.

## Deviations

### Changed injection region suppression logic

The plan identified Risk #1: `markdown_inline` doesn't capture heading text as `@text.title`.
The original approach (registering `markdown_inline` + defensive skip for unregistered languages)
didn't fully solve the problem because:

1. Registering `markdown_inline` means injection regions ARE created for all `(inline)` nodes
2. The defensive skip only applies to UNREGISTERED languages
3. The merge logic in `build_line_from_captures` was suppressing host captures when they
   overlapped with injection REGIONS (not captures)

**Actual fix implemented**: Changed the overlap check to use actual injection CAPTURES instead
of injection REGIONS. The `injection_regions` vector is now built from the captures buffer
instead of from `InjectionRegion.byte_range`. This ensures host captures (like `@text.title`
for headings) are only suppressed when there are actual injection captures covering that range.

This fix applies to three places:
- `build_line_from_captures`
- `build_line_from_captures_impl`
- `build_spans_with_external_text`

### Updated theme test

The `test_styles_use_rgb_colors` test originally asserted all captures have RGB colors.
Added exception for `text.emphasis` and `text.strong` which only set italic/bold formatting
without changing the foreground color - standard practice for emphasis styling in editors.
