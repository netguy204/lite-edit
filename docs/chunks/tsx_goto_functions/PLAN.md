<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Two-pronged fix following the established pattern of custom query files (see `crates/syntax/src/queries/rust.rs` and `python.rs`) and the highlight-layering pattern (JS+TS combined query, established in the `typescript_highlight_layering` chunk):

1. **Custom `locals.scm` for TypeScript/TSX** — Write a `crates/syntax/src/queries/typescript.rs` module providing a `LOCALS_QUERY` constant that captures function declarations, arrow functions (via `variable_declarator`), class declarations, `const`/`let`/`var` bindings, and proper scopes. The upstream `tree_sitter_typescript::LOCALS_QUERY` only captures `required_parameter` and `optional_parameter`, which is far too minimal for goto-def to work.

2. **Combined JS+TS tags query** — Apply the same layering pattern used for highlights: concatenate `tree_sitter_javascript::TAGS_QUERY` with `tree_sitter_typescript::TAGS_QUERY` so the `SymbolIndex` captures both JS-base constructs (function declarations, class declarations, arrow functions assigned to variables) and TS-specific constructs (interfaces, type aliases, abstract classes, modules). Currently the TS/TSX tags query only captures TS-specific symbols.

3. **Tests first (TDD)** — Per TESTING_PHILOSOPHY.md, write failing tests for each success criterion before implementing. Tests cover: same-file function declaration resolution, arrow function resolution, JSX element resolution, and cross-file fallthrough verification.

## Sequence

### Step 1: Write failing tests for TSX same-file goto-def

Add test helper functions and test cases to `crates/syntax/src/gotodef.rs`:

- `make_tsx_resolver()` — Creates a `LocalsResolver` using the TSX language and the new custom locals query
- `parse_tsx()` — Parses TSX code using `tree_sitter_typescript::LANGUAGE_TSX`
- `test_tsx_function_declaration()` — Cmd+click on `greet()` call resolves to `function greet()` definition
- `test_tsx_arrow_function()` — Cmd+click on `Foo` reference resolves to `const Foo = () => ...` declaration
- `test_tsx_jsx_element_resolution()` — Cmd+click inside `<MyComponent />` resolves to the `MyComponent` function/const definition
- `test_tsx_class_declaration()` — Cmd+click on class name reference resolves to `class Foo {}` definition

These tests should reference the new custom query via `crate::queries::typescript::LOCALS_QUERY` and will fail to compile until Step 2.

Location: `crates/syntax/src/gotodef.rs` (test module)

### Step 2: Create custom TypeScript locals query

Create `crates/syntax/src/queries/typescript.rs` with a `LOCALS_QUERY` constant. This query must compile against both `LANGUAGE_TYPESCRIPT` and `LANGUAGE_TSX` grammars.

**Scopes** (nodes that create a new name-resolution scope):
```
[
  (statement_block)
  (function_expression)
  (function_declaration)
  (arrow_function)
  (method_definition)
  (class_declaration)
  (class)
  (for_statement)
  (for_in_statement)
  (while_statement)
  (do_statement)
  (if_statement)
  (switch_case)
] @local.scope
```

**Definitions** (nodes that introduce a name):
- `(variable_declarator name: (identifier) @local.definition)` — catches `const Foo = ...`, `let x = ...`, `var y = ...`
- `(function_declaration name: (identifier) @local.definition)` — `function foo() {}`
- `(class_declaration name: (type_identifier) @local.definition)` — `class Foo {}`
- `(required_parameter (identifier) @local.definition)` — TS function params
- `(optional_parameter (identifier) @local.definition)` — TS optional params
- `(pattern/identifier) @local.definition` — destructuring patterns (if the TSX grammar supports the `pattern/` anchor; otherwise use `(required_parameter pattern: (identifier) ...)`)

**References**:
- `(identifier) @local.reference`
- `(type_identifier) @local.reference` — for type references like interface names

Include unit tests in the module:
- `test_typescript_locals_query_compiles_ts()` — Compiles against `LANGUAGE_TYPESCRIPT`
- `test_typescript_locals_query_compiles_tsx()` — Compiles against `LANGUAGE_TSX`
- `test_typescript_locals_query_has_expected_captures()` — Has `@local.scope`, `@local.definition`, `@local.reference`

Register the module in `crates/syntax/src/queries/mod.rs` by adding `pub mod typescript;`.

Location: `crates/syntax/src/queries/typescript.rs`, `crates/syntax/src/queries/mod.rs`

### Step 3: Wire custom locals query into registry

Update `crates/syntax/src/registry.rs` to use the new custom query for both TypeScript and TSX:

