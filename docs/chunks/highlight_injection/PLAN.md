<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Extend the existing `SyntaxHighlighter` to support tree-sitter injections for
embedded languages (Markdown fenced code blocks, HTML script/style tags). The
approach builds on the viewport-batch `QueryCursor` pattern from `syntax_highlight_perf`
while adding injection-specific capabilities.

### High-Level Strategy

1. **Parse injection queries** at `SyntaxHighlighter::new()` time, storing a compiled
   `Query` for injections alongside the existing highlight query.

2. **Identify injection regions** by running the injection query against the host tree.
   Extract `@injection.content` nodes and their target language (via `@injection.language`
   capture or `#set! injection.language` predicate).

3. **Cache injection parse trees** alongside the host tree. Each injection region gets
   its own `Tree` parsed with the target language's parser. These trees are updated
   incrementally via `Tree::edit()` when edits occur within their byte range.

4. **Merge injected highlights** during `highlight_viewport()` and `highlight_line()`.
   For each line in the viewport, collect captures from both the host tree and any
   overlapping injection trees. Injected spans take precedence within their byte range.

5. **Language resolution** uses `LanguageRegistry` to look up configs for injected
   language names. Unknown languages gracefully fall back to no highlighting (host
   tree captures are preserved).

### Performance Considerations

The viewport-scoped approach ensures injection parsing and highlighting stay within
the <8ms budget:

- **Lazy parsing**: Only parse injection trees that intersect the visible viewport.
- **Incremental updates**: When an edit occurs inside an injection region, only that
  region's tree is re-parsed incrementally.
- **Viewport batching**: Collect injection captures in the same pass as host captures,
  avoiding per-line query overhead.

The existing ~170µs baseline for a 60-line viewport should increase by at most 2-3x
when files contain multiple injection regions (still well under 1ms).

### Key Implementation Decisions

- **Single injection tree per region**: Each `(code_fence_content)` or `(raw_text)`
  node gets one parse tree. Multi-region "combined injections" (tree-sitter's feature
  for HEREDOC-style constructs) are not needed for Markdown/HTML use cases.

- **Registry-based language lookup**: Use `LanguageRegistry::config_for_extension()`
  with language name aliases (e.g., "rust" → "rs", "javascript" → "js"). Add a new
  `config_for_language_name()` method for direct name lookups.

- **Cache invalidation**: The injection cache shares the `generation` counter with
  the host tree. Any edit invalidates all cached highlights, but injection trees are
  only re-parsed if their byte range intersects the edit.

## Subsystem Considerations

No existing subsystems are directly relevant. The `renderer` and `viewport_scroll`
subsystems operate downstream of syntax highlighting and don't need modification.

If the `syntax_*` chunk cluster grows further after this implementation (5+ chunks),
consider proposing a `syntax_highlighting` subsystem to capture the architectural
invariants (viewport batching, incremental parsing, capture-to-style resolution).

## Sequence

### Step 1: Add language name lookup to LanguageRegistry

**File**: `crates/syntax/src/registry.rs`

Add a `config_for_language_name(&self, name: &str) -> Option<&LanguageConfig>` method
to `LanguageRegistry` that maps common language names to configs:

- "rust" → config for "rs"
- "python" → config for "py"
- "javascript" / "js" → config for "js"
- "typescript" / "ts" → config for "ts"
- "tsx" → config for "tsx"
- "json" → config for "json"
- "toml" → config for "toml"
- "html" → config for "html"
- "css" → config for "css"
- "bash" / "shell" / "sh" → config for "sh"
- "c" → config for "c"
- "cpp" / "c++" → config for "cpp"
- "go" / "golang" → config for "go"
- "markdown" / "md" → config for "md"

This enables injection query results (which use language names like "rust", "python")
to resolve to the correct `LanguageConfig`.

**Tests**:
- `test_language_name_lookup_rust()`: "rust" returns same config as "rs"
- `test_language_name_lookup_javascript()`: "javascript" and "js" return same config
- `test_language_name_lookup_unknown()`: "fortran" returns None

---

### Step 2: Define InjectionRegion and InjectionLayer structs

**File**: `crates/syntax/src/highlighter.rs`

Define structs to track injection state:

