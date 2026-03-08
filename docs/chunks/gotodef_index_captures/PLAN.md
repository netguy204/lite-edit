<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk fixes two bugs in `crates/syntax/src/symbol_index.rs` that prevent cross-file go-to-definition from working correctly, as identified in the `cross_file_goto_definition` investigation.

**Bug 1: Reference captures being indexed as definitions**

The `from_capture_name` function has incomplete filtering. When it encounters `@reference.call` or `@reference.implementation` captures from the tags query:
1. It checks if the name starts with `"definition."` — it doesn't
2. It checks if the name equals `"name"` — it doesn't
3. It falls through to the else branch where `kind_str = name`, which hits `_ => SymbolKind::Unknown`

This causes every function call site to be indexed as a symbol with `SymbolKind::Unknown`, polluting the index.

**Fix:** Return `None` for capture names starting with `"reference."`, same as we do for `"name"` captures.

**Bug 2: QueryCaptures delivers captures interleaved across matches**

The `index_file` function uses `QueryCaptures`, a `StreamingIterator` that delivers captures one at a time. The code assumes all captures for match N arrive before match N+1 starts. This assumption is violated when a node matches multiple query patterns simultaneously.

For methods inside `impl` blocks, the function matches both:
- `@definition.method` (inside `declaration_list`)
- `@definition.function` (general `function_item`)

The captures interleave: `@definition.method` (match 3) → `@definition.function` (match 4) → `@name` (match 3) → `@name` (match 4). When the state machine sees match 4 start, it tries to finalize match 3, but only has `symbol_kind` (no `symbol_name` yet), so the method is silently dropped.

**Fix:** Switch from `QueryCaptures` to `QueryMatches`. `QueryMatches` is a standard `Iterator` (not `StreamingIterator`) that yields `QueryMatch` objects containing all captures for a single match grouped together. This eliminates the interleaving problem and simplifies the code.

**Testing Strategy:** Following docs/trunk/TESTING_PHILOSOPHY.md:
- Write failing tests first that demonstrate the bugs
- Implement fixes to make tests pass
- Tests assert semantic properties (methods are indexed, call sites are NOT indexed)

## Subsystem Considerations

No existing subsystems are relevant to this bug fix. The symbol index is part of the tree-sitter infrastructure established by `treesitter_symbol_index` chunk, but no subsystem documentation exists for tree-sitter patterns in this codebase.

## Sequence

### Step 1: Write failing test for reference capture filtering

Add a test that creates a Rust file with function calls (which produce `@reference.call` captures) and verifies that the call sites are NOT indexed — only definitions should appear.

**Test file:** `crates/syntax/src/symbol_index.rs` (in the `#[cfg(test)]` module)

**Test structure:**
```rust
#[test]
fn test_reference_captures_not_indexed() {
    // Create a Rust file with:
    // - A function definition: `fn foo() {}`
    // - A call site: `foo();` inside another function
    // Index the file and verify:
    // - "foo" appears exactly once in the index (the definition)
    // - The location points to the definition line, not the call site
}
```

### Step 2: Fix from_capture_name to filter reference captures

Modify `SymbolKind::from_capture_name` in `crates/syntax/src/symbol_index.rs`:

**Current behavior (lines 88-112):**
```rust
fn from_capture_name(name: &str) -> Option<Self> {
    let kind_str = if name.starts_with("definition.") {
        &name["definition.".len()..]
    } else if name == "name" {
        return None;
    } else {
        name  // BUG: "reference.call" falls through here
    };
    Some(match kind_str { ... _ => SymbolKind::Unknown })
}
```

**Fix:** Add a check for `"reference."` prefix before the fallback:
```rust
fn from_capture_name(name: &str) -> Option<Self> {
    let kind_str = if name.starts_with("definition.") {
        &name["definition.".len()..]
    } else if name == "name" {
        return None;
    } else if name.starts_with("reference.") {
        return None;  // Filter out @reference.call, @reference.implementation
    } else {
        name
    };
    // ... rest unchanged
}
```

**Verify:** The test from Step 1 should now pass.

### Step 3: Write failing test for method capture in impl blocks

Add a test that creates a Rust file with methods inside `impl` blocks and verifies they ARE indexed.

