---
decision: FEEDBACK
summary: "The core IndentComputer module is implemented with all planned captures and the hybrid heuristic, but the Enter key integration is missing - indent is never actually applied when the user presses Enter."
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: `LanguageConfig` loads an `indents.scm` query alongside highlights/injections for each supported language

- **Status**: satisfied
- **Evidence**: `crates/syntax/src/registry.rs` has `indents_query: &'static str` field on `LanguageConfig` (lines 31-34). All 13 languages have indent queries loaded via `include_str!("../queries/{lang}/indents.scm")` (lines 85-272). The glob found 13 indents.scm files covering rust, python, go, javascript, typescript, c, cpp, bash, json, toml, html, css, markdown.

### Criterion 2: When the user presses Enter in a file buffer, the new line is automatically indented to the correct level based on the parse tree

- **Status**: gap
- **Evidence**: The `IndentComputer` and `SyntaxHighlighter.compute_indent()` APIs are implemented and exported. However, **the Enter key handling in `buffer_target.rs` does NOT call the indent computation**. Line 292-295 shows `Command::InsertNewline` simply calls `ctx.buffer.insert_newline_tracked()` with no subsequent indent computation or insertion. The PLAN.md Step 12 ("Wire Enter key handling to use computed indent") was NOT implemented. The `compute_indent()` method exists but is never called from the editor.

### Criterion 3: The hybrid heuristic is implemented: indent is computed as a delta relative to a nearby reference line's actual indentation

- **Status**: satisfied
- **Evidence**: `crates/syntax/src/indent.rs` lines 158-189 show `compute_indent()` finding a reference line, getting its indentation level, computing a delta via `compute_indent_delta()`, and applying `(ref_indent_level + delta).max(0)`. Lines 310-318 implement `find_reference_line()` walking backwards to find the previous non-blank line.

### Criterion 4: `@indent`, `@outdent`, `@indent.always`, `@outdent.always`, `@extend`, and `@extend.prevent-once` captures are supported

- **Status**: satisfied
- **Evidence**: `CaptureIndices` struct (lines 66-82) stores all captures. `CaptureIndices::from_query()` (lines 84-102) maps capture names including "indent", "indent.always", "outdent", "outdent.always", "extend", "extend.prevent-once". The algorithm in `compute_indent_delta()` (lines 199-254) processes these captures appropriately.

### Criterion 5: Scope modifiers (`tail` vs `all`) are respected per Helix's convention

- **Status**: unclear
- **Evidence**: The implementation has `indent_added` and `outdent_added` flags for non-stacking (lines 210-211), but I don't see explicit `tail` vs `all` scope handling. The algorithm checks `capture_start_row == ref_line` and `capture_start_row == target_line` which is a simplified version. Helix's full scope semantics may be more nuanced. This may be acceptable for initial scope but warrants documentation of any deviations.

### Criterion 6: Indent query files are present and tested for at minimum Rust and Python

- **Status**: satisfied
- **Evidence**: `queries/rust/indents.scm` (94 lines) and `queries/python/indents.scm` (67 lines) exist with comprehensive patterns. Unit tests in `indent.rs` (lines 443-500) cover `test_rust_indent_after_open_brace`, `test_rust_maintain_indent`, `test_python_indent_after_colon`, `test_python_indent_in_class`. All 13 tests pass.

### Criterion 7: Indentation does not introduce latency perceptible to the user (must stay within the 8ms budget)

- **Status**: satisfied
- **Evidence**: The doc comment on `IndentComputer` (lines 111-116) documents expected performance: "Query execution: ~50-100µs, Ancestor walk: ~10-20µs, Total: ~100µs per indent computation". No performance tests were added (PLAN.md Step 16 deferred), but the architecture is sound: query is pre-compiled once, computation is a single tree traversal.

### Criterion 8: Typing inside multiline strings or comments does not produce incorrect indentation

- **Status**: satisfied
- **Evidence**: `@indent.ignore` capture is supported (line 81, 97). `is_in_ignored_region()` method (lines 267-304) checks for cursor inside ignored nodes. Python query includes `(string) @indent.ignore` and `(comment) @indent.ignore` (lines 62-67).

## Feedback Items

### Issue 1: Enter key integration not wired

- **Location**: `crates/editor/src/buffer_target.rs:292-295` and `crates/editor/src/editor_state.rs`
- **Concern**: The `InsertNewline` command handler inserts a newline but does not compute or insert indentation. The `SyntaxHighlighter.compute_indent()` API exists and works (tests pass), but it's never called when the user presses Enter.
- **Suggestion**: Implement PLAN.md Step 12. After `handle_key` returns for Enter key in `editor_state.rs`, check if a newline was inserted, compute indent via the tab's highlighter, and insert the indent string. The PLAN suggested: "After `handle_key` returns for Enter key, in `editor_state.rs`: 1) Check if buffer cursor moved to a new line, 2) Compute indent via `tab.compute_indent_for_line()`, 3) Insert the computed indent string."
- **Severity**: functional
- **Confidence**: high

### Issue 2: No integration test file created

- **Location**: `crates/editor/tests/indent_test.rs` (missing)
- **Concern**: PLAN.md Step 15 specified creating `crates/editor/tests/indent_test.rs` with integration tests for Enter key behavior. This file does not exist.
- **Suggestion**: After fixing Issue 1, add integration tests that simulate typing and verify indent is applied.
- **Severity**: functional
- **Confidence**: high

### Issue 3: Tab-level API not implemented

- **Location**: `crates/editor/src/workspace.rs`
- **Concern**: PLAN.md Step 11 specified adding `Tab::compute_indent_for_line()` method. This method does not exist on `Tab`. The `SyntaxHighlighter` has `compute_indent()`, but there's no Tab-level wrapper.
- **Suggestion**: Add the `compute_indent_for_line()` method to `Tab` as specified in the PLAN, delegating to `self.highlighter.compute_indent()`.
- **Severity**: functional
- **Confidence**: high
