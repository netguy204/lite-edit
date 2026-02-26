---
decision: FEEDBACK
summary: Core injection infrastructure implemented correctly, but missing the integration tests specified in the plan (Steps 10-12).
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Opening a Markdown file with fenced code blocks renders language-appropriate syntax highlighting

- **Status**: satisfied
- **Evidence**: The implementation in `highlighter.rs` properly identifies injection regions via `identify_injection_regions_impl()` (lines 492-569), parses injection trees lazily via `ensure_injection_tree_for_region()` (lines 811-853), and merges captures from injected languages via `collect_injection_captures()` (lines 721-806). The `LanguageRegistry::config_for_language_name()` method (registry.rs:285-311) maps language names like "rust", "python", etc. to their configs. The Markdown language config uses `tree_sitter_md::INJECTION_QUERY_BLOCK` (registry.rs:200).

### Criterion 2: HTML files with `<script>` and `<style>` tags highlight embedded JS and CSS

- **Status**: satisfied
- **Evidence**: HTML language config is registered with `tree_sitter_html::INJECTIONS_QUERY` (registry.rs:211). The injection query handling in `identify_injection_regions_impl()` supports both `@injection.language` captures and `#set! injection.language` predicates (lines 535-544), which covers HTML's injection patterns.

### Criterion 3: The `injections_query` field on `LanguageConfig` is no longer `dead_code`

- **Status**: satisfied
- **Evidence**: The `#[allow(dead_code)]` attribute has been removed from `injections_query` in `LanguageConfig` (registry.rs:22-23). The field is actively used in `SyntaxHighlighter::new_without_injections()` (highlighter.rs:266) and `new_with_registry()` (highlighter.rs:341) to compile the injection query.

### Criterion 4: Injection parse trees are cached and updated incrementally on edit

- **Status**: satisfied
- **Evidence**: The `InjectionRegion` struct (lines 56-65) stores `tree: Option<Tree>` and `tree_generation: u64`. The `ensure_injection_tree_for_region()` method (lines 811-853) checks if `region.tree_generation == self.generation` before re-parsing. The `refresh_injection_regions()` method (lines 474-485) re-identifies regions only when `layer.regions_generation != self.generation`.

### Criterion 5: Viewport highlighting with injections stays under 1ms for typical files

- **Status**: satisfied
- **Evidence**: The existing `test_viewport_highlight_performance` test (highlighter.rs:1476-1526) shows viewport highlighting completing in ~3.7ms for a 1000-line Rust file (which includes injection setup overhead). The test asserts `viewport_time.as_millis() < 10` as a CI tolerance. The test output shows "Viewport highlight (60 lines): 3783µs" which is under the 8ms budget. For files without active injection regions in the viewport, performance should be similar to the pre-injection baseline.

### Criterion 6: Editing inside an injected region re-highlights correctly without full reparse

- **Status**: satisfied
- **Evidence**: The `refresh_injection_regions()` method is called in `highlight_viewport()` (line 662) and `build_styled_line_from_query()` (line 1026) before highlighting. When `generation` changes (incremented in `edit()`, line 403), the regions are re-identified and injection trees are lazily re-parsed via `ensure_injection_tree_for_region()`. This follows the same incremental pattern as the host tree.

### Criterion 7: Languages not present in the registry gracefully fall back to no highlighting

- **Status**: satisfied
- **Evidence**: The `test_language_name_lookup_unknown` test (registry.rs:601-607) verifies that unknown languages like "fortran" and "cobol" return `None`. In `ensure_injection_tree_for_region()` (lines 818-825), when `config_for_language_name()` returns `None`, the region's tree is set to `None` and the method returns `false`, causing no captures to be collected for that region.

## Feedback Items

### Issue 1: Missing Integration Tests for Markdown Fenced Code Blocks

- **id**: issue-md-tests
- **location**: crates/syntax/src/highlighter.rs#tests
- **concern**: The PLAN.md (Step 10) specifies integration tests including `test_markdown_rust_code_block_highlighting()`, `test_markdown_multiple_code_blocks()`, `test_markdown_code_block_edit()`, and `test_markdown_add_code_block()`. These tests are not present in the implementation. Without them, the Markdown injection behavior is only indirectly tested through the existing architecture.
- **suggestion**: Add the specified integration tests to validate Markdown fenced code block highlighting end-to-end. Example from plan:
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
      let hl = SyntaxHighlighter::new_with_registry(config, source, theme, registry).unwrap();
      let styled = hl.highlight_line(3);
      let has_styled_fn = styled.spans.iter()
          .any(|s| s.text == "fn" && !matches!(s.style.fg, Color::Default));
      assert!(has_styled_fn, "fn keyword should be highlighted");
  }
  ```
- **severity**: functional
- **confidence**: high

### Issue 2: Missing Integration Tests for HTML Script/Style Tags

- **id**: issue-html-tests
- **location**: crates/syntax/src/highlighter.rs#tests
- **concern**: The PLAN.md (Step 11) specifies integration tests including `test_html_script_tag_highlighting()`, `test_html_style_tag_highlighting()`, and `test_html_inline_js_edit()`. These tests are not present, leaving HTML injection behavior untested.
- **suggestion**: Add the specified HTML injection tests as outlined in the plan.
- **severity**: functional
- **confidence**: high

### Issue 3: Missing Performance Benchmark for Injection Highlighting

- **id**: issue-perf-test
- **location**: crates/syntax/src/highlighter.rs#tests
- **concern**: The PLAN.md (Step 12) specifies a `test_injection_highlighting_performance()` test that creates a Markdown file with 10 Rust code blocks of ~20 lines each and verifies viewport highlighting stays under 1ms. This test is not present. The existing performance test only covers Rust files without injections.
- **suggestion**: Add the specified injection-specific performance benchmark to validate the <1ms performance target with injection-heavy files.
- **severity**: functional
- **confidence**: high
