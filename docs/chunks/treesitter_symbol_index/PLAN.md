<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds cross-file go-to-definition by building a workspace-wide symbol index from tree-sitter `tags.scm` queries. The strategy has three main components:

1. **SymbolIndex struct** (`crates/syntax/src/symbol_index.rs`): A thread-safe data structure (`Arc<RwLock<...>>`) that maps symbol names to definition locations `(file_path, line, col)`. The index stores only names and positions—no file contents—keeping memory proportional to symbol count.

2. **Background indexer thread**: On workspace open, spawn a thread that walks all source files (respecting `.gitignore` and exclusion patterns), parses each with tree-sitter, runs `TAGS_QUERY` to extract top-level definitions (`@definition.function`, `@definition.class`, etc.), and populates the index. The indexer must not block the editor's render loop.

3. **Go-to-definition fallback**: Extend `EditorState::goto_definition()` to consult the symbol index when `LocalsResolver::find_definition()` returns `None`. If multiple definitions match (e.g., `new()` in several files), present a disambiguation UI (selector overlay) rather than jumping arbitrarily.

**Key patterns used**:
- Follow `FileIndex` (`crates/editor/src/file_index.rs`) as the reference pattern for background-threaded indexing with watcher-based updates
- Use `TAGS_QUERY` constants exported by tree-sitter grammar crates (verified: rust, python, go, javascript, typescript all export `TAGS_QUERY`)
- Reuse `LanguageRegistry` to get the tree-sitter `Language` for parsing
- Store the index on `Workspace` (like `FileIndex`) so each workspace has its own index

**Testing approach** (per `docs/trunk/TESTING_PHILOSOPHY.md`):
- Unit tests for `SymbolIndex` CRUD operations
- Integration tests for indexing a tempdir with known files
- Tests for the disambiguation scenario (multiple matches)
- Performance test validating <5s index time for ~1000 files

## Subsystem Considerations

No existing subsystems are directly affected by this chunk. The renderer, spatial_layout, and viewport_scroll subsystems are not touched.

This chunk may warrant a future `treesitter_features` subsystem discovery if the pattern of "load query → run query → process captures → wire to editor action" becomes common across go-to-def, indent, and potential future features (refactoring, reference finding). However, that subsystem discovery is out of scope for this chunk.

## Sequence

### Step 1: Add tags_query field to LanguageConfig

Extend `LanguageConfig` in `crates/syntax/src/registry.rs` to include a `tags_query: &'static str` field. Populate it for languages that export `TAGS_QUERY`:

- Rust: `tree_sitter_rust::TAGS_QUERY`
- Python: `tree_sitter_python::TAGS_QUERY`
- Go: `tree_sitter_go::TAGS_QUERY`
- JavaScript: `tree_sitter_javascript::TAGS_QUERY`
- TypeScript: `tree_sitter_typescript::TAGS_QUERY`

For languages without tags queries (C, C++, JSON, TOML, Markdown, HTML, CSS, Bash), use an empty string.

**Location**: `crates/syntax/src/registry.rs`

**Test**: Add a unit test verifying `config.tags_query.len() > 0` for rust, python, go, js, ts.

---

### Step 2: Define SymbolLocation and SymbolIndex structs

Create `crates/syntax/src/symbol_index.rs` with:

```rust
// Chunk: docs/chunks/treesitter_symbol_index - Cross-file symbol index

/// Location of a symbol definition
pub struct SymbolLocation {
    pub file_path: PathBuf,
    pub line: usize,      // 0-indexed
    pub col: usize,       // 0-indexed
    pub kind: SymbolKind, // function, class, method, etc.
}

pub enum SymbolKind {
    Function,
    Class,
    Method,
    Module,
    Interface,
    Macro,
    Constant,
}

/// Thread-safe symbol index
pub struct SymbolIndex {
    // Maps symbol name → Vec<SymbolLocation> (multiple files can define same name)
    index: Arc<RwLock<HashMap<String, Vec<SymbolLocation>>>>,
    // True while initial indexing is running
    indexing: Arc<AtomicBool>,
}
```

Implement basic methods:
- `new() -> Self`
- `is_indexing(&self) -> bool`
- `insert(&self, name: String, loc: SymbolLocation)`
- `lookup(&self, name: &str) -> Vec<SymbolLocation>` (returns clone)
- `remove_file(&self, path: &Path)` (removes all symbols from a file)
- `clear(&self)`

