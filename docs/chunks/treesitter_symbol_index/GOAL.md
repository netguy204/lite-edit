---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/symbol_index.rs
  - crates/syntax/src/registry.rs
  - crates/syntax/src/lib.rs
  - crates/syntax/Cargo.toml
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/syntax/src/symbol_index.rs#SymbolIndex
    implements: "Thread-safe symbol index data structure with background indexing and incremental updates"
  - ref: crates/syntax/src/symbol_index.rs#SymbolIndex::start_indexing
    implements: "Background thread spawning for workspace-wide symbol indexing"
  - ref: crates/syntax/src/symbol_index.rs#SymbolIndex::lookup
    implements: "Symbol name to definition locations lookup"
  - ref: crates/syntax/src/symbol_index.rs#SymbolIndex::update_file
    implements: "Incremental index update on file save"
  - ref: crates/syntax/src/symbol_index.rs#SymbolLocation
    implements: "Symbol definition location (file path, line, column, kind)"
  - ref: crates/syntax/src/symbol_index.rs#SymbolKind
    implements: "Classification of symbol types (function, class, method, etc.)"
  - ref: crates/syntax/src/registry.rs#LanguageConfig::tags_query
    implements: "Tags query field for tree-sitter symbol extraction per language"
  - ref: crates/editor/src/workspace.rs#Workspace::symbol_index
    implements: "Workspace ownership of the symbol index"
  - ref: crates/editor/src/workspace.rs#Workspace::start_symbol_indexing
    implements: "Workspace method to initiate background symbol indexing"
  - ref: crates/editor/src/workspace.rs#Workspace::update_symbol_index_for_file
    implements: "Workspace method to update index for a saved file"
  - ref: crates/editor/src/editor_state.rs#EditorState::goto_definition
    implements: "Two-stage go-to-definition: same-file locals then cross-file symbol index"
  - ref: crates/editor/src/editor_state.rs#EditorState::goto_cross_file_definition
    implements: "Navigation to definition in another file"
  - ref: crates/editor/src/editor_state.rs#EditorState::show_definition_selector
    implements: "Disambiguation UI for multiple matching definitions"
  - ref: crates/editor/src/editor_state.rs#DefinitionSelectorContext
    implements: "Context for definition disambiguation selector overlay"
  - ref: crates/syntax/src/gotodef.rs#identifier_at_position
    implements: "Identifier extraction at cursor for cross-file symbol lookup"
narrative: null
investigation: treesitter_editing
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- treesitter_gotodef
created_after:
- pty_wakeup_reliability
---

# Chunk Goal

## Minor Goal

Add cross-file go-to-definition by building a workspace-wide symbol index from tree-sitter `tags.scm` queries. When same-file go-to-definition (from `treesitter_gotodef`) finds no match, fall back to searching a project-wide index of top-level symbol definitions (functions, structs, classes, modules, traits, constants) and jump to the matching file and location.

This chunk introduces a background symbol indexer that, on project open, walks all source files in the workspace, parses each with tree-sitter, runs `tags.scm` queries to extract top-level definition names and their locations, and stores them in a name→(file, line, col) map. The index is incrementally updated on file save. The go-to-definition action (from `treesitter_gotodef`) is extended to consult this index as a fallback when `locals.scm` resolution finds no same-file match.

This is the furthest tree-sitter can go toward go-to-definition without LSP. It covers the common case of jumping to a function, struct, or class defined in another file within the same project. It cannot resolve method calls on typed values, trait implementations, or macro-expanded names — those require semantic analysis from a language server.

## Success Criteria

- A `SymbolIndex` (or equivalent) struct maintains a map of symbol names to their definition locations (file path, line, column) across the workspace
- The index is built on a background thread at project/workspace open time, without blocking the editor's render loop or adding to startup latency
- `tags.scm` queries are loaded for at minimum Rust, Python, Go, JavaScript, and TypeScript (these grammars ship `tags.scm` upstream)
- The index is incrementally updated when a file is saved — only the saved file is re-parsed and its entries replaced
- Go-to-definition falls back to the symbol index when `locals.scm` same-file resolution returns no result
- When multiple definitions match a symbol name (e.g., `new()` defined in several files), the editor presents a disambiguation UI (e.g., a list of matches with file paths) rather than jumping to an arbitrary one
- Index build time for a medium-sized project (~1000 source files) is under 5 seconds
- Memory usage of the index is reasonable (proportional to number of symbols, not file sizes — only names and locations are stored, not file contents)
- Files matching `.gitignore` patterns (and common excludes like `target/`, `node_modules/`, `.git/`) are skipped during indexing

## Rejected Ideas

### Use GitHub's stack-graphs for cross-file resolution

Stack-graphs build a graph of name bindings across files, enabling more accurate resolution than flat name matching (e.g., handling imports and re-exports).

Rejected because: Stack-graphs require authoring language-specific `.tsg` (tree-sitter graph) files, which are complex and only exist for a handful of languages. The simpler ctags-style approach (flat name→location index from `tags.scm`) covers the 80% case with far less complexity. Stack-graphs could be explored later if the flat index proves insufficient.

### Run indexing synchronously on project open

Parse all files on the main thread before showing the editor.

Rejected because: This would violate GOAL.md's <200ms cold startup target. A medium project with 1000 files could take several seconds to index. Background indexing preserves startup responsiveness — go-to-definition gracefully degrades (returns "indexing...") until the index is ready.