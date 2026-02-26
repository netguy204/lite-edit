<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Apply the established C++ highlight query layering pattern to TypeScript and TSX.

The codebase already solves this exact problem for C++: at `registry.rs` lines 69-87, the C++ configuration combines `tree_sitter_c::HIGHLIGHT_QUERY` with `tree_sitter_cpp::HIGHLIGHT_QUERY` because the C++ grammar's query only covers C++-specific constructs while fundamental constructs (types, keywords, functions) are defined in C.

TypeScript has the same relationship with JavaScript. The TypeScript grammar's `HIGHLIGHTS_QUERY` covers only TypeScript-specific constructs (type annotations, interfaces, generics, enums), while JavaScript's `HIGHLIGHT_QUERY` covers fundamentals (`const`, `let`, `function`, string literals, operators, etc.).

**Implementation:**
1. Replace the single `tree_sitter_typescript::HIGHLIGHTS_QUERY` with a combined query: `tree_sitter_javascript::HIGHLIGHT_QUERY + "\n" + tree_sitter_typescript::HIGHLIGHTS_QUERY`
2. Apply this to both `.ts` (TypeScript) and `.tsx` (TSX) configurations
3. Use the same `Box::leak(format!(...).into_boxed_str())` pattern used for C++, which produces a `&'static str` from the combined query

**Testing approach (per TESTING_PHILOSOPHY.md):**
- Write a TDD-style test that parses a TypeScript snippet and asserts highlights for JavaScript-level constructs (keywords like `const`, string literals)
- The test should demonstrate that these constructs receive styled spans, which they currently do not

## Subsystem Considerations

This chunk references `subsystem_id: syntax_highlighting` with relationship `implements` in its frontmatter. However, `docs/subsystems/syntax_highlighting/OVERVIEW.md` does not exist yet. The syntax highlighting functionality is documented via chunk backreferences in `crates/syntax/src/registry.rs` and `crates/syntax/src/highlighter.rs`.

Since no formal subsystem documentation exists, this chunk follows the established patterns visible in the existing code (C++ query layering) rather than documented subsystem invariants.

## Sequence

### Step 1: Write failing test for TypeScript JavaScript-level highlighting

Create a test in `crates/syntax/src/registry.rs` (in the `#[cfg(test)]` module) that:

1. Creates a `SyntaxHighlighter` for a TypeScript snippet containing JavaScript constructs: `const message: string = "hello";`
2. Highlights the line and checks that `const` receives a non-default style (keyword highlighting)
3. Checks that the string literal `"hello"` receives a non-default style

This test will **fail** initially because the current TypeScript config only uses `tree_sitter_typescript::HIGHLIGHTS_QUERY`, which doesn't define captures for JavaScript keywords or string literals.

Location: `crates/syntax/src/registry.rs` tests module

### Step 2: Update TypeScript config to use combined highlight query

Modify the TypeScript configuration (around line 107-114 in `registry.rs`) to use a combined query:

```rust
// TypeScript needs the JavaScript highlight query as a base, with TypeScript-specific
// additions layered on top. Same pattern as C/C++.
let ts_combined_query: &'static str = Box::leak(
    format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_typescript::HIGHLIGHTS_QUERY)
        .into_boxed_str(),
);
let typescript_config = LanguageConfig::new(
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    ts_combined_query,
    "",
    tree_sitter_typescript::LOCALS_QUERY,
);
configs.insert("ts", typescript_config);
```

### Step 3: Update TSX config to use combined highlight query

Apply the same fix to the TSX configuration (around line 117-123), sharing the combined query:

```rust
// TSX also needs the JavaScript base (it extends TypeScript which extends JavaScript)
let tsx_config = LanguageConfig::new(
    tree_sitter_typescript::LANGUAGE_TSX.into(),
    ts_combined_query,  // Reuse the combined query
    "",
    tree_sitter_typescript::LOCALS_QUERY,
);
configs.insert("tsx", tsx_config);
```

### Step 4: Run the test to confirm it passes

Execute `cargo test -p lite-edit-syntax` to verify:
1. The new TypeScript highlighting test passes
2. Existing tests continue to pass

### Step 5: Add backreference comment

Add a comment above the TypeScript config section documenting this chunk's contribution, following the existing pattern for C++:

```rust
// Chunk: docs/chunks/typescript_highlight_layering - Combined JS/TS highlight queries
```

## Dependencies

None. All required dependencies are already present:
- `tree_sitter_javascript` crate (already used for `.js`/`.jsx`/`.mjs` configs)
- `tree_sitter_typescript` crate (already used for `.ts`/`.tsx` configs)

## Risks and Open Questions

**Low risk:** This is a direct application of an existing pattern (C++ query layering) to an analogous situation (TypeScript/JavaScript). The pattern is proven in production.

**Potential query conflict:** JavaScript and TypeScript may define captures for the same AST node types. Tree-sitter's query system handles this by returning all matching captures. The highlighter already handles overlapping captures (see `build_line_from_captures` which skips already-covered bytes). The TypeScript-specific captures will take precedence for TypeScript-specific constructs because they appear later in the combined query and will match more specific node types.

**Memory overhead:** The `Box::leak` pattern creates a static string allocation. This is acceptable because:
1. It only runs once at `LanguageRegistry::new()` time
2. The combined query is a few KB, same as the C++ case
3. The registry lives for the lifetime of the process

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->