---
decision: FEEDBACK
summary: "Core symbol index infrastructure is implemented and tested, but editor integration is incomplete: no go-to-definition fallback, no disambiguation UI, no incremental update on save, and no indexing initialization"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: A `SymbolIndex` (or equivalent) struct maintains a map of symbol names to their definition locations (file path, line, column) across the workspace

- **Status**: satisfied
- **Evidence**: `SymbolIndex` struct implemented in `crates/syntax/src/symbol_index.rs` with `HashMap<String, Vec<SymbolLocation>>` storage. `SymbolLocation` contains `file_path: PathBuf`, `line: usize`, `col: usize`, and `kind: SymbolKind`. The struct supports `insert()`, `lookup()`, `remove_file()`, `clear()`, and `symbol_count()` operations. Unit tests verify these operations work correctly.

### Criterion 2: The index is built on a background thread at project/workspace open time, without blocking the editor's render loop or adding to startup latency

- **Status**: gap
- **Evidence**: `SymbolIndex::start_indexing()` spawns a background `std::thread` that walks files and populates the index asynchronously. The `is_indexing()` method allows checking completion status. `Workspace.symbol_index: Option<SymbolIndex>` field exists. **However**, `start_symbol_indexing()` is never called - it's defined on `Workspace` (line 789) but no code invokes it at workspace creation time. The symbol index remains `None`.

### Criterion 3: `tags.scm` queries are loaded for at minimum Rust, Python, Go, JavaScript, and TypeScript (these grammars ship `tags.scm` upstream)

- **Status**: satisfied
- **Evidence**: `LanguageConfig` has `tags_query: &'static str` field. In `registry.rs`, tags queries are configured for: Rust (`tree_sitter_rust::TAGS_QUERY`), Python (`tree_sitter_python::TAGS_QUERY`), Go (`tree_sitter_go::TAGS_QUERY`), JavaScript (`tree_sitter_javascript::TAGS_QUERY`), TypeScript (`tree_sitter_typescript::TAGS_QUERY`), and TSX. Test `test_tags_query_available_for_supported_languages` verifies these are non-empty.

### Criterion 4: The index is incrementally updated when a file is saved — only the saved file is re-parsed and its entries replaced

- **Status**: gap
- **Evidence**: `SymbolIndex::update_file()` and `Workspace::update_symbol_index_for_file()` methods exist and correctly implement the logic (remove_file + re-index). **However**, `save_file()` in `editor_state.rs` does NOT call `update_symbol_index_for_file()`. The incremental update is never triggered.

### Criterion 5: Go-to-definition falls back to the symbol index when `locals.scm` same-file resolution returns no result

- **Status**: gap
- **Evidence**: `goto_definition()` in `editor_state.rs` (lines 1327-1416) only uses `LocalsResolver::find_definition()` for same-file resolution. When it returns `None`, the code shows "Definition not found in this file" status message. There is **no fallback** to consult `workspace.symbol_index`. The symbol index lookup is not integrated.

### Criterion 6: When multiple definitions match a symbol name (e.g., `new()` defined in several files), the editor presents a disambiguation UI (e.g., a list of matches with file paths) rather than jumping to an arbitrary one

- **Status**: gap
- **Evidence**: No `definition_selector.rs` file exists. No `DefinitionSelector` or disambiguation UI is implemented. The PLAN.md Step 9 describes creating this component, but it was not implemented. The `SymbolIndex::lookup()` returns `Vec<SymbolLocation>` correctly, but there's no consumer that handles multiple results with a selector.

### Criterion 7: Index build time for a medium-sized project (~1000 source files) is under 5 seconds

- **Status**: unclear
- **Evidence**: PLAN.md Step 11 calls for a performance benchmark test with `#[test] #[ignore]`. This test does not exist. While the implementation uses reasonable approaches (no redundant parsing, streaming iteration), there's no benchmark to verify the <5s target. Since indexing is never invoked in the editor, this also cannot be tested in practice.

### Criterion 8: Memory usage of the index is reasonable (proportional to number of symbols, not file sizes — only names and locations are stored, not file contents)

