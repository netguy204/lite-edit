<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Implement same-file go-to-definition using tree-sitter `locals.scm` queries. The approach follows the nvim-treesitter pattern: run a locals query to collect scope, definition, and reference captures; identify the reference node at the cursor position; walk enclosing scopes upward matching by text equality; and jump to the first matching definition.

**Architecture**: The go-to-def logic will be implemented in a new `gotodef` module in `crates/syntax`, keeping tree-sitter query execution close to the existing highlighter infrastructure. The `SyntaxHighlighter` will be extended with a public method to access the parse tree and source (required for running locals queries). The editor layer will wire keyboard shortcuts and mouse events to invoke the resolution.

**Key patterns from DEC decisions and TESTING_PHILOSOPHY.md**:
- Follow the Model-View-Update pattern: cursor position and jump stack are model state, resolution is update logic, rendering is unchanged
- Test behavior at boundaries: empty files, cursor on non-identifier, definition on same line as reference, shadowed variables
- Incremental parsing is now active (via `incremental_parse` chunk), so query execution can rely on up-to-date trees

**Performance budget**: Query execution and scope walking must complete within the 8ms P99 latency budget. Based on investigation findings, locals queries on typical files (~500 captures) execute in ~200µs, well within budget.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll**: Not directly relevant - go-to-def jumps the cursor, viewport follows via existing cursor-follows-viewport logic
- **docs/subsystems/renderer**: Not relevant - no rendering changes needed, just cursor position updates
- **docs/subsystems/spatial_layout**: Not relevant - cursor positioning uses existing buffer position APIs

No subsystems are directly relevant to this chunk.

## Sequence

### Step 1: Write `locals.scm` query files for Rust, Python, JavaScript, TypeScript

Create `locals.scm` query content for the four required languages. These define `@local.scope`, `@local.definition`, and `@local.reference` captures that enable scope-walking resolution.

**Sources**:
- JavaScript and TypeScript: Use `tree_sitter_javascript::LOCALS_QUERY` and `tree_sitter_typescript::LOCALS_QUERY` which are already populated in `LanguageConfig`
- Rust and Python: Port from nvim-treesitter's `queries/{lang}/locals.scm` files (MIT-licensed)

**Format**: Store as `&'static str` constants in a new `queries` module in `crates/syntax/src/queries/`. Use the same `Box::leak` pattern as existing combined queries in `registry.rs`.

**Files created**: `crates/syntax/src/queries/mod.rs`, `crates/syntax/src/queries/rust.rs`, `crates/syntax/src/queries/python.rs`

**Test**: Verify query compiles for each language (no syntax errors).

### Step 2: Populate `locals_query` in `LanguageRegistry` for all four languages

Update `LanguageRegistry::new()` in `registry.rs` to use the new locals query constants for Rust and Python. Remove the `#[allow(dead_code)]` annotation from the `locals_query` field.

**Location**: `crates/syntax/src/registry.rs`

**Changes**:
- Import query constants from the new `queries` module
- Set `locals_query` to the appropriate constant for Rust, Python, JavaScript, TypeScript
- Remove `#[allow(dead_code)]` from `locals_query` field (line 25)

**Test**: `registry.config_for_extension("rs").unwrap().locals_query.is_empty()` returns `false`

### Step 3: Implement `LocalsResolver` struct in new `gotodef` module

Create `crates/syntax/src/gotodef.rs` with a `LocalsResolver` that:
1. Compiles the locals query once (cached on struct)
2. Provides `resolve(&self, source: &str, tree: &Tree, cursor_byte: usize) -> Option<Position>`

**Data structures**:
```rust
/// A captured scope/definition/reference from the locals query
struct LocalsCapture {
    kind: CaptureKind,  // Scope, Definition, or Reference
    node_range: (usize, usize),  // byte range
    name: String,  // text content for definitions/references
}

enum CaptureKind {
    Scope,
    Definition,
    Reference,
}
```

