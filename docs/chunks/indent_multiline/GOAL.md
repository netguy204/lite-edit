---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/syntax/src/indent.rs
code_references:
  - ref: crates/syntax/src/indent.rs#IndentComputer::compute_indent_delta
    implements: "Multiline check logic for @indent captures - containers only trigger indent if spanning multiple lines"
  - ref: crates/syntax/src/indent.rs#IndentComputer::is_container_node
    implements: "Classification of bracket/container node types (argument_list, list, tuple, etc.) that require multiline check"
  - ref: crates/syntax/src/indent.rs#IndentComputer::line_ends_with_indent_delimiter
    implements: "Fallback heuristic for incomplete syntax when tree-sitter captures don't fire"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on:
- treesitter_indent
created_after:
- terminal_unicode_env
- incremental_parse
- tab_rendering
- treesitter_indent
---

# Chunk Goal

## Minor Goal

Fix auto-indent incorrectly increasing indent after single-line bracket
expressions in Python (and likely other languages). Currently, typing
`main()` inside a block and pressing Enter produces an extra indent level
because the `argument_list` node `()` matches `@indent` in the query,
even though it's a single-line expression that shouldn't trigger indent.

The root cause is in `compute_indent_delta()` (`crates/syntax/src/indent.rs`
lines 199-255): it counts `@indent` captures that start on the reference line
without checking whether the captured node actually spans multiple lines.
Bracket/container nodes like `argument_list`, `list`, `dictionary`, `set`,
`tuple`, `parenthesized_expression`, `parameters`, `lambda`,
`list_comprehension`, etc. should only trigger indent when they span multiple
lines (i.e., the opening bracket is on one line and the content/close is on
another).

Block-introducing statements (`function_definition`, `if_statement`, etc.)
are not affected because their tree-sitter nodes inherently span multiple
lines when they have a body.

## Success Criteria

- Pressing Enter after `main()` inside a Python block maintains the current
  indent level (does not add an extra level)
- Pressing Enter after a single-line list `[1, 2, 3]` maintains indent level
- Multi-line argument lists still indent correctly:
  ```python
  foo(
      arg1,  # ← correctly indented
  )
  ```
- Multi-line lists/dicts/sets still indent correctly
- Block-introducing statements (`def`, `if`, `for`, `class`, etc.) still
  indent correctly after their colon
- Existing unit tests in `crates/syntax/src/indent.rs` continue to pass
- New unit tests cover the single-line bracket regression case
- The fix applies to all languages with bracket/container `@indent` captures,
  not just Python