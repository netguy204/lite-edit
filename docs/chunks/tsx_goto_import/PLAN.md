<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Extend the existing TypeScript/TSX `LOCALS_QUERY` in `crates/syntax/src/queries/typescript.rs`
with tree-sitter patterns that capture import bindings as `@local.definition`. This is the
same approach Python uses (see `crates/syntax/src/queries/python.rs` lines 124-131 for
`aliased_import` and `import_from_statement` patterns).

Because import statements are at module level (root scope), the `LocalsResolver` will find
them via `find_definition_at_root()` — but only after failing to find a same-file definition
in any inner scope. This gives us the "fallback" behavior described in the goal: local
definitions naturally take priority through scope-based resolution (innermost-first search).

No changes to the resolution pipeline in `editor_state.rs` are needed. The `LocalsResolver`
already handles root-scope definitions. No new resolver type is required.

The tree-sitter TypeScript grammar represents imports as:
- **Named imports** (`import { foo } from 'bar'`): `import_statement > import_clause > named_imports > import_specifier > identifier`
- **Default imports** (`import React from 'react'`): `import_statement > import_clause > identifier`
- **Namespace imports** (`import * as R from 'ramda'`): `import_statement > import_clause > namespace_import > identifier`

Per `docs/trunk/TESTING_PHILOSOPHY.md`, tests will be written first (failing) and then the
query patterns added to make them pass. Tests will assert semantic properties matching each
success criterion: named, default, and namespace import resolution, plus local-definition
priority over imports.

## Sequence

### Step 1: Write failing tests for import resolution

Add four new tests to `crates/syntax/src/gotodef.rs` using the existing `make_tsx_resolver()`
and `parse_tsx()` test helpers:

1. **`test_tsx_named_import_resolution`** — Source code with `import { useState } from 'react'`
   and a reference to `useState` later in the file. Assert that `find_definition` returns the
   byte range of the `useState` identifier inside the import specifier.

2. **`test_tsx_default_import_resolution`** — Source code with `import React from 'react'` and
   a reference to `React`. Assert resolution to the `React` identifier in the import clause.

3. **`test_tsx_namespace_import_resolution`** — Source code with `import * as R from 'ramda'`
   and a reference to `R`. Assert resolution to the `R` identifier in the namespace import.

4. **`test_tsx_local_definition_shadows_import`** — Source code with both
   `import { foo } from 'bar'` and `const foo = 42` in the same scope, with a reference to
   `foo` after the local definition. Assert that `find_definition` resolves to the local
   `const` declaration, NOT the import. This validates priority behavior.

Run `cargo test -p lite-edit-syntax` to confirm all four tests fail (the query doesn't capture
import bindings yet).

Location: `crates/syntax/src/gotodef.rs` (test module at bottom of file)

### Step 2: Add import binding patterns to LOCALS_QUERY

Add three new tree-sitter patterns to the `LOCALS_QUERY` constant in
`crates/syntax/src/queries/typescript.rs`, in the Definitions section:

```scheme
; Import bindings
; Named imports: import { foo, bar } from 'baz'
(import_specifier
  name: (identifier) @local.definition)

; Default imports: import Foo from 'bar'
(import_clause
  (identifier) @local.definition)

; Namespace imports: import * as Foo from 'bar'
(namespace_import
  (identifier) @local.definition)
```

These patterns capture the binding identifier (not the module source string) as a definition
at root scope, making them findable by the `LocalsResolver`'s `find_definition_at_root()`.

**Important caveat**: The `(import_clause (identifier))` pattern for default imports will
match the bare `identifier` child of `import_clause`. This is correct because default imports
produce `import_clause > identifier` in the tree (see exploration output). Named and namespace
imports produce different sub-structures (`named_imports`, `namespace_import`) that won't
match this pattern.

Also update the module-level doc comment and `///` doc comment on `LOCALS_QUERY` to mention
import binding captures.

Location: `crates/syntax/src/queries/typescript.rs`

### Step 3: Run tests and verify

Run `cargo test -p lite-edit-syntax` and verify all four new tests pass, plus all existing
tests continue to pass. The existing TSX tests (`test_tsx_function_declaration`,
`test_tsx_arrow_function`, `test_tsx_jsx_element_resolution`, `test_tsx_class_declaration`,
`test_tsx_typescript_local_variable_with_custom_query`) must not regress.

Also run the query compilation tests (`test_typescript_locals_query_compiles_ts`,
`test_typescript_locals_query_compiles_tsx`) to ensure the new patterns are valid.

### Step 4: Add edge case test for type imports

Add a test `test_tsx_type_import_resolution` verifying that `import { type FC } from 'react'`
with a reference to `FC` resolves to the import specifier. The tree-sitter grammar represents
this as `import_specifier > type > name: identifier`, so the same `import_specifier name:`
pattern should match. If it doesn't, adjust the pattern.

Location: `crates/syntax/src/gotodef.rs` (test module)

### Step 5: Add test for on-import-definition returns None

Add a test `test_tsx_on_import_definition_returns_none` that places the cursor ON the import
specifier identifier itself (e.g., cursor on `useState` in `import { useState } from 'react'`).
Assert that `find_definition` returns `None` — the existing `is_definition` check in
`LocalsResolver` should handle this, since the identifier node will be captured as both
`@local.definition` and `@local.reference`.

Location: `crates/syntax/src/gotodef.rs` (test module)

## Dependencies

- **tsx_goto_functions** (ACTIVE): Provides the custom `LOCALS_QUERY` and TSX test infrastructure (`make_tsx_resolver`, `parse_tsx`) that this chunk extends.

## Risks and Open Questions

- The `(import_clause (identifier))` pattern for default imports might also match other
  identifier children of `import_clause` in edge cases. The tree exploration shows default
  imports produce exactly one bare `identifier` child, while named imports produce a
  `named_imports` child. However, if future grammar versions change this structure, the
  pattern may need updating. The compilation and behavioral tests will catch any regression.
- `import { type FC }` (type-only imports) use an `import_specifier` with a `type` keyword
  child. The `name: (identifier)` field selector should still match since `name` is a
  field, not a positional child. Step 4 explicitly validates this.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->