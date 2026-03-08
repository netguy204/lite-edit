---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/src/queries/typescript.rs
- crates/syntax/src/gotodef.rs
code_references:
  - ref: crates/syntax/src/queries/typescript.rs#LOCALS_QUERY
    implements: "Import binding captures (named, default, namespace) as @local.definition for goto-definition fallback"
  - ref: crates/syntax/src/gotodef.rs#test_tsx_named_import_resolution
    implements: "Test: named import resolution (import { useState } from 'react')"
  - ref: crates/syntax/src/gotodef.rs#test_tsx_default_import_resolution
    implements: "Test: default import resolution (import React from 'react')"
  - ref: crates/syntax/src/gotodef.rs#test_tsx_namespace_import_resolution
    implements: "Test: namespace import resolution (import * as R from 'ramda')"
  - ref: crates/syntax/src/gotodef.rs#test_tsx_local_definition_shadows_import
    implements: "Test: local definitions take priority over import bindings"
  - ref: crates/syntax/src/gotodef.rs#test_tsx_type_import_resolution
    implements: "Test: type import resolution (import { type FC } from 'react')"
  - ref: crates/syntax/src/gotodef.rs#test_tsx_on_import_definition_returns_none
    implements: "Test: cursor on import definition itself returns None"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- tsx_goto_functions
created_after:
- tsx_goto_functions
---

# Chunk Goal

## Minor Goal

In Python and Rust, cmd+clicking on a symbol imported from an external library navigates to the import statement that brought it into scope. In TSX/TypeScript files, this doesn't work — cmd+clicking on an externally-imported symbol either does nothing or falls through to the cross-file `SymbolIndex` which can't resolve external packages.

The desired behavior: when the `LocalsResolver` fails to find a same-file definition and the `SymbolIndex` also has no match (because the symbol comes from an external library like `react`, `lodash`, etc.), goto-definition should fall back to navigating to the import statement where the symbol was introduced. For example, cmd+clicking `useState` in a TSX file should jump to `import { useState } from 'react'`.

This requires the goto-definition pipeline to recognize import bindings as a fallback resolution target. The `locals.scm` query for TypeScript/TSX (introduced in `tsx_goto_functions`) may need to capture import specifiers as `@local.definition`, or a separate import-aware resolution step may be needed in the goto-definition chain.

## Success Criteria

- Cmd+click on a symbol imported via named import (`import { foo } from 'bar'`) in a `.tsx` file jumps to that import statement
- Cmd+click on a default import (`import React from 'react'`) jumps to the import statement
- Cmd+click on a namespace import (`import * as R from 'ramda'`) jumps to the import statement
- Same-file definitions still take priority — if a symbol is both imported and redefined locally, goto-def resolves to the local definition
- This works as a fallback: only triggers when neither `LocalsResolver` nor `SymbolIndex` find a match
- Tests cover named, default, and namespace import resolution in TSX files