```rust
/// An identified region where another language is embedded.
struct InjectionRegion {
    /// Byte range in the host document
    byte_range: std::ops::Range<usize>,
    /// Language name extracted from the injection query
    language_name: String,
    /// Parsed tree for this region (lazily populated)
    tree: Option<Tree>,
    /// Generation at which the tree was parsed (for cache invalidation)
    tree_generation: u64,
}

/// Manages injection regions and their parse trees.
struct InjectionLayer {
    /// Compiled injection query for the host language
    injection_query: Option<Query>,
    /// Cached injection regions (re-identified when host tree changes)
    regions: Vec<InjectionRegion>,
    /// Generation at which regions were identified
    regions_generation: u64,
}
```

The `InjectionLayer` will be stored in `SyntaxHighlighter` alongside the existing
host tree and query.

---

### Step 3: Compile injection query at SyntaxHighlighter creation

**File**: `crates/syntax/src/highlighter.rs`

Modify `SyntaxHighlighter::new()` to:

1. Check if `config.injections_query` is non-empty
2. If so, compile it into a `Query` for the host language
3. Store the query in a new `injection_layer: Option<InjectionLayer>` field

Remove the `#[allow(dead_code)]` attribute from `LanguageConfig::injections_query`
once it's being used.

**Tests**:
- `test_markdown_has_injection_query()`: Creating a highlighter for `.md` files
  results in a populated `injection_layer`
- `test_rust_has_no_injection_query()`: Creating a highlighter for `.rs` files
  results in `injection_layer: None` (Rust has no injections query)

---

### Step 4: Implement injection region identification

**File**: `crates/syntax/src/highlighter.rs`

Add method `identify_injection_regions(&self) -> Vec<InjectionRegion>`:

1. Run the injection `QueryCursor` against the host tree's root node
2. For each match, extract:
   - `@injection.content` capture → byte range
   - `@injection.language` capture OR `#set! injection.language` predicate → language name
3. Normalize language names (lowercase, trim whitespace)
4. Return the collected regions sorted by start byte

The query predicates are accessed via `Query::property_settings(pattern_index)`.
Look for `("injection.language", Some(value))` tuples.

**Tests**:
- `test_identify_markdown_injection_regions()`: Markdown with ` ```rust ` and ` ```python `
  fenced blocks identifies two regions with correct languages and byte ranges
- `test_identify_html_injection_regions()`: HTML with `<script>` and `<style>` tags
  identifies JavaScript and CSS regions

---

### Step 5: Implement lazy injection tree parsing

**File**: `crates/syntax/src/highlighter.rs`

Add method `ensure_injection_tree(&self, region: &mut InjectionRegion, registry: &LanguageRegistry)`:

1. If `region.tree.is_some()` and `region.tree_generation == self.generation`, return early
2. Look up the language config via `registry.config_for_language_name(&region.language_name)`
3. If config not found, set `region.tree = None` and return (graceful fallback)
4. Create a `Parser`, set the language, extract the source substring for the region
5. Parse and store the tree: `region.tree = Some(tree)`
6. Update `region.tree_generation = self.generation`

This requires passing a `&LanguageRegistry` to the highlighter methods that need injection
support. Consider storing a registry reference in `SyntaxHighlighter` at construction time.

**Design decision**: The highlighter will store a `LanguageRegistry` owned value (or `Arc`
for future threading). This adds ~500 bytes per highlighter but avoids lifetime complexity.

---

### Step 6: Integrate injection captures into highlight_viewport

**File**: `crates/syntax/src/highlighter.rs`

Modify `highlight_viewport()` to:

1. Before collecting host captures, refresh injection regions if needed:
   - If `injection_layer.regions_generation != self.generation`, re-identify regions
2. For each injection region that overlaps the viewport byte range:
   - Call `ensure_injection_tree()` to lazily parse
   - If the region has a valid tree, run `QueryCursor` with the injected language's
     highlight query, scoped to the region's byte range
   - Collect captures into `captures_buffer` with adjusted byte offsets (injection
     byte range is relative to region start, needs offset to host document coordinates)
3. Merge injection captures into the host captures buffer:
   - Injection captures come after host captures but within the same byte range
   - The span-building logic already handles overlapping captures (later captures
     take precedence within their range via `covered_until` tracking)

**Critical**: Injection captures must be sorted by start byte and interleaved correctly.
Simplest approach: collect host and injection captures separately, then merge-sort.

---

### Step 7: Handle injection region edits incrementally