**TypeScript config** (line ~175): Replace `tree_sitter_typescript::LOCALS_QUERY` with `queries::typescript::LOCALS_QUERY`.

**TSX config** (line ~189): Replace `tree_sitter_typescript::LOCALS_QUERY` with `queries::typescript::LOCALS_QUERY`.

Add backreference comments:
```rust
// Chunk: docs/chunks/tsx_goto_functions - Custom locals query for TS/TSX go-to-definition
```

Location: `crates/syntax/src/registry.rs`

### Step 4: Combine JS+TS tags queries for cross-file resolution

In `crates/syntax/src/registry.rs`, create a combined tags query following the same pattern as `ts_combined_query` for highlights:

```rust
// Chunk: docs/chunks/tsx_goto_functions - Combined JS/TS tags for cross-file go-to-definition
let ts_combined_tags: &'static str = Box::leak(
    format!("{}\n{}", tree_sitter_javascript::TAGS_QUERY, tree_sitter_typescript::TAGS_QUERY)
        .into_boxed_str(),
);
```

Use `ts_combined_tags` for both the TypeScript and TSX `LanguageConfig` entries (replacing the `tree_sitter_typescript::TAGS_QUERY` argument).

This ensures the `SymbolIndex` captures:
- **From JS tags**: `function_declaration`, `function_expression`, `generator_function`, `class_declaration`, arrow functions assigned via `lexical_declaration`/`variable_declaration`, method definitions
- **From TS tags**: `function_signature`, `method_signature`, `abstract_method_signature`, `abstract_class_declaration`, `module`, `interface_declaration`

Location: `crates/syntax/src/registry.rs`

### Step 5: Verify JSX element identifier extraction

Verify that `identifier_at_position()` already handles JSX elements correctly. In TSX, `<MyComponent />` is parsed as `jsx_self_closing_element` → `identifier` (for the tag name). The `identifier` node kind is already in `IDENTIFIER_KINDS`, so no code change should be needed.

Add a test `test_identifier_at_position_tsx_jsx_element()` in `gotodef.rs` that:
1. Parses `const App = () => <MyComponent />;` with the TSX parser
2. Calls `identifier_at_position()` at the position of `MyComponent` inside the JSX
3. Asserts it returns `Some("MyComponent")`

Location: `crates/syntax/src/gotodef.rs` (test module)

### Step 6: Run full test suite and iterate

Run `cargo test -p syntax` (or equivalent) to confirm:
- The new custom query compiles against both TS and TSX grammars
- All new TSX goto-def tests pass (function declarations, arrow functions, JSX elements, classes)
- The existing TypeScript test (`test_typescript_local_variable`) now passes instead of being skipped
- All existing Rust, Python, JavaScript tests still pass
- Symbol index tests for TS/TSX files still pass with the combined tags query

Fix any compilation errors or test failures from node type mismatches (the TSX grammar may use slightly different node names than expected — consult `tree-sitter-typescript` grammar source if needed).

### Step 7: Update GOAL.md code_paths

Update the `code_paths` field in `docs/chunks/tsx_goto_functions/GOAL.md` with the files touched:
- `crates/syntax/src/queries/typescript.rs` (new file)
- `crates/syntax/src/queries/mod.rs` (add module)
- `crates/syntax/src/registry.rs` (wire custom query + combined tags)
- `crates/syntax/src/gotodef.rs` (tests only)

## Risks and Open Questions

- **TSX grammar node types**: The exact node names in `tree-sitter-typescript` 0.23 may differ from what's documented above. For example, arrow function parameter patterns may use `(identifier)` directly rather than `(pattern/identifier)`. If the query fails to compile, inspect the grammar's `node-types.json` or use `tree.root_node().to_sexp()` on sample code to discover the actual node structure.

- **JS TAGS_QUERY predicates**: The JavaScript `TAGS_QUERY` uses `#strip!`, `#select-adjacent!`, and `#not-eq?` predicates. Tree-sitter's Rust binding ignores unknown predicates at query execution time (they're treated as metadata). The JS tags query already works for `.js` files in the `SymbolIndex`, so combining it with the TS query should work without predicate handling. However, if unexpected behavior arises, the `#not-eq?` predicate (which tree-sitter does understand natively) could interact with the TS query patterns.

- **`type_identifier` vs `identifier` for classes**: In the TS grammar, `class_declaration` uses `type_identifier` for the name, but JSX references to classes use `identifier`. The locals query captures both `(identifier)` and `(type_identifier)` as references, and the `find_definition()` algorithm matches by text equality, so cross-kind matches (identifier referencing a type_identifier definition) should work. But this needs verification.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