**Resolution algorithm**:
1. Run `QueryCursor` over the tree to collect all captures
2. Find the reference capture that contains `cursor_byte`
3. If not found, return `None` (cursor not on a reference)
4. Extract the reference's text (the identifier name)
5. Build list of scopes that contain the cursor position
6. For each scope (innermost first), search for a definition with matching text
7. Return the definition's position if found

**Location**: `crates/syntax/src/gotodef.rs`

**Backreference**:
```rust
// Chunk: docs/chunks/treesitter_gotodef - Same-file go-to-definition resolution
```

**Test**: Unit test with inline Rust source containing a local variable definition and reference; verify resolution returns the definition position.

### Step 4: Add public tree and source accessors to `SyntaxHighlighter`

Add methods to `SyntaxHighlighter` to expose the parse tree and source, enabling external code to run additional queries:

```rust
/// Returns a reference to the current parse tree.
pub fn tree(&self) -> &Tree {
    &self.tree
}

/// Returns a reference to the current source text.
pub fn source(&self) -> &str {
    &self.source
}
```

The `source()` method already exists (line 1276). Only `tree()` needs to be added.

**Location**: `crates/syntax/src/highlighter.rs`

**Test**: Compile; method is trivial and covered by integration tests.

### Step 5: Add `GotoDefResolver` struct that wraps `LocalsResolver` with language config

Create a higher-level struct that handles language config lookup and query compilation:

```rust
/// Go-to-definition resolver for a specific language.
///
/// Compiled once per language, reused for all files of that language.
pub struct GotoDefResolver {
    locals_query: Query,
}

impl GotoDefResolver {
    /// Creates a new resolver for the given language config.
    /// Returns None if the language has no locals query.
    pub fn new(config: &LanguageConfig) -> Option<Self>;

    /// Resolves the definition for the symbol at the given byte position.
    /// Returns the byte offset of the definition, or None if not found.
    pub fn resolve(&self, source: &str, tree: &Tree, cursor_byte: usize) -> Option<usize>;
}
```

**Location**: `crates/syntax/src/gotodef.rs`

**Export**: Add to `crates/syntax/src/lib.rs`: `pub use gotodef::GotoDefResolver;`

**Test**: Integration test: create highlighter, get tree, call resolver, verify result.

### Step 6: Add jump stack to `Workspace` for back-navigation

Add a simple jump stack to track cursor positions before go-to-def jumps:

```rust
/// Position in a buffer for jump stack
#[derive(Clone, Debug)]
pub struct JumpPosition {
    pub tab_id: TabId,
    pub line: usize,
    pub col: usize,
}

/// Stack of jump positions for back-navigation
pub struct JumpStack {
    positions: Vec<JumpPosition>,
    max_size: usize,  // Limit to prevent unbounded growth, e.g., 100
}
```

**Methods**:
- `push(pos: JumpPosition)`: Push a position onto the stack
- `pop() -> Option<JumpPosition>`: Pop and return the most recent position

Add `jump_stack: JumpStack` field to `Workspace`.

**Location**: `crates/editor/src/workspace.rs`

**Backreference**:
```rust
// Chunk: docs/chunks/treesitter_gotodef - Jump stack for go-to-definition back navigation
```

**Test**: Unit test: push positions, pop returns in LIFO order, respects max size.

### Step 7: Add `GotoDefinition` command to `BufferFocusTarget`

Add a new command variant to `buffer_target.rs`:

```rust
/// Go to the definition of the symbol under the cursor
GotoDefinition,
/// Go back to the previous cursor position (from jump stack)
GoBack,
```

