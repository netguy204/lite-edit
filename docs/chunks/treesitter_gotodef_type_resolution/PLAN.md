<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The Rust `LOCALS_QUERY` in `crates/syntax/src/queries/rust.rs` currently only
captures `(identifier) @local.reference` for references, and only captures
variable-like definitions (parameters, let bindings, function names). This
means `type_identifier` nodes (struct names, enum names, trait names, type
aliases) are invisible to the resolver.

The fix is straightforward:
1. Add `(type_identifier) @local.reference` to capture type references
2. Add definition patterns for type-defining constructs (`struct_item`,
   `enum_item`, `trait_item`, `type_item`, `union_item`)

The implementation follows TDD per `docs/trunk/TESTING_PHILOSOPHY.md`:
1. Write a failing test that uses a type identifier in a function signature
   and expects go-to-definition to resolve to the struct definition
2. Extend the `LOCALS_QUERY` to capture type identifiers and type definitions
3. Verify the test passes

The `LocalsResolver` in `crates/syntax/src/gotodef.rs` already handles name
matching by text equality, so once the query captures both the definition
name (e.g., `Span` in `struct Span { ... }`) and the reference (e.g., `Span`
in `fg: Span`), resolution will work automatically.

## Sequence

### Step 1: Write failing test for type_identifier resolution

Create a test in `crates/syntax/src/gotodef.rs` that:
- Defines a struct at the top of a function (e.g., `struct Point { x: i32, y: i32 }`)
- Uses that struct as a type in a function parameter or return type
- Calls `find_definition()` on the type reference position
- Asserts that the definition range is returned

This test will fail because `type_identifier` isn't currently captured as a
reference.

Location: `crates/syntax/src/gotodef.rs#tests`

### Step 2: Add type_identifier reference capture to Rust locals query

Add a new reference pattern to `LOCALS_QUERY`:

```
(type_identifier) @local.reference
```

This captures type identifiers (struct names, enum names, etc.) as references,
enabling the resolver to find them at cursor positions.

Location: `crates/syntax/src/queries/rust.rs#LOCALS_QUERY`

### Step 3: Add type-defining construct definition captures

Add definition patterns for:
- `struct_item` → capture the `name` field as `@local.definition`
- `enum_item` → capture the `name` field as `@local.definition`
- `trait_item` → capture the `name` field as `@local.definition`
- `type_item` → capture the `name` field as `@local.definition`
- `union_item` → capture the `name` field as `@local.definition`

Example pattern:
```
; Struct name definition
(struct_item
  name: (type_identifier) @local.definition)
```

Note: The definition names are `type_identifier` nodes, not regular `identifier`
nodes. The name-matching in `LocalsResolver` compares text, so this will match
`type_identifier` references to `type_identifier` definitions.

Location: `crates/syntax/src/queries/rust.rs#LOCALS_QUERY`

### Step 4: Verify existing tests pass

Run the existing `test_rust_locals_query_compiles` and
`test_rust_locals_query_has_expected_captures` tests to ensure the query
modifications compile correctly and maintain the expected capture names.

Command: `cargo test -p lite-edit-syntax locals`

### Step 5: Verify new test passes

Run the new type_identifier test to confirm type name resolution works.

Command: `cargo test -p lite-edit-syntax type_identifier`

### Step 6: Add additional type resolution tests

Add tests for:
- Enum type used in a variable binding (`let color: Color = ...`)
- Trait bounds (`fn foo<T: MyTrait>(...)`)
- Type alias usage (`type Result<T> = std::result::Result<T, MyError>`)
- Generic type parameters where the type is defined locally

These tests verify edge cases and ensure the implementation is robust.

Location: `crates/syntax/src/gotodef.rs#tests`

### Step 7: Update code_paths in GOAL.md

Add the modified files to the chunk's `code_paths` frontmatter:
- `crates/syntax/src/queries/rust.rs`
- `crates/syntax/src/gotodef.rs`

## Dependencies

This chunk builds on:
- `treesitter_gotodef`: Provides `LocalsResolver` and the existing Rust `LOCALS_QUERY`
- `treesitter_symbol_index`: Extended `identifier_at_position` to handle `type_identifier`

Both are ACTIVE chunks, so no blocking dependencies.

## Risks and Open Questions

1. **Generic type parameters**: Type parameters (`T` in `fn foo<T>`) are
   `type_identifier` nodes but defined via `type_parameters`. The current
   approach captures the `type_identifier` but may not correctly identify the
   definition site in the `type_parameters` node. Will verify with a test and
   add a pattern if needed.

2. **Associated types**: Types like `Self::Output` won't resolve with this
   approach because `Output` is defined in a trait. This is expected — cross-
   file and trait resolution requires LSP. The resolver will return `None` for
   these cases, which is correct behavior per the existing "definition not
   found" feedback.

3. **Module-level vs. local type definitions**: Most struct/enum/trait/type
   definitions are at module scope, not inside functions. The `find_definition`
   algorithm checks scopes from innermost to outermost and falls back to
   root-level definitions. This should work for module-level types, but needs
   verification via testing.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->