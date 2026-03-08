---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: null
investigation: cross_file_goto_definition
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after: ["alt_screen_viewport_reset"]
---

# Chunk Goal

## Minor Goal

Fix two bugs in the symbol index (`crates/syntax/src/symbol_index.rs`) that prevent cross-file go-to-definition from working correctly:

1. **Filter out `@reference.*` captures**: The `from_capture_name` function falls through to `SymbolKind::Unknown` for `@reference.call` and `@reference.implementation` captures from the tags query. This pollutes the index with every function call site and impl block, not just definitions.

2. **Fix capture interleaving for methods**: The `index_file` function uses `QueryCaptures` (a `StreamingIterator`) with a state machine that assumes all captures for a match arrive before the next match begins. This assumption is violated when a node matches multiple query patterns (e.g., methods in `impl` blocks match both `@definition.method` and `@definition.function`). The interleaved delivery causes methods to be silently dropped from the index.

These fixes are required before cross-file go-to-definition can work, as the index is currently missing all method definitions and polluted with call-site references.

## Success Criteria

- `from_capture_name` returns `None` for capture names starting with `"reference."`, preventing call sites from being indexed
- `index_file` correctly indexes methods inside `impl` blocks (e.g., `new()`, `start_indexing()`) by switching from `QueryCaptures` to `QueryMatches` or by buffering captures by match ID
- A test verifies that a Rust file with `impl` blocks produces symbol index entries for methods defined within them
- A test verifies that function call sites (`@reference.call`) are NOT present in the index
- Existing symbol index tests continue to pass
