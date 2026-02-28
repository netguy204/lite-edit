---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/gotodef.rs
  - crates/syntax/src/queries/mod.rs
  - crates/syntax/src/queries/rust.rs
  - crates/syntax/src/queries/python.rs
  - crates/syntax/src/registry.rs
  - crates/syntax/src/highlighter.rs
  - crates/syntax/src/lib.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
code_references:
  - ref: crates/syntax/src/gotodef.rs#LocalsResolver
    implements: "Core go-to-definition resolution using tree-sitter locals queries"
  - ref: crates/syntax/src/gotodef.rs#LocalsResolver::find_definition
    implements: "Scope-walking algorithm to find definition for identifier at cursor position"
  - ref: crates/syntax/src/gotodef.rs#CaptureIndices
    implements: "Capture index lookup for @local.scope, @local.definition, @local.reference"
  - ref: crates/syntax/src/queries/rust.rs#LOCALS_QUERY
    implements: "Rust locals.scm query for scope/definition/reference captures"
  - ref: crates/syntax/src/queries/python.rs#LOCALS_QUERY
    implements: "Python locals.scm query for scope/definition/reference captures"
  - ref: crates/syntax/src/registry.rs#LanguageConfig::locals_query
    implements: "Locals query field on LanguageConfig now populated for Rust, Python, JS, TS"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::tree
    implements: "Public accessor exposing parse tree for go-to-definition queries"
  - ref: crates/editor/src/workspace.rs#JumpStack
    implements: "Bounded stack for tracking cursor positions before go-to-def jumps"
  - ref: crates/editor/src/workspace.rs#JumpPosition
    implements: "Position record (tab_id, pane_id, line, col) for jump stack"
  - ref: crates/editor/src/buffer_target.rs#Command::GotoDefinition
    implements: "Command variant for F12 go-to-definition"
  - ref: crates/editor/src/buffer_target.rs#Command::GoBack
    implements: "Command variant for Ctrl+- back navigation"
  - ref: crates/editor/src/editor_state.rs#EditorState::goto_definition
    implements: "Go-to-definition entry point coordinating resolver, jump stack, and cursor movement"
  - ref: crates/editor/src/editor_state.rs#EditorState::go_back
    implements: "Go back navigation popping from jump stack and restoring cursor"
  - ref: crates/editor/src/editor_state.rs#StatusMessage
    implements: "Transient status message for 'Definition not found' feedback"
narrative: null
investigation: treesitter_editing
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- incremental_parse
created_after:
- pty_wakeup_reliability
---

# Chunk Goal

## Minor Goal

Add same-file go-to-definition using tree-sitter `locals.scm` queries. When a user Cmd-clicks (or uses a keyboard shortcut) on a symbol in a file buffer, the cursor jumps to that symbol's definition within the same file.

This chunk activates the existing `locals_query` field on `LanguageConfig` (currently `#[allow(dead_code)]`), loads `locals.scm` query files for supported languages, and implements the scope-walking resolution algorithm: run the locals query to collect `@local.scope`, `@local.definition`, and `@local.reference` captures, identify the reference node at the cursor position, walk enclosing scopes upward matching by text equality, and jump to the first matching definition.

This is the highest-value go-to-definition tier — resolving local variables, parameters, and locally-defined functions covers the most common "where is this thing defined?" question during editing. Cross-file resolution is handled by a separate chunk (`treesitter_symbol_index`). LSP integration remains out of scope per GOAL.md.

## Success Criteria

- `LanguageConfig.locals_query` is no longer dead code — it is loaded and used for go-to-definition resolution
- `locals.scm` query files are present for at minimum Rust, Python, JavaScript, and TypeScript
- Cmd-click (or a keyboard shortcut like Cmd-D or F12) on an identifier in a file buffer jumps the cursor to that identifier's definition within the same file
- The resolution algorithm correctly handles: local variables, function parameters, locally-defined functions/closures, block-scoped variables, and simple shadowing (innermost scope wins)
- When no same-file definition is found (e.g., imported symbols, method calls on types), the editor provides clear feedback (e.g., a brief status message "definition not found in this file") rather than silently doing nothing
- Go-to-definition does not introduce perceptible latency — the locals query and scope walk must complete within the 8ms budget per GOAL.md
- A "go back" action (e.g., Cmd-[ or a back-navigation shortcut) returns the cursor to the pre-jump position (requires maintaining a simple jump stack)

## Rejected Ideas

### Use tree-sitter-highlight's built-in locals processing

The `tree-sitter-highlight` crate's `HighlightConfiguration` accepts a locals query and processes it internally for highlight consistency.

Rejected because: lite-edit does not use `tree-sitter-highlight` (it was benchmarked at 14.5ms, 182% of the 8ms budget, and replaced with direct `QueryCursor` usage). The locals processing inside `tree-sitter-highlight` is not exposed as a reusable API — it's embedded in the C highlight loop. We need to implement scope-walking ourselves using `Query` + `QueryCursor`.

### Implement full cross-file resolution in this chunk

Resolve imported symbols and cross-file definitions alongside same-file resolution.

Rejected because: Cross-file resolution requires a workspace-wide symbol index (background file parsing, `tags.scm` queries, name→location map). This is architecturally distinct work with its own performance and memory considerations. Same-file go-to-def provides immediate value with zero cross-file infrastructure. Cross-file is handled by the `treesitter_symbol_index` chunk.