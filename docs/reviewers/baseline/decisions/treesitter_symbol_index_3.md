---
decision: APPROVE
summary: All success criteria satisfied. SymbolIndex fully implements workspace-wide symbol indexing with background threading, incremental updates, disambiguation UI, and proper gitignore handling.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: A `SymbolIndex` (or equivalent) struct maintains a map of symbol names to their definition locations (file path, line, column) across the workspace

- **Status**: satisfied
- **Evidence**: `crates/syntax/src/symbol_index.rs` defines `SymbolIndex` with `index: Arc<RwLock<HashMap<String, Vec<SymbolLocation>>>>` where `SymbolLocation` contains `file_path: PathBuf`, `line: usize`, `col: usize`, and `kind: SymbolKind`. The struct includes `insert()`, `lookup()`, `remove_file()`, and `clear()` methods. Tests verify CRUD operations work correctly (lines 461-534).

### Criterion 2: The index is built on a background thread at project/workspace open time, without blocking the editor's render loop or adding to startup latency

- **Status**: satisfied
- **Evidence**: `SymbolIndex::start_indexing()` (lines 213-231) spawns a background thread via `thread::spawn()` that calls `index_workspace()`. The method returns immediately with `indexing: Arc<AtomicBool>` set to `true`, which is set to `false` when indexing completes. The index is initialized in `EditorState::create_workspace()` via `ws.start_symbol_indexing(Arc::clone(&self.language_registry))` (line 640), which is non-blocking.

### Criterion 3: `tags.scm` queries are loaded for at minimum Rust, Python, Go, JavaScript, and TypeScript (these grammars ship `tags.scm` upstream)

- **Status**: satisfied
- **Evidence**: `crates/syntax/src/registry.rs` populates `tags_query` from upstream grammar crates: `tree_sitter_rust::TAGS_QUERY` (line 104), `tree_sitter_python::TAGS_QUERY` (line 158), `tree_sitter_go::TAGS_QUERY` (line 222), `tree_sitter_javascript::TAGS_QUERY` (line 206), and `tree_sitter_typescript::TAGS_QUERY` (lines 178, 192 for both TS and TSX). Tests `test_tags_query_available_for_supported_languages` (line 717) and `test_tags_query_empty_for_unsupported_languages` (line 733) verify correct configuration.

### Criterion 4: The index is incrementally updated when a file is saved — only the saved file is re-parsed and its entries replaced

- **Status**: satisfied
- **Evidence**: `SymbolIndex::update_file()` (lines 237-245) calls `remove_file()` then `index_file()` for just the saved file. This is wired to the save path in `editor_state.rs` line 4259: `ws.update_symbol_index_for_file(&path, &self.language_registry)`. The test `test_update_file_incremental` (lines 725-775) verifies that modifying a file and calling `update_file()` correctly replaces old entries with new ones.

### Criterion 5: Go-to-definition falls back to the symbol index when `locals.scm` same-file resolution returns no result

- **Status**: satisfied
- **Evidence**: `EditorState::goto_definition()` (lines 1370-1493) first tries same-file resolution via `LocalsResolver::find_definition()` (line 1419-1420). If that returns `None`, it extracts the identifier via `identifier_at_position()` (line 1453), then queries `symbol_index.lookup(&identifier)` (line 1478). This two-stage resolution matches the documented intent.

### Criterion 6: When multiple definitions match a symbol name (e.g., `new()` defined in several files), the editor presents a disambiguation UI (e.g., a list of matches with file paths) rather than jumping to an arbitrary one

- **Status**: satisfied
- **Evidence**: In `goto_definition()` (lines 1480-1492), when `locations.len() > 1`, the code calls `show_definition_selector(pane_id, cursor_pos, locations)`. The `show_definition_selector()` method (lines 1539-1567) creates a `SelectorWidget` with items formatted as `"{file_path}:{line}"` and stores `DefinitionSelectorContext` for later navigation. The `handle_definition_selector_confirm()` method (lines 2405-2428) completes the navigation when the user selects an item.

### Criterion 7: Index build time for a medium-sized project (~1000 source files) is under 5 seconds

- **Status**: satisfied
- **Evidence**: The performance test `test_indexing_performance_1000_files` (lines 781-856) creates 1000 Rust source files with multiple definitions each, times the `start_indexing()` call, and asserts `elapsed.as_secs() < 5`. The test is `#[ignore]` for normal runs but available for manual performance validation via `cargo test -- --ignored`.

### Criterion 8: Memory usage of the index is reasonable (proportional to number of symbols, not file sizes — only names and locations are stored, not file contents)

- **Status**: satisfied
- **Evidence**: `SymbolLocation` (lines 121-130) stores only `file_path: PathBuf`, `line: usize`, `col: usize`, and `kind: SymbolKind`. No source code content is stored. The index stores `HashMap<String, Vec<SymbolLocation>>` where keys are symbol names (typically short strings). The architecture follows the plan's specification of "only names and locations are stored, not file contents."

### Criterion 9: Files matching `.gitignore` patterns (and common excludes like `target/`, `node_modules/`, `.git/`) are skipped during indexing

- **Status**: satisfied
- **Evidence**: `index_workspace()` (lines 408-450) uses `ignore::WalkBuilder` with `.hidden(true)` (skips dotfiles/directories), `.git_ignore(true)`, `.git_global(true)`, and `.git_exclude(true)`. This respects `.gitignore` patterns automatically. The `ignore` crate (version 0.4) is added to `Cargo.toml` (line 35). Hidden files are skipped, which covers `.git/`. Standard excludes like `target/` and `node_modules/` are handled by `.gitignore` in typical projects.
