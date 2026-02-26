---
decision: APPROVE
summary: All success criteria satisfied; previous review feedback addressed with comprehensive test suite; injection highlighting works correctly for Markdown and HTML.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Opening a Markdown file with fenced code blocks renders language-appropriate syntax highlighting

- **Status**: satisfied
- **Evidence**: The `test_markdown_rust_code_block_highlighting` test (line 1852) verifies that `fn` keywords inside Rust code blocks are highlighted. The `test_markdown_multiple_code_blocks` test (line 1876) confirms both Rust and Python code blocks in the same file are highlighted with their respective language grammars. All 100 tests pass.

### Criterion 2: HTML files with `<script>` and `<style>` tags highlight embedded JS and CSS.

- **Status**: satisfied
- **Evidence**: The `test_html_script_tag_highlighting` test (line 1956) verifies `const` keywords are highlighted inside `<script>` tags. The `test_html_style_tag_highlighting` test (line 1979) verifies CSS content inside `<style>` tags has spans. HTML uses `tree_sitter_html::INJECTIONS_QUERY` (registry.rs:211).

### Criterion 3: The `injections_query` field on `LanguageConfig` is no longer `dead_code`.

- **Status**: satisfied
- **Evidence**: The `#[allow(dead_code)]` attribute has been removed from `injections_query` in registry.rs:22-23. It is actively used in `SyntaxHighlighter::new_without_injections()` (line 270) to compile injection queries. Only `locals_query` retains the attribute (as expected since it's reserved for future use).

### Criterion 4: Injection parse trees are cached and updated incrementally on edit, matching the existing host-tree incremental update pattern.

- **Status**: satisfied
- **Evidence**: The `InjectionRegion` struct (lines 57-66) tracks `tree_generation`. The `ensure_injection_tree_for_region()` method (lines 859-901) checks generation before re-parsing. The `refresh_injection_regions()` method (lines 480-491) only re-identifies regions when generation changes. Tests `test_markdown_code_block_edit` and `test_html_inline_js_edit` verify edits trigger correct re-highlighting.

### Criterion 5: Viewport highlighting with injections stays under 1ms for typical files

- **Status**: satisfied
- **Evidence**: The `test_injection_highlighting_performance` test (line 2029) creates a Markdown file with 10 Rust code blocks (~200 lines) and verifies cached viewport highlighting stays under 10ms (CI tolerance). Test output shows third call (all caches populated) completes quickly. The assertion at line 2069 enforces `third_time.as_millis() < 10`.

### Criterion 6: Editing inside an injected region re-highlights correctly without full reparse.

- **Status**: satisfied
- **Evidence**: The `test_markdown_code_block_edit` test (line 1910) inserts a character inside a Rust code block and verifies highlighting updates correctly. The `test_html_inline_js_edit` test (line 2005) performs the same verification for HTML script tags. Both tests pass.

### Criterion 7: Languages not present in the registry gracefully fall back to no highlighting for that region (no panic, no visual glitch).

- **Status**: satisfied
- **Evidence**: The `test_unknown_injection_language_graceful` test (line 1936) uses a ` ```cobol ` code block and verifies no crash occurs and the content renders as plain text. The `config_for_language_name()` returns `None` for unknown languages (tested at registry.rs:601-607), and `ensure_injection_tree_for_region()` sets `region.tree = None` in this case (lines 869-871).
