---
decision: APPROVE
summary: All success criteria satisfied; implementation correctly filters reference captures and fixes method capture interleaving using QueryMatches
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `from_capture_name` returns `None` for capture names starting with `"reference."`, preventing call sites from being indexed

- **Status**: satisfied
- **Evidence**: Lines 95-96 in `crates/syntax/src/symbol_index.rs` add the check `else if name.starts_with("reference.")` that returns `None`, preventing `@reference.call` and `@reference.implementation` captures from being indexed. The existing test for `from_capture_name` at line 524 validates the standard behavior, and the new `test_reference_captures_not_indexed` test (lines 838-903) verifies call sites are NOT indexed.

### Criterion 2: `index_file` correctly indexes methods inside `impl` blocks (e.g., `new()`, `start_indexing()`) by switching from `QueryCaptures` to `QueryMatches` or by buffering captures by match ID

- **Status**: satisfied
- **Evidence**: The `index_file` function (lines 286-362) now uses `cursor.matches()` instead of `cursor.captures()`. The comment at lines 324-327 explains the rationale: "Use QueryMatches instead of QueryCaptures to avoid interleaving issues. QueryMatches groups all captures for a single match together, ensuring we see both @name and @definition.* captures before processing." This eliminates the state machine that previously dropped methods due to capture interleaving.

### Criterion 3: A test verifies that a Rust file with `impl` blocks produces symbol index entries for methods defined within them

- **Status**: satisfied
- **Evidence**: The test `test_methods_in_impl_blocks_indexed` (lines 761-831) creates a file with `struct Foo` and an `impl Foo` block containing `new()`, `bar()`, and `baz()` methods. It explicitly asserts all three methods are indexed: "new method should be indexed", "bar method should be indexed", "baz method should be indexed".

### Criterion 4: A test verifies that function call sites (`@reference.call`) are NOT present in the index

- **Status**: satisfied
- **Evidence**: The test `test_reference_captures_not_indexed` (lines 838-903) creates a file with function definitions (`foo`, `bar`, `main`) and multiple call sites (`foo()` called 4 times). It asserts that "foo should appear exactly once (definition only)" at the definition location (line 1), not at call sites.

### Criterion 5: Existing symbol index tests continue to pass

- **Status**: satisfied
- **Evidence**: Running `cargo test -p lite-edit-syntax symbol_index` shows all 13 tests pass (1 ignored - performance test). Tests include: `test_symbol_index_insert_lookup`, `test_symbol_index_multiple_definitions`, `test_symbol_index_remove_file`, `test_symbol_index_lookup_nonexistent`, `test_symbol_kind_from_capture_name`, `test_byte_offset_to_position`, `test_index_file_rust`, `test_index_file_python`, `test_index_file_no_tags_query`, `test_start_indexing_with_tempdir`, `test_update_file_incremental`, plus the two new tests.
