<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is in `compute_indent_delta()` (lines 199-255 of `crates/syntax/src/indent.rs`).
Currently, when a node marked `@indent` starts on the reference line, we add +1 to
the indent delta unconditionally. This is correct for block-introducing constructs
like `function_definition` or `if_statement` (which inherently span multiple lines
when they have a body), but incorrect for bracket/container nodes like `argument_list`,
`list`, `tuple`, etc. These should only trigger indent when they span multiple lines.

**The fix**: Before counting an `@indent` capture, check if the captured node spans
multiple lines (`node.start_position().row != node.end_position().row`). If the node
is single-line, skip it.

This is a semantic bug fix with localized scope. The change modifies only the
`@indent` capture handling in `compute_indent_delta()`. No query file changes are
required—the existing Python and Rust queries are correct; the algorithm is wrong.

Per `docs/trunk/TESTING_PHILOSOPHY.md`, we follow TDD: write failing tests first,
then implement the fix to make them pass.

## Sequence

### Step 1: Write failing tests for single-line bracket regression

Add unit tests to `crates/syntax/src/indent.rs` that demonstrate the bug:

1. `test_python_single_line_call_no_indent`: After `main()` inside a function,
   pressing Enter should maintain indent, not increase it.
2. `test_python_single_line_list_no_indent`: After `[1, 2, 3]` assignment,
   pressing Enter should maintain indent.
3. `test_python_multiline_call_indents`: Multi-line `foo(\narg\n)` should still
   indent correctly.
4. `test_python_multiline_list_indents`: Multi-line `[\n1,\n]` should still
   indent correctly.
5. `test_rust_single_line_call_no_indent`: Rust equivalent to ensure fix applies
   to all languages.

These tests should fail before the fix, demonstrating the regression.

Location: `crates/syntax/src/indent.rs`, in the `#[cfg(test)]` module.

### Step 2: Implement multiline check in compute_indent_delta

Modify the `@indent` capture handling in `compute_indent_delta()`:

**Current logic** (lines 221-225):
```rust
if Some(capture.index) == self.captures.indent {
    if capture_start_row == ref_line && !indent_added {
        delta += 1;
        indent_added = true;
    }
}
```

**New logic**:
```rust
if Some(capture.index) == self.captures.indent {
    if capture_start_row == ref_line && !indent_added {
        // Only count @indent if the node spans multiple lines.
        // Single-line bracket expressions (argument_list, list, etc.)
        // should not trigger indent.
        let is_multiline = capture.node.start_position().row
                         != capture.node.end_position().row;
        if is_multiline {
            delta += 1;
            indent_added = true;
        }
    }
}
```

This change affects only `@indent` captures. `@indent.always` is intentionally
left unchanged (it stacks and may have different semantics).

Location: `crates/syntax/src/indent.rs`, `compute_indent_delta()` method.

### Step 3: Verify existing tests still pass

Run the full test suite for the `syntax` crate:

```bash
cargo test -p syntax
```

All existing tests must pass. The change should not regress:
- `test_rust_indent_after_open_brace` (multiline by definition)
- `test_python_indent_after_colon` (multiline by definition)
- `test_python_indent_in_class` (multiline by definition)
- `test_rust_maintain_indent` (no new indent expected)

### Step 4: Verify new tests pass

Confirm the tests from Step 1 now pass:

```bash
cargo test -p syntax -- single_line
cargo test -p syntax -- multiline
```

### Step 5: Add backreference comment

Add a chunk backreference to the modified section:

```rust
// Chunk: docs/chunks/indent_multiline - Multiline check for bracket @indent
```

Place this comment above the `compute_indent_delta` method to document the
semantic fix.

## Dependencies

- **treesitter_indent**: This chunk depends on the base indent implementation
  from `treesitter_indent`. That chunk is already ACTIVE (merged).

## Risks and Open Questions

1. **False negatives for intentionally single-line indent captures**: If a
   language query intentionally marks a single-line construct for indent (e.g.,
   a one-line lambda that should indent its body), this fix would suppress it.
   Mitigation: Review query files after implementation; no such cases exist in
   Python or Rust queries today.

2. **Performance impact**: Adding `node.end_position()` lookup adds negligible
   overhead (it's already cached in the tree-sitter node). No measurable impact
   expected.

3. **`@indent.always` behavior**: We intentionally leave `@indent.always`
   unchanged. If a query author explicitly uses `@indent.always` on a single-line
   node, they get the indent. This preserves the "always" semantics.