**File**: `crates/syntax/src/highlighter.rs`

Modify `edit()` to:

1. After updating the host tree, iterate through injection regions
2. For each region whose byte range overlaps with the edit range:
   - Compute the adjusted `InputEdit` relative to the injection region's start byte
   - Call `region.tree.edit()` if the tree exists
   - Mark `region.tree_generation` as stale to trigger reparse on next highlight

For edits entirely outside an injection region:
- If edit is before the region, adjust `region.byte_range` by the delta
- If edit is after the region, no adjustment needed

For edits that span injection boundaries:
- The host tree's injection query will re-identify regions, so existing region
  tracking becomes invalid; set `regions_generation` to stale

---

### Step 8: Update line offset handling for injections

**File**: `crates/syntax/src/highlighter.rs`

The existing `line_offsets` index maps host document lines to byte offsets. Injection
trees parse only their embedded content, so their byte offsets are relative to the
region start.

Add helper: `fn offset_injection_captures(captures: &mut [CaptureEntry], region_start: usize)`
that adds `region_start` to all capture byte positions, translating injection-local
coordinates to host-document coordinates.

---

### Step 9: Add fallback for unknown injection languages

**File**: `crates/syntax/src/highlighter.rs`

When `config_for_language_name()` returns `None`:

1. Set `region.tree = None`
2. The injection region contributes no captures
3. Host-level captures (e.g., `punctuation.special` for fences) remain visible
4. No error, no panic, no visual glitch — the block just appears unstyled

**Test**:
- `test_unknown_injection_language_graceful()`: Markdown with ` ```cobol ` renders
  without crashes; the code block body has no syntax highlighting but fences are styled

---

### Step 10: Write integration tests for Markdown fenced code blocks

**File**: `crates/syntax/src/highlighter.rs` (test module)

```rust
#[test]
fn test_markdown_rust_code_block_highlighting() {
    let source = r#"# Hello

```rust
fn main() {
    println!("Hello!");
}
```

Some text.
"#;
    let registry = LanguageRegistry::new();
    let config = registry.config_for_extension("md").unwrap();
    let theme = SyntaxTheme::catppuccin_mocha();
    let hl = SyntaxHighlighter::new(config, source, theme, &registry).unwrap();

    // Line 3 is "fn main() {" — should have "fn" highlighted as keyword
    let styled = hl.highlight_line(3);
    let has_styled_fn = styled.spans.iter()
        .any(|s| s.text == "fn" && !matches!(s.style.fg, Color::Default));
    assert!(has_styled_fn, "fn keyword should be highlighted");
}
```

Additional tests:
- `test_markdown_multiple_code_blocks()`: File with Rust and Python blocks highlights both
- `test_markdown_code_block_edit()`: Typing inside a code block re-highlights correctly
- `test_markdown_add_code_block()`: Adding ` ``` ` delimiters triggers injection detection

---

### Step 11: Write integration tests for HTML script/style tags

**File**: `crates/syntax/src/highlighter.rs` (test module)

```rust
#[test]
fn test_html_script_tag_highlighting() {
    let source = r#"<!DOCTYPE html>
<html>
<body>
<script>
const x = 42;
console.log(x);
</script>
</body>
</html>
"#;
    let registry = LanguageRegistry::new();
    let config = registry.config_for_extension("html").unwrap();
    let theme = SyntaxTheme::catppuccin_mocha();
    let hl = SyntaxHighlighter::new(config, source, theme, &registry).unwrap();

    // Line 4 is "const x = 42;" — should have "const" highlighted as keyword
    let styled = hl.highlight_line(4);
    let has_styled_const = styled.spans.iter()
        .any(|s| s.text.contains("const") && !matches!(s.style.fg, Color::Default));
    assert!(has_styled_const, "const keyword should be highlighted in script tag");
}
```

Additional tests:
- `test_html_style_tag_highlighting()`: CSS inside `<style>` tags is highlighted
- `test_html_inline_js_edit()`: Editing inside `<script>` re-highlights correctly

---

### Step 12: Write performance benchmark

**File**: `crates/syntax/src/highlighter.rs` (test module)