**Test structure:**
```rust
#[test]
fn test_methods_in_impl_blocks_indexed() {
    // Create a Rust file with:
    // - A struct: `struct Foo {}`
    // - An impl block: `impl Foo { fn new() -> Self { ... } fn bar(&self) {} }`
    // Index the file and verify:
    // - "new" is in the index with kind Method or Function
    // - "bar" is in the index with kind Method or Function
    // - "Foo" is in the index (the struct)
}
```

This test will fail initially because the current `QueryCaptures` state machine drops methods due to capture interleaving.

### Step 4: Switch from QueryCaptures to QueryMatches

Refactor `index_file` in `crates/syntax/src/symbol_index.rs` to use `QueryMatches` instead of `QueryCaptures`.

**Current approach (lines 321-381):**
- Uses `cursor.captures()` which returns a `StreamingIterator`
- Manual state machine tracking `current_match_id`, `symbol_name`, `symbol_kind`
- Processes captures one at a time, grouping by match ID

**New approach:**
- Use `cursor.matches()` which returns a standard `Iterator<Item = QueryMatch>`
- Each `QueryMatch` contains all captures for that match, already grouped
- Iterate over matches, then iterate over captures within each match
- Simpler code, no state machine needed

**Implementation:**
```rust
let mut cursor = QueryCursor::new();
for query_match in cursor.matches(&query, tree.root_node(), content.as_bytes()) {
    let mut symbol_name: Option<String> = None;
    let mut symbol_kind: Option<SymbolKind> = None;
    let mut name_start_byte: Option<usize> = None;

    for capture in query_match.captures {
        let capture_name = query.capture_names()[capture.index as usize];
        if capture_name == "name" {
            symbol_name = capture.node.utf8_text(content.as_bytes()).ok().map(String::from);
            name_start_byte = Some(capture.node.start_byte());
        } else if let Some(kind) = SymbolKind::from_capture_name(capture_name) {
            symbol_kind = Some(kind);
        }
    }

    // Insert if we have both name and kind
    if let (Some(name), Some(kind), Some(start_byte)) = (symbol_name, symbol_kind, name_start_byte) {
        let (line, col) = byte_offset_to_position(&content, start_byte);
        let loc = SymbolLocation { file_path: file_path.to_path_buf(), line, col, kind };
        let mut guard = index.write().unwrap();
        guard.entry(name).or_default().push(loc);
    }
}
```

**Note:** Remove the `use streaming_iterator::StreamingIterator;` import since we no longer need it for this function (it may be used elsewhere).

**Verify:** The test from Step 3 should now pass.

### Step 5: Remove unused StreamingIterator import if no longer needed

Check if `streaming_iterator::StreamingIterator` is still used elsewhere in `symbol_index.rs`. If not, remove the import.

### Step 6: Run all existing tests

Run `cargo test -p lite-edit-syntax` to ensure:
- All existing symbol index tests pass
- The new tests pass
- No regressions in other syntax-related tests

### Step 7: Add a backreference comment

Add a chunk backreference to the `from_capture_name` function documenting the bug fix:

```rust
// Chunk: docs/chunks/gotodef_index_captures - Filter reference captures, fix method interleaving
fn from_capture_name(name: &str) -> Option<Self> {
```

## Dependencies

None. This chunk builds on the existing symbol index infrastructure from `treesitter_symbol_index` which is already ACTIVE.

## Risks and Open Questions

**Risk: Duplicate entries for methods**

Methods in impl blocks match BOTH `@definition.method` and `@definition.function` patterns. With `QueryMatches`, we'll get separate matches for each pattern. This could result in the same method being indexed twice.

**Mitigation:** After fixing the interleaving, check if duplicates appear. If they do, either:
1. De-duplicate in `index_file` by checking if a symbol at the same location already exists
2. Accept duplicates (both point to the same location, so go-to-definition will work)

Option 2 is simpler and has no functional impact — the disambiguation UI will just show the same method twice, which is acceptable for this bug fix. A future chunk can optimize de-duplication if needed.

**Risk: Performance regression**

`QueryMatches` may have different performance characteristics than `QueryCaptures`. The original code may have chosen `QueryCaptures` for performance.

**Mitigation:** The existing performance test (`test_indexing_performance_1000_files`) will catch any significant regression. The test asserts indexing 1000 files completes in under 5 seconds.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->