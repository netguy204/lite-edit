---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/queries/rust.rs
  - crates/syntax/src/gotodef.rs
code_references:
  - ref: crates/syntax/src/queries/rust.rs#LOCALS_QUERY
    implements: "Type-defining constructs (struct_item, enum_item, trait_item, type_item, union_item) as @local.definition and type_identifier as @local.reference"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_type_identifier_in_function_parameter
    implements: "Test verifying struct type resolution in function parameters"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_enum_type_in_variable_binding
    implements: "Test verifying enum type resolution in variable bindings"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_type_alias_resolution
    implements: "Test verifying type alias resolution in return types"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_trait_resolution
    implements: "Test verifying trait name resolution in impl blocks"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_struct_in_generic_type
    implements: "Test verifying struct type resolution within generic type parameters (Vec<Span>)"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_on_type_definition_returns_none
    implements: "Test verifying cursor on type definition returns None"
  - ref: crates/syntax/src/gotodef.rs#tests::test_rust_type_in_struct_field
    implements: "Test verifying type resolution in struct field types"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- treesitter_gotodef
- treesitter_symbol_index
- viewport_keystroke_jostle
- indent_multiline
---

# Chunk Goal

## Minor Goal

The Rust locals query in `crates/syntax/src/queries/rust.rs` only captures
`(identifier)` as `@local.reference` and only defines variable-like bindings
(parameters, let, for, match arms, function names) as `@local.definition`. This
means same-file go-to-definition fails for **type names** — `type_identifier`
nodes like struct names, enum names, trait names, and type aliases are invisible
to the locals resolver.

This chunk adds type-level definition and reference patterns to the Rust locals
query so that cmd+click / F12 on a type name (e.g., `Span` in `Vec<Span>`,
`Color` in `fg: Color`) resolves to its definition within the same file.

## Success Criteria

- The Rust `LOCALS_QUERY` captures `(type_identifier) @local.reference`
- The Rust `LOCALS_QUERY` captures definition names for `struct_item`,
  `enum_item`, `trait_item`, `type_item`, and `union_item` as
  `@local.definition`
- Cmd+click on a type name (e.g., `Span` in `Vec<Span>`) in
  `crates/buffer/src/buffer_view.rs` navigates to the struct definition in the
  same file
- Existing variable/function go-to-definition behavior is unaffected
- The locals query compiles (existing `test_rust_locals_query_compiles` test
  passes)
- A targeted test verifies type_identifier resolution (e.g., a test source with
  a struct definition and a usage in a function signature, confirming the
  resolver returns the definition range)