- **Status**: satisfied
- **Evidence**: `SymbolLocation` stores only `PathBuf`, `usize` (line), `usize` (col), and `SymbolKind`. File contents are read transiently in `index_file()` and not retained. The index structure is `HashMap<String, Vec<SymbolLocation>>` - memory is proportional to symbol count, not file sizes.

### Criterion 9: Files matching `.gitignore` patterns (and common excludes like `target/`, `node_modules/`, `.git/`) are skipped during indexing

- **Status**: satisfied
- **Evidence**: `index_workspace()` uses `ignore::WalkBuilder` with `.hidden(true)`, `.git_ignore(true)`, `.git_global(true)`, `.git_exclude(true)`. This correctly respects `.gitignore` patterns and skips hidden directories (including `.git/`). `Cargo.toml` shows `ignore = "0.4"` dependency added.

## Feedback Items

### Issue 1: Symbol indexing not initialized at workspace creation

- **ID**: issue-init
- **Location**: `crates/editor/src/workspace.rs:740` and integration points
- **Concern**: `symbol_index` is initialized to `None` but `start_symbol_indexing()` is never called. The symbol index remains unpopulated.
- **Suggestion**: In `EditorState` initialization (or workspace creation path), call `workspace.start_symbol_indexing(Arc::clone(&language_registry))` when a workspace has a root path.
- **Severity**: functional
- **Confidence**: high

### Issue 2: goto_definition does not fall back to symbol index

- **ID**: issue-fallback
- **Location**: `crates/editor/src/editor_state.rs:1411-1414`
- **Concern**: When `LocalsResolver::find_definition()` returns `None`, the code shows "Definition not found in this file" instead of consulting the symbol index for cross-file matches.
- **Suggestion**: Before showing "not found", check `workspace.symbol_index.lookup(symbol_name)`. If matches exist, proceed to disambiguation or single-jump. If `is_indexing()`, show "Indexing workspace..." message.
- **Severity**: functional
- **Confidence**: high

### Issue 3: Missing disambiguation UI for multiple matches

- **ID**: issue-disambig
- **Location**: `crates/editor/src/definition_selector.rs` (missing)
- **Concern**: PLAN.md Step 9 describes implementing a `DefinitionSelector` using the existing selector infrastructure. This file does not exist. Without it, multiple-match scenarios cannot be handled properly.
- **Suggestion**: Implement `DefinitionSelector` following the `FilePickerSelector` pattern, displaying `{file_path}:{line}` for each match. Wire it into `goto_definition()` when `lookup()` returns multiple results.
- **Severity**: functional
- **Confidence**: high

### Issue 4: Incremental index update not wired to file save

- **ID**: issue-save
- **Location**: `crates/editor/src/editor_state.rs:4034-4096` (`save_file()`)
- **Concern**: `save_file()` does not call `workspace.update_symbol_index_for_file()`. Saved files are not re-indexed, causing stale index entries.
- **Suggestion**: After successful file write, add: `workspace.update_symbol_index_for_file(&path, &self.language_registry);`
- **Severity**: functional
- **Confidence**: high

### Issue 5: Missing "indexing..." feedback in goto_definition

- **ID**: issue-status
- **Location**: `crates/editor/src/editor_state.rs` (goto_definition)
- **Concern**: PLAN.md Step 10 specifies showing "Indexing workspace..." when `is_indexing()` is true and no same-file result is found. This graceful degradation is not implemented.
- **Suggestion**: Check `symbol_index.is_indexing()` before returning "Definition not found" and show appropriate status message.
- **Severity**: functional
- **Confidence**: high

### Issue 6: Missing performance benchmark test

- **ID**: issue-perf
- **Location**: `crates/syntax/src/symbol_index.rs` (tests section)
- **Concern**: PLAN.md Step 11 specifies an `#[ignore]` benchmark test to validate <5s indexing time for ~1000 files. This test does not exist.
- **Suggestion**: Add a `#[test] #[ignore]` test that creates a tempdir with ~1000 synthetic source files, runs `start_indexing()`, and asserts completion within 5 seconds.
- **Severity**: style
- **Confidence**: medium