**Location**: `crates/syntax/src/symbol_index.rs`

**Test**: Unit tests for insert, lookup, remove_file, multiple-definitions case.

---

### Step 3: Implement index_file function

Add `index_file()` that parses a single file and extracts symbol definitions:

```rust
pub fn index_file(
    index: &SymbolIndex,
    file_path: &Path,
    registry: &LanguageRegistry,
) -> Result<(), IndexError>
```

Implementation:
1. Determine file extension and get `LanguageConfig` from registry
2. Skip if `tags_query` is empty
3. Create a tree-sitter `Parser`, set language, parse file contents
4. Compile the `tags_query` into a `Query`
5. Run `QueryCursor` over the tree root
6. For each capture matching `@name` within a `@definition.*` pattern:
   - Extract the symbol name from the node text
   - Extract the symbol kind from the outer capture name (e.g., `definition.function` → `Function`)
   - Convert byte offset to (line, col) position
   - Insert into the index
7. Return Ok(())

Cache compiled queries in a `HashMap<&'static str, Query>` (keyed by language name) to avoid recompilation per file.

**Location**: `crates/syntax/src/symbol_index.rs`

**Test**: Integration test that indexes a tempdir with a known Rust file and verifies expected symbols are found.

---

### Step 4: Implement background indexer thread

Add `SymbolIndex::start_indexing()` that spawns a background thread:

```rust
pub fn start_indexing(
    root: PathBuf,
    registry: Arc<LanguageRegistry>,
) -> SymbolIndex
```

The spawned thread:
1. Sets `indexing = true`
2. Walks `root` recursively using `walkdir` or similar
3. Skips directories matching exclusion patterns:
   - Any path component starting with `.` (dotfiles/directories)
   - `target/` (Rust build)
   - `node_modules/`
   - Files matching `.gitignore` patterns (use `ignore` crate for gitignore parsing)
4. For each source file (matching supported extensions: rs, py, go, js, ts, tsx, jsx), calls `index_file()`
5. Sets `indexing = false` when complete

Use a batching approach: index files in groups of ~50, then yield to allow other operations. This prevents thread starvation.

**Location**: `crates/syntax/src/symbol_index.rs`

**Dependencies**: Add `walkdir = "2"` and `ignore = "0.4"` to `crates/syntax/Cargo.toml`

**Test**: Integration test with a tempdir containing ~100 files, verifying indexing completes and `is_indexing()` transitions false.

---

### Step 5: Add incremental update on file save

Extend `SymbolIndex` to support incremental updates:

```rust
pub fn update_file(&self, file_path: &Path, registry: &LanguageRegistry)
```

Implementation:
1. Call `remove_file(file_path)` to clear stale entries
2. Call `index_file()` to re-extract symbols from the updated file

This will be wired to the editor's file save path.

**Location**: `crates/syntax/src/symbol_index.rs`

**Test**: Unit test that modifies a file's content, calls `update_file`, and verifies the index reflects the change.

---

### Step 6: Add SymbolIndex to Workspace

Modify `Workspace` struct in `crates/editor/src/workspace.rs`:

```rust
pub struct Workspace {
    // ... existing fields ...
    /// Cross-file symbol index for go-to-definition
    pub symbol_index: Option<SymbolIndex>,
}
```

Initialize `symbol_index` to `None` initially. In `Workspace::new_with_root()` (or equivalent), if a root directory is provided, call `SymbolIndex::start_indexing(root, registry)`.

**Location**: `crates/editor/src/workspace.rs`

---

### Step 7: Wire incremental update to file save

In `EditorState::save_file()` (or the save path), after successfully writing the file to disk:

```rust
if let Some(ref index) = workspace.symbol_index {
    index.update_file(&file_path, &self.language_registry);
}
```

**Location**: `crates/editor/src/editor_state.rs`

---

### Step 8: Extend goto_definition to consult symbol index

Modify `EditorState::goto_definition()`:

1. First, try same-file resolution via `LocalsResolver::find_definition()` (existing code)
2. If `None`, extract the symbol name at cursor position (use the identifier node under cursor)
3. Query `workspace.symbol_index.lookup(symbol_name)`
4. If zero results: show status message "Definition not found"
5. If one result: jump to that location (open file if not already open, move cursor)
6. If multiple results: show disambiguation selector (Step 9)