```rust
#[test]
fn test_injection_highlighting_performance() {
    // Generate a Markdown file with 10 Rust code blocks of ~20 lines each
    let mut source = String::new();
    for i in 0..10 {
        source.push_str(&format!("## Section {}\n\n", i));
        source.push_str("```rust\n");
        for j in 0..20 {
            source.push_str(&format!("fn function_{}_{j}() {{ let x = {}; }}\n", i, j * 42));
        }
        source.push_str("```\n\n");
    }

    let registry = LanguageRegistry::new();
    let config = registry.config_for_extension("md").unwrap();
    let theme = SyntaxTheme::catppuccin_mocha();
    let hl = SyntaxHighlighter::new(config, &source, theme, &registry).unwrap();

    // Time viewport highlighting (60 lines, spanning multiple code blocks)
    let start = std::time::Instant::now();
    hl.highlight_viewport(0, 60);
    let viewport_time = start.elapsed();

    eprintln!("Injection viewport highlight (60 lines): {}µs", viewport_time.as_micros());

    // Assert performance stays under 1ms (consistent with goal's <1ms budget)
    assert!(
        viewport_time.as_millis() < 2,
        "Injection highlighting took too long: {}ms (target: <1ms)",
        viewport_time.as_millis()
    );
}
```

---

### Step 13: Remove dead_code attribute and update GOAL.md code_paths

**Files**:
- `crates/syntax/src/registry.rs`: Remove `#[allow(dead_code)]` from `injections_query`
- `docs/chunks/highlight_injection/GOAL.md`: Update `code_paths` with all modified files

Final `code_paths` should include:
- `crates/syntax/src/highlighter.rs`
- `crates/syntax/src/registry.rs`

---

### Step 14: Verify full test suite passes

Run `cargo test -p lite-edit-syntax` and ensure:

1. All existing tests pass (no regressions)
2. All new injection tests pass
3. Performance test meets the <1ms target

---

**BACKREFERENCE COMMENTS**

Add this backreference at the top of `highlighter.rs`:
```rust
// Chunk: docs/chunks/highlight_injection - Tree-sitter injection-based highlighting
```

Add to any new injection-specific structs/methods:
```rust
// Chunk: docs/chunks/highlight_injection - Injection region management
```

## Dependencies

**Chunk dependencies**: None. This chunk builds on `syntax_highlighting` and
`syntax_highlight_perf` which are both ACTIVE.

**External libraries**: No new dependencies needed. The existing `tree-sitter`,
`tree-sitter-md`, and `tree-sitter-html` crates already export injection queries.

## Risks and Open Questions

### Performance with many injection regions

**Risk**: A Markdown file with 50+ code blocks could exceed the <1ms budget if all
injection trees are parsed and queried.

**Mitigation**: Only parse injection trees that intersect the visible viewport. A
60-line viewport will typically intersect at most 2-3 code blocks. Benchmark with
realistic "documentation file" scenarios.

### Query predicate parsing

**Risk**: Extracting `#set! injection.language "rust"` from query predicates uses
`Query::property_settings()` which returns `&[QueryProperty]`. The exact API may
differ between tree-sitter versions.

**Mitigation**: Pin to tree-sitter 0.22+ which has stable property settings API.
Add a test that exercises predicate extraction from both Markdown and HTML
injection queries.

### Injection regions with complex nesting

**Risk**: Some edge cases like code blocks inside blockquotes, or nested HTML
templates, may have unexpected byte offset calculations.

**Mitigation**: Start with the straightforward cases (top-level fenced blocks,
direct `<script>`/`<style>` children). Complex nesting can be addressed in
follow-up work if users report issues.

### LanguageRegistry lifetime

**Risk**: Storing a `LanguageRegistry` reference in `SyntaxHighlighter` requires
either `&'static` lifetime (impractical) or owned/`Arc` storage (memory cost).

**Decision**: Store an owned `LanguageRegistry` in the highlighter. At ~500 bytes
per language (13 languages × ~40 bytes each), this is acceptable. If memory
becomes a concern, switch to `Arc<LanguageRegistry>` shared across all buffers.

### Markdown inline content injection

**Risk**: The `tree-sitter-md` grammar has a separate inline grammar with its own
injection query (`INJECTION_QUERY_INLINE`). The current plan uses only the block
grammar's injection query.

**Decision**: Support block-level injections first (fenced code blocks). Inline
injections (e.g., inline code with language hints) are out of scope for this chunk.
The architecture supports adding inline grammar support later if needed.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION, not at planning time. -->