---
decision: FEEDBACK
summary: "Syntax layer complete (LocalsResolver, query files, registry wiring) but editor integration missing (commands, keybindings, jump stack, Cmd-click)"
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: `LanguageConfig.locals_query` is no longer dead code — it is loaded and used for go-to-definition resolution

- **Status**: satisfied
- **Evidence**: The `#[allow(dead_code)]` annotation was removed from `registry.rs` (line 26). The `locals_query` field is now populated with query constants for Rust (`queries::rust::LOCALS_QUERY`), Python (`queries::python::LOCALS_QUERY`), JavaScript (`tree_sitter_javascript::LOCALS_QUERY`), and TypeScript (`tree_sitter_typescript::LOCALS_QUERY`). The field is exported and documented in `registry.rs:27-29`.

### Criterion 2: `locals.scm` query files are present for at minimum Rust, Python, JavaScript, and TypeScript

- **Status**: satisfied
- **Evidence**: Created `crates/syntax/src/queries/rust.rs` with `LOCALS_QUERY` constant (96 lines). Created `crates/syntax/src/queries/python.rs` with `LOCALS_QUERY` constant (138 lines). JavaScript and TypeScript use their upstream `LOCALS_QUERY` from `tree_sitter_javascript` and `tree_sitter_typescript` crates. All queries compile and have the required captures (`@local.scope`, `@local.definition`, `@local.reference`) verified by unit tests.

### Criterion 3: Cmd-click (or a keyboard shortcut like Cmd-D or F12) on an identifier in a file buffer jumps the cursor to that identifier's definition within the same file

- **Status**: gap
- **Evidence**: No `GotoDefinition` command variant in `buffer_target.rs`. No Cmd+D or F12 keybindings wired to go-to-definition (grep for `Char('d')` shows only existing DeleteForwardWord binding). No Cmd-click handling in `editor_state.rs`. The `LocalsResolver` exists but is not wired to any user-facing action.

### Criterion 4: The resolution algorithm correctly handles: local variables, function parameters, locally-defined functions/closures, block-scoped variables, and simple shadowing (innermost scope wins)

- **Status**: satisfied
- **Evidence**: The `LocalsResolver::find_definition()` method in `gotodef.rs` implements scope-walking resolution. Tests verify: local variables (`test_rust_local_variable`, `test_python_local_variable`, `test_js_local_variable`), function parameters (`test_rust_function_parameter`, `test_python_function_parameter`), for loop variables (`test_rust_for_loop_variable`), and nested scope shadowing (`test_rust_nested_scope`). The algorithm sorts scopes by size (innermost first) and returns the first matching definition.

### Criterion 5: When no same-file definition is found (e.g., imported symbols, method calls on types), the editor provides clear feedback (e.g., a brief status message "definition not found in this file") rather than silently doing nothing

- **Status**: gap
- **Evidence**: No status message implementation. The `LocalsResolver` returns `None` when no definition is found (tested in `test_unknown_identifier_returns_none`), but there is no editor integration to display a message. PLAN.md Step 10 describes using `MiniBuffer` for status messages but this was not implemented.

### Criterion 6: Go-to-definition does not introduce perceptible latency — the locals query and scope walk must complete within the 8ms budget per GOAL.md

- **Status**: unclear
- **Evidence**: The investigation documented that locals queries on typical files (~500 captures) execute in ~200µs, well within budget. The `LocalsResolver` implementation uses efficient query cursor iteration and scope sorting. However, no explicit performance test exists for go-to-definition. The implementation appears sound for the budget.

### Criterion 7: A "go back" action (e.g., Cmd-[ or a back-navigation shortcut) returns the cursor to the pre-jump position (requires maintaining a simple jump stack)

- **Status**: gap
- **Evidence**: No `JumpStack` or `JumpPosition` structs in `workspace.rs`. No `GoBack` command in `buffer_target.rs`. Cmd+[ is already bound to `PrevWorkspace` in `global_shortcuts.rs:131-133`. PLAN.md Steps 6-8 describe the jump stack implementation but none of this was implemented.

## Feedback Items

### Issue 1: GotoDefinition command not implemented

- **id**: issue-gotodef-cmd
- **location**: crates/editor/src/buffer_target.rs
- **concern**: The `GotoDefinition` and `GoBack` command variants are not defined in the `Command` enum, and the command execution logic (PLAN.md Step 8) is not implemented.
- **suggestion**: Add `GotoDefinition` and `GoBack` variants to the `Command` enum. Implement `execute_command` handling that: (1) gets cursor position, (2) converts to byte offset, (3) calls `LocalsResolver::find_definition`, (4) pushes to jump stack and moves cursor on success, or (5) displays status message on failure.
- **severity**: functional
- **confidence**: high

### Issue 2: Keybindings not wired

- **id**: issue-keybindings
- **location**: crates/editor/src/buffer_target.rs:resolve_command
- **concern**: No keybindings map to `GotoDefinition`. PLAN.md specifies Cmd+D or F12. No keybinding maps to `GoBack`.
- **suggestion**: Add in `resolve_command()`: `Key::Char('d')` with Command modifier → `GotoDefinition`, and F12 → `GotoDefinition`. For GoBack, consider a different binding than Cmd+[ since that's used for workspace cycling—perhaps Ctrl+[ or Ctrl+-.
- **severity**: functional
- **confidence**: high

### Issue 3: Jump stack not implemented

- **id**: issue-jumpstack
- **location**: crates/editor/src/workspace.rs
- **concern**: The `JumpStack` and `JumpPosition` structs (PLAN.md Step 6) are not implemented. Back-navigation cannot work without them.
- **suggestion**: Add `JumpPosition` struct with `tab_id`, `line`, `col` fields. Add `JumpStack` with bounded Vec and push/pop methods. Add `jump_stack: JumpStack` field to `Workspace`.
- **severity**: functional
- **confidence**: high

### Issue 4: Tree accessor missing from SyntaxHighlighter

- **id**: issue-tree-accessor
- **location**: crates/syntax/src/highlighter.rs
- **concern**: The `tree()` accessor method (PLAN.md Step 4) is not implemented. Only `source()` exists. Without `tree()`, external code cannot run locals queries on the parse tree.
- **suggestion**: Add `pub fn tree(&self) -> &Tree { &self.tree }` to `SyntaxHighlighter`.
- **severity**: functional
- **confidence**: high

### Issue 5: Cmd-click handling not implemented

- **id**: issue-cmd-click
- **location**: crates/editor/src/editor_state.rs
- **concern**: PLAN.md Step 9 describes Cmd-click handling to invoke go-to-definition at the click position. This is not implemented—mouse events with Command modifier do not trigger go-to-definition.
- **suggestion**: In `handle_mouse()` or mouse event handling, check for Command modifier on mouse down. Convert click position to buffer position and invoke go-to-definition logic.
- **severity**: functional
- **confidence**: high

### Issue 6: Status message not displayed on failure

- **id**: issue-status-message
- **location**: crates/editor/src/buffer_target.rs or mini_buffer.rs
- **concern**: When go-to-definition fails (no same-file definition), there is no user feedback. PLAN.md Step 10 describes showing "Definition not found in this file" message.
- **suggestion**: When `LocalsResolver::find_definition` returns `None`, display a transient status message. Check existing status/message patterns in `MiniBuffer` or add a simple status display with auto-clear.
- **severity**: functional
- **confidence**: high
