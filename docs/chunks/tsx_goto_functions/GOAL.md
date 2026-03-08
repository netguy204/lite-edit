---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/src/queries/typescript.rs
- crates/syntax/src/queries/mod.rs
- crates/syntax/src/registry.rs
- crates/syntax/src/gotodef.rs
code_references:
- ref: crates/syntax/src/queries/typescript.rs#LOCALS_QUERY
  implements: "Custom TypeScript/TSX locals.scm query capturing function declarations, arrow functions, class declarations, variable declarations, and parameters for scope-aware go-to-definition"
- ref: crates/syntax/src/registry.rs
  implements: "Wiring custom TS locals query and combined JS+TS tags query into LanguageRegistry for both TypeScript and TSX configs"
- ref: crates/syntax/src/gotodef.rs#make_tsx_resolver
  implements: "TSX test helpers and go-to-definition tests covering function declarations, arrow functions, JSX element resolution, and class declarations"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- highlight_md_inline
---

# Chunk Goal

## Minor Goal

Cmd+click (goto-definition) in `.tsx` files currently only resolves TypeScript-unique symbols like interfaces and type aliases. It fails to jump to function definitions — including when clicking on a function used as a JSX element (e.g., `<MyComponent />`).

**Root cause:** The upstream `tree_sitter_typescript::LOCALS_QUERY` is minimal — it only captures function parameters (`required_parameter` and `optional_parameter`) as `@local.definition`. It does not capture function declarations, variable declarations (including `const Foo = () => ...` arrow functions), or class declarations. This means the `LocalsResolver` (same-file goto-def) can almost never resolve references in TS/TSX files.

The cross-file `SymbolIndex` (via `tags.scm`) does capture function signatures, methods, interfaces, modules, and abstract classes — but it only captures TS-specific constructs, not the base JavaScript constructs like `function_declaration`, `class_declaration`, `variable_declarator`, or arrow functions. Since TSX extends JavaScript, the tags query for `.tsx` files needs the JavaScript definitions too.

**Fix approach:** Write a custom `locals.scm` query for TypeScript/TSX (similar to the existing custom queries for Rust and Python in `crates/syntax/src/queries/`) that captures function declarations, variable declarations, arrow functions, class declarations, and scopes. Additionally, investigate whether the tags query needs to be augmented with the JavaScript `TAGS_QUERY` (like highlights already combines JS + TS queries) so the cross-file index captures all definitions.

A secondary concern is JSX element resolution: when the user clicks on `<Foo>`, tree-sitter parses the tag name as a `jsx_opening_element` → `identifier` node. The `identifier_at_position()` function in `gotodef.rs` should already handle this (it checks for `"identifier"` kind), but this should be verified and tested.

## Success Criteria

- Cmd+click on a function name used as a call expression in a `.tsx` file jumps to its definition (same-file)
- Cmd+click on a `const Foo = () => ...` arrow function reference jumps to the `const` declaration (same-file)
- Cmd+click on `<MyComponent />` (JSX element) resolves to the `MyComponent` function/const definition
- Cmd+click on function definitions imported from other files falls through to `SymbolIndex` and resolves cross-file
- Existing goto-definition behavior for interfaces and type aliases continues to work
- Tests cover same-file resolution for function declarations, arrow functions, and JSX element references in TSX