Add key bindings in `resolve_command()`:
- `Key::Char('d')` with Command modifier → `GotoDefinition` (Cmd+D)
- Or F12 → `GotoDefinition` (common IDE binding)
- `Key::Char('[')` with Command modifier → `GoBack` (Cmd+[)

**Location**: `crates/editor/src/buffer_target.rs`

**Test**: Key binding resolution test.

### Step 8: Implement `execute_command` for `GotoDefinition` and `GoBack`

In `BufferFocusTarget::execute_command()`, handle the new commands:

**GotoDefinition**:
1. Get current cursor position (line, col)
2. Convert to byte offset using `TextBuffer::byte_offset_at()`
3. Get the `SyntaxHighlighter` from the active tab
4. Get the `LanguageConfig` and create/reuse a `GotoDefResolver`
5. Call `resolver.resolve(source, tree, cursor_byte)`
6. If found:
   - Push current position to jump stack
   - Convert definition byte offset to (line, col)
   - Move cursor to definition position
   - Mark dirty region for redraw
7. If not found:
   - Display status message "Definition not found in this file"

**GoBack**:
1. Pop from jump stack
2. If position found:
   - Check if tab still exists
   - Switch to that tab if different
   - Move cursor to saved position
3. If stack empty: do nothing

**Location**: `crates/editor/src/buffer_target.rs` for command execution, `crates/editor/src/editor_state.rs` for workspace access

**Note**: The resolver may need to be cached on the highlighter or workspace to avoid recompilation. Consider adding `GotoDefResolver` as a lazily-initialized field alongside the `SyntaxHighlighter`.

**Backreference**:
```rust
// Chunk: docs/chunks/treesitter_gotodef - GotoDefinition command execution
```

### Step 9: Add Cmd-click handling for go-to-definition

Extend mouse event handling in `editor_state.rs` to detect Cmd-click:

```rust
// In handle_mouse() or handle_mouse_buffer()
if event.kind == MouseEventKind::Down && event.modifiers.command {
    // Convert click position to buffer position
    // Trigger go-to-definition at that position
}
```

**Integration**: The click position is converted to a buffer position, then `GotoDefinition` logic is invoked with that position rather than the current cursor position.

**Location**: `crates/editor/src/editor_state.rs` and/or `crates/editor/src/buffer_target.rs`

### Step 10: Add status message display for "definition not found"

When go-to-definition fails (no same-file definition found), display a brief status message to the user. Use the existing `MiniBuffer` or add a simple status display.

**Location**: Check existing status/message patterns in `crates/editor/src/mini_buffer.rs`

**Message**: "Definition not found in this file" (or similar concise text)

**Duration**: Message should auto-clear after ~2 seconds or on next keypress

### Step 11: Write comprehensive tests

**Unit tests** (in `crates/syntax/src/gotodef.rs`):
- Resolution of local variable definition
- Resolution of function parameter
- Resolution of locally-defined function
- Shadowed variable (innermost definition wins)
- Cursor on definition (not reference) → returns None or same position
- Cursor on non-identifier → returns None
- Empty file → returns None

**Integration tests** (in `crates/editor/tests/`):
- Cmd+D on identifier moves cursor to definition
- Cmd+[ after Cmd+D returns to original position
- Cmd-click on identifier moves cursor to definition
- Jump stack size is bounded

## Dependencies

- **incremental_parse chunk** (ACTIVE): This chunk depends on incremental parsing being wired up, which it now is. The `Tab::notify_edit()` path is active, ensuring parse trees are up-to-date.
- **tree_sitter_javascript, tree_sitter_typescript, tree_sitter_rust, tree_sitter_python crates**: Already dependencies in `Cargo.toml`.

## Risks and Open Questions

1. **Locals query accuracy for Rust**: nvim-treesitter has documented bugs for hoisted function names (issue #499). May need to test carefully and document limitations.

2. **Query caching strategy**: Should `GotoDefResolver` be created once per language and shared, or created per-file? Per-language is more efficient but requires a cache somewhere (perhaps on `LanguageRegistry` or a new struct).

3. **Cmd+D conflict**: Cmd+D is used in some editors for "add selection to next find match". Verify no existing binding conflict. F12 is a safer alternative if conflicts arise.

4. **Status message infrastructure**: May need to add a simple status display if `MiniBuffer` doesn't support transient messages. Keep implementation minimal.

5. **Cross-file symbols**: When resolution fails for an imported symbol, the "not found" message should be clear that this is a same-file limitation, not a bug.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?
-->
