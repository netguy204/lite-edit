---
decision: FEEDBACK
summary: "All iteration 1 issues resolved; implementation functionally complete but missing the #[ignore] performance benchmark test specified in PLAN.md Step 11"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: A `SymbolIndex` (or equivalent) struct maintains a map of symbol names to their definition locations (file path, line, column) across the workspace

- **Status**: satisfied
- **Evidence**: `SymbolIndex` struct implemented in `crates/syntax/src/symbol_index.rs` with `HashMap<String, Vec<SymbolLocation>>` storage. `SymbolLocation` contains `file_path: PathBuf`, `line: usize`, `col: usize`, and `kind: SymbolKind`. Unit tests verify insert, lookup, remove_file, and multiple-definitions scenarios all work correctly.

### Criterion 2: The index is built on a background thread at project/workspace open time, without blocking the editor's render loop or adding to startup latency

- **Status**: satisfied
- **Evidence**: `SymbolIndex::start_indexing()` spawns a background `std::thread` that walks files and populates the index asynchronously. The `is_indexing()` method tracks completion status. `start_symbol_indexing()` is now called in `EditorState::new()` (line 640) and when creating new workspaces via directory picker (line 4598). This ensures indexing starts immediately when a workspace is created.

### Criterion 3: `tags.scm` queries are loaded for at minimum Rust, Python, Go, JavaScript, and TypeScript (these grammars ship `tags.scm` upstream)

- **Status**: satisfied
- **Evidence**: `LanguageConfig` has `tags_query: &'static str` field. In `registry.rs`, tags queries are configured for: Rust (`tree_sitter_rust::TAGS_QUERY`), Python (`tree_sitter_python::TAGS_QUERY`), Go (`tree_sitter_go::TAGS_QUERY`), JavaScript (`tree_sitter_javascript::TAGS_QUERY`), TypeScript (`tree_sitter_typescript::TAGS_QUERY`), and TSX. Tests `test_tags_query_available_for_supported_languages` and `test_tags_query_empty_for_unsupported_languages` verify this.

### Criterion 4: The index is incrementally updated when a file is saved — only the saved file is re-parsed and its entries replaced

- **Status**: satisfied
- **Evidence**: `SymbolIndex::update_file()` and `Workspace::update_symbol_index_for_file()` methods exist. The `save_file()` method in `editor_state.rs` (line 4259) now calls `ws.update_symbol_index_for_file(&path, &self.language_registry)` after successful file write. The implementation correctly removes existing entries for the file before re-indexing.

### Criterion 5: Go-to-definition falls back to the symbol index when `locals.scm` same-file resolution returns no result

- **Status**: satisfied
- **Evidence**: `goto_definition()` in `editor_state.rs` (lines 1370-1493) first tries `LocalsResolver::find_definition()`. If it returns `None`, the code extracts the identifier at cursor using `identifier_at_position()`, looks up the workspace's symbol index, and handles the result appropriately. The `identifier_at_position()` function in `gotodef.rs` (lines 252-299) handles multiple identifier node kinds across languages.

### Criterion 6: When multiple definitions match a symbol name (e.g., `new()` defined in several files), the editor presents a disambiguation UI (e.g., a list of matches with file paths) rather than jumping to an arbitrary one

- **Status**: satisfied
- **Evidence**: `DefinitionSelectorContext` struct (lines 278-285) stores context for disambiguation. When `symbol_index.lookup()` returns multiple results, `show_definition_selector()` (lines 1539-1567) creates a `SelectorWidget` showing `{file_path}:{line}` for each match. Selection is handled by `handle_definition_selector_confirm()` (lines 2405-2428) which navigates to the chosen definition. The disambiguation UI reuses the existing selector infrastructure rather than creating a separate file.

### Criterion 7: Index build time for a medium-sized project (~1000 source files) is under 5 seconds

- **Status**: unclear
- **Evidence**: PLAN.md Step 11 calls for a `#[test] #[ignore]` benchmark test to validate <5s indexing time for ~1000 files. This test does not exist in the codebase. While the implementation uses reasonable approaches (ignore crate for fast walking, streaming iteration, no redundant parsing), there's no benchmark to verify the <5s target. The implementation compiles and runs, but the performance assertion is untested.

### Criterion 8: Memory usage of the index is reasonable (proportional to number of symbols, not file sizes — only names and locations are stored, not file contents)

- **Status**: satisfied
- **Evidence**: `SymbolLocation` stores only `PathBuf` (file_path), `usize` (line), `usize` (col), and `SymbolKind` (enum). File contents are read transiently in `index_file()` and not retained. The index structure is `HashMap<String, Vec<SymbolLocation>>` — memory is proportional to symbol count, not file sizes.

### Criterion 9: Files matching `.gitignore` patterns (and common excludes like `target/`, `node_modules/`, `.git/`) are skipped during indexing

- **Status**: satisfied
- **Evidence**: `index_workspace()` uses `ignore::WalkBuilder` with `.hidden(true)`, `.git_ignore(true)`, `.git_global(true)`, `.git_exclude(true)`. This correctly respects `.gitignore` patterns and skips hidden directories (including `.git/`, `target/`, `node_modules/`). `Cargo.toml` shows `ignore = "0.4"` dependency added.

## Feedback Items

### Issue 1: Missing performance benchmark test

- **ID**: issue-perf-benchmark
- **Location**: `crates/syntax/src/symbol_index.rs` (tests section)
- **Concern**: PLAN.md Step 11 specifies an `#[test] #[ignore]` benchmark test to validate <5s indexing time for ~1000 files. This test does not exist. While the success criterion for "under 5 seconds" is reasonable based on the implementation approach, there's no test to verify or guard against regression.
- **Suggestion**: Add a `#[test] #[ignore]` test that:
  1. Creates a tempdir with ~1000 synthetic Rust/Python source files (e.g., each with a few function definitions)
  2. Measures time to complete `SymbolIndex::start_indexing()`
  3. Asserts completion within 5 seconds
  4. Run with `cargo test -- --ignored` to execute
- **Severity**: style
- **Confidence**: medium
