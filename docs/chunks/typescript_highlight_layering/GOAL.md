---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/src/registry.rs
code_references:
  - ref: crates/syntax/src/registry.rs#LanguageRegistry::new
    implements: "Combined JavaScript/TypeScript highlight query layering for .ts and .tsx files"
  - ref: crates/syntax/src/registry.rs#tests::test_typescript_highlights_javascript_keywords
    implements: "Test verifying JavaScript keyword highlighting in TypeScript files"
  - ref: crates/syntax/src/registry.rs#tests::test_typescript_highlights_string_literals
    implements: "Test verifying string literal highlighting in TypeScript files"
  - ref: crates/syntax/src/registry.rs#tests::test_tsx_highlights_javascript_keywords
    implements: "Test verifying JavaScript keyword highlighting in TSX files"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- base_snapshot_reload
- conflict_mode_lifecycle
- deletion_rename_handling
- file_change_events
- three_way_merge
---

# Chunk Goal

## Minor Goal

TypeScript files open with minimal syntax highlighting because `tree-sitter-typescript`'s `HIGHLIGHTS_QUERY` only covers TypeScript-specific constructs (type annotations, interfaces, enums, generics, etc.) while fundamental JavaScript constructs (keywords, functions, variables, string literals, operators) are defined in `tree-sitter-javascript`'s `HIGHLIGHT_QUERY`.

This is the same issue that was resolved for C++ in `registry.rs`: the C++ grammar's highlight query only covered C++-specific constructs (templates, namespaces, `this`), so it needed the C grammar's query layered underneath.

Apply the same pattern for TypeScript: combine JavaScript's `HIGHLIGHT_QUERY` as a base with TypeScript's `HIGHLIGHTS_QUERY` layered on top, for both the `.ts` and `.tsx` configurations.

## Success Criteria

- TypeScript (`.ts`) and TSX (`.tsx`) configs in `crates/syntax/src/registry.rs` use a combined highlight query: `tree_sitter_javascript::HIGHLIGHT_QUERY` + `"\n"` + `tree_sitter_typescript::HIGHLIGHTS_QUERY`
- Opening a `.ts` file highlights fundamental constructs (keywords like `const`/`let`/`function`/`return`, string literals, number literals, function names, operators) in addition to TypeScript-specific constructs (type annotations, interfaces, generics)
- Existing tests continue to pass
- A test opens a TypeScript snippet and asserts that highlights are produced for basic JavaScript-level constructs (e.g., `keyword` captures for `const`, `string` captures for string literals)