To extract the symbol name at cursor:
- Use the parse tree from the highlighter
- Find the node at cursor byte offset
- Walk up to find an identifier node
- Extract its text

**Location**: `crates/editor/src/editor_state.rs`

**Test**: Integration test with a workspace containing two files, verifying cross-file jump works.

---

### Step 9: Implement disambiguation UI for multiple matches

When `symbol_index.lookup()` returns multiple `SymbolLocation`s, present a selector overlay showing each match with file path context.

Reuse the existing selector infrastructure (`Selector`, `SelectorTarget`) from the file picker:

1. Create `DefinitionSelector` similar to `FilePickerSelector`
2. Items are `SymbolLocation`s, displayed as `{file_path}:{line}` (e.g., `src/foo.rs:42`)
3. On selection, jump to that location
4. Pressing Escape dismisses without jumping

**Location**: `crates/editor/src/definition_selector.rs` (new file), wired from `editor_state.rs`

**Test**: Test that presenting 3 matches shows selector, selection jumps correctly, escape dismisses.

---

### Step 10: Add "indexing..." feedback

When `symbol_index.is_indexing()` is true and user triggers go-to-definition with no same-file result:
- Show status message "Indexing workspace..." instead of "Definition not found"
- This gracefully degrades startup experience

**Location**: `crates/editor/src/editor_state.rs`

---

### Step 11: Performance validation

Add an ignored benchmark test that:
1. Creates a tempdir with ~1000 source files (can generate synthetic Rust files with function definitions)
2. Measures time to complete `start_indexing()`
3. Asserts < 5 seconds

Use `#[test] #[ignore]` with manual invocation via `cargo test -- --ignored`.

**Location**: `crates/syntax/src/symbol_index.rs` (or `tests/`)

---

### Step 12: Export module and update lib.rs

Export the new module:
- `crates/syntax/src/lib.rs`: Add `pub mod symbol_index;` and re-export `SymbolIndex`, `SymbolLocation`, `SymbolKind`

**Location**: `crates/syntax/src/lib.rs`

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments:

```rust
// Chunk: docs/chunks/treesitter_symbol_index - Cross-file symbol index for go-to-definition
```

Place at module level for `symbol_index.rs` and at function level for `goto_definition` changes.

## Dependencies

**Chunk dependencies** (already satisfied per frontmatter):
- `treesitter_gotodef` - Provides same-file go-to-definition infrastructure (jump stack, `LocalsResolver`, F12 binding)

**New crate dependencies** (add to `crates/syntax/Cargo.toml`):
- `walkdir = "2"` - Recursive directory walking
- `ignore = "0.4"` - Gitignore pattern matching (respects `.gitignore` files automatically)

## Risks and Open Questions

1. **Query compilation caching**: The tags query must be compiled once per language, not per file. Use a `OnceCell` or `Lazy<HashMap<&str, Query>>` to cache compiled queries. If this is slow, consider compiling at LanguageRegistry construction time.

2. **Large monorepo performance**: The 5-second target for ~1000 files may be tight for very large projects. Mitigations:
   - Parallelize parsing with `rayon` (deferred optimization)
   - Batch file discovery and parsing
   - Profile to identify bottlenecks (file I/O vs parsing vs query execution)

3. **Symbol name collisions**: Common names like `new`, `init`, `main`, `test` will have many matches. The disambiguation UI must handle long lists gracefully (possibly truncating with "... and N more").

4. **File not in workspace**: If user opens a file outside the indexed workspace root, cross-file go-to-def won't work for symbols defined in that file. This is acceptable—document as a limitation.

5. **LanguageRegistry thread safety**: `LanguageRegistry` is currently not `Send + Sync`. The background thread needs access to it for extension→language mapping. Options:
   - Pass a cloned `LanguageRegistry` to the background thread (preferred—it's cheap to clone)
   - Wrap in `Arc<LanguageRegistry>` and ensure `Send + Sync`

6. **Identifier extraction at cursor**: Need a reliable way to get the identifier under cursor. Current plan: use the parse tree from `SyntaxHighlighter`, find the innermost node at cursor byte offset, then walk up to find an identifier node. Edge case: cursor between tokens or on whitespace.

7. **Disambiguation selector integration**: The selector infrastructure in `crates/editor` is tightly coupled to file picking. May need refactoring to make it generic, or implement a simpler purpose-built selector for definition results.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->