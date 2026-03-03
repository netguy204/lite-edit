# Implementation Plan

## Approach

This chunk implements intelligent auto-indentation using Helix-style indent queries (`indents.scm`). The implementation ports Helix's proven approach (`helix-core/src/indent.rs`) which uses a **hybrid heuristic**: rather than computing absolute indent levels, it computes the indent change (delta) relative to a reference line's actual indentation. This makes the system resilient to incomplete expressions and ERROR nodes that are common during mid-expression typing.

The strategy is:

1. **Extend `LanguageConfig` with indent queries**: Add an `indents_query` field alongside the existing `highlights_query`, `injections_query`, and `locals_query`. Port indent query files from Helix's `runtime/queries/{lang}/indents.scm` for Rust, Python, and progressively for other supported languages.

2. **Implement an `IndentComputer` module in `crates/syntax`**: This module:
   - Walks ancestors from the cursor position
   - Collects `@indent`/`@outdent`/`@extend` captures with scope awareness (`tail` vs `all`)
   - Applies the hybrid heuristic: compute indent delta vs a reference line
   - Returns the computed indent string (spaces or tabs based on editor config)

3. **Integrate into Enter-key handling**: When the user presses Enter, after inserting the newline:
   - Query the syntax highlighter for the computed indent at the new cursor position
   - Insert the indent string at the start of the new line
   - Position cursor after the indent

This approach follows the architecture patterns in `TESTING_PHILOSOPHY.md` (testable pure logic, humble view). The indent computation is a pure function: `(tree, source, cursor_position, tab_config) → indent_string`. It can be tested without any platform dependencies.

**Reference**: Helix's `helix-core/src/indent.rs` (MIT licensed) and [indent query guide](https://docs.helix-editor.com/guides/indent.html).

## Subsystem Considerations

No existing subsystems are directly relevant to this chunk. The work touches the syntax crate but does not interact with the existing `renderer`, `spatial_layout`, or `viewport_scroll` subsystems documented in `docs/subsystems/`.

## Sequence

### Step 1: Add indent query files to `crates/syntax/queries/`

Create a `queries/` directory in the syntax crate to hold indent query files:

```
crates/syntax/queries/
  rust/indents.scm
  python/indents.scm
```

Port the indent queries from Helix's `runtime/queries/{lang}/indents.scm`:

- **Rust**: `@indent` for `{`, `[`, `(`, block expressions; `@outdent` for `}`, `]`, `)`; `@extend` for continued expressions
- **Python**: Critical use of `@extend` for whitespace-sensitive blocks; `@indent` for `:` ending lines

For initial implementation, start with Rust and Python. Other languages can use empty queries (no indent changes) or be added incrementally.

**Location**: `crates/syntax/queries/rust/indents.scm`, `crates/syntax/queries/python/indents.scm`

**Build integration**: Update `Cargo.toml` to include these files (via `include` or embed via `include_str!`).

### Step 2: Add `indents_query` field to `LanguageConfig`

Extend `LanguageConfig` in `crates/syntax/src/registry.rs`:

```rust
pub struct LanguageConfig {
    pub language: Language,
    pub highlights_query: &'static str,
    pub injections_query: &'static str,
    pub locals_query: &'static str,
    pub language_name: &'static str,
    // Chunk: docs/chunks/treesitter_indent - Indent query for intelligent indentation
    /// The indents query (for computing line indentation from parse tree structure)
    pub indents_query: &'static str,
}
```

Update the `new()` constructor and all call sites. Initially, most languages will have an empty `indents_query` (`""`).

**Location**: `crates/syntax/src/registry.rs`

**Tests**: Existing tests should continue to pass; add tests verifying `indents_query` is accessible.

### Step 3: Populate indent queries for Rust and Python in registry

Embed the indent query content in the `LanguageRegistry::new()` initializer:

```rust
// For Rust
let rust_config = LanguageConfig::new(
    tree_sitter_rust::LANGUAGE.into(),
    tree_sitter_rust::HIGHLIGHTS_QUERY,
    tree_sitter_rust::INJECTIONS_QUERY,
    "",  // locals
    "rust",
    include_str!("../queries/rust/indents.scm"),  // indents
);

// For Python
let python_config = LanguageConfig::new(
    tree_sitter_python::LANGUAGE.into(),
    tree_sitter_python::HIGHLIGHTS_QUERY,
    "",  // injections
    "",  // locals
    "python",
    include_str!("../queries/python/indents.scm"),
);
```

**Location**: `crates/syntax/src/registry.rs`

### Step 4: Create `IndentConfig` type

Define a configuration type for indentation preferences:

```rust
// Chunk: docs/chunks/treesitter_indent - Indent configuration
/// Configuration for indentation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndentConfig {
    /// Number of spaces per indent level (used when `use_tabs` is false)
    pub indent_width: usize,
    /// Whether to use tabs for indentation
    pub use_tabs: bool,
    /// Width of a tab character in spaces (for computing visual column)
    pub tab_width: usize,
}

impl Default for IndentConfig {
    fn default() -> Self {
        Self {
            indent_width: 4,
            use_tabs: false,
            tab_width: 4,
        }
    }
}

impl IndentConfig {
    /// Returns the string to insert for one level of indentation.
    pub fn indent_unit(&self) -> String {
        if self.use_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.indent_width)
        }
    }
}
```

**Location**: `crates/syntax/src/indent.rs` (new file)

### Step 5: Implement `IndentComputer` struct

Create the core indent computation struct:

```rust
// Chunk: docs/chunks/treesitter_indent - Tree-sitter based indent computation
/// Computes intelligent indentation using tree-sitter indent queries.
pub struct IndentComputer {
    /// Compiled indent query
    query: Query,
    /// Capture indices for special captures
    capture_indices: CaptureIndices,
}

struct CaptureIndices {
    indent: Option<u32>,
    indent_always: Option<u32>,
    outdent: Option<u32>,
    outdent_always: Option<u32>,
    extend: Option<u32>,
    extend_prevent_once: Option<u32>,
}
```

The struct pre-compiles the indent query and caches capture indices for fast lookup during computation.

**Location**: `crates/syntax/src/indent.rs`

### Step 6: Implement capture collection algorithm

Implement the core algorithm that walks ancestors and collects indent/outdent captures:

```rust
impl IndentComputer {
    /// Computes the indent delta at a position by walking ancestors.
    ///
    /// Returns the net indent change: positive = indent, negative = outdent.
    /// The hybrid heuristic applies this delta to a reference line's indentation.
    pub fn compute_indent_delta(
        &self,
        tree: &Tree,
        source: &str,
        line: usize,
        col: usize,
    ) -> i32 {
        let mut cursor = QueryCursor::new();
        let root = tree.root_node();

        // Find the deepest node at the cursor position
        let byte_offset = position_to_byte_offset(source, line, col);
        let Some(node) = root.descendant_for_byte_range(byte_offset, byte_offset) else {
            return 0;
        };

        // Walk ancestors, collecting indent/outdent captures
        let mut delta = 0i32;
        let mut current = Some(node);

        while let Some(n) = current {
            // Execute query limited to this node's range
            cursor.set_byte_range(n.byte_range());
            let matches = cursor.matches(&self.query, n, source.as_bytes());

            for m in matches {
                for capture in m.captures {
                    let scope = self.scope_for_capture(capture.index);
                    let applies = match scope {
                        Scope::Tail => capture.node.start_position().row < line as u32,
                        Scope::All => true,
                    };

                    if applies {
                        if self.is_indent_capture(capture.index) {
                            delta += 1;
                        } else if self.is_outdent_capture(capture.index) {
                            delta -= 1;
                        }
                    }
                }
            }

            current = n.parent();
        }

        delta
    }
}
```

Handle `@indent.always`/`@outdent.always` captures which stack (multiple on same line), vs regular `@indent`/`@outdent` which don't stack.

**Location**: `crates/syntax/src/indent.rs`

### Step 7: Implement reference line lookup for hybrid heuristic

The hybrid heuristic computes indent as: `reference_line_indent + delta`. Implement reference line lookup:

```rust
impl IndentComputer {
    /// Finds a suitable reference line for the hybrid heuristic.
    ///
    /// The reference line is typically the first non-blank line above the target
    /// that is at the same or lower indentation level.
    fn find_reference_line(&self, source: &str, target_line: usize) -> Option<usize> {
        // Walk backwards from target_line to find a suitable reference
        for line_num in (0..target_line).rev() {
            let line_content = self.line_content(source, line_num);
            if !line_content.trim().is_empty() {
                return Some(line_num);
            }
        }
        None
    }

    /// Gets the existing indentation of a line.
    fn line_indentation(&self, source: &str, line: usize) -> &str {
        let content = self.line_content(source, line);
        let non_ws = content.find(|c: char| !c.is_whitespace()).unwrap_or(content.len());
        &content[..non_ws]
    }
}
```

**Location**: `crates/syntax/src/indent.rs`

### Step 8: Implement public `compute_indent` API

Combine delta computation and hybrid heuristic into the public API:

```rust
impl IndentComputer {
    /// Computes the indentation string for a new line.
    ///
    /// This is the main entry point. It:
    /// 1. Finds a reference line (typically the previous non-blank line)
    /// 2. Computes the indent delta from tree-sitter queries
    /// 3. Applies the delta to the reference line's indentation
    /// 4. Returns the resulting indent string
    ///
    /// # Arguments
    ///
    /// * `tree` - The current parse tree
    /// * `source` - The source text (after the newline was inserted)
    /// * `line` - The line number to compute indent for (the new line)
    /// * `config` - Indentation configuration (tabs vs spaces, width)
    ///
    /// # Returns
    ///
    /// The indentation string to insert at the start of the new line.
    pub fn compute_indent(
        &self,
        tree: &Tree,
        source: &str,
        line: usize,
        config: &IndentConfig,
    ) -> String {
        // Find reference line
        let Some(ref_line) = self.find_reference_line(source, line) else {
            // No reference line (first line of file), no indent
            return String::new();
        };

        // Get reference line's indentation
        let ref_indent = self.line_indentation(source, ref_line);
        let ref_indent_level = self.indent_level(ref_indent, config);

        // Compute delta at new line position
        let delta = self.compute_indent_delta(tree, source, line, 0);

        // Apply delta to reference indent
        let new_level = (ref_indent_level as i32 + delta).max(0) as usize;

        // Generate indent string
        self.indent_string(new_level, config)
    }

    fn indent_level(&self, indent_str: &str, config: &IndentConfig) -> usize {
        // Count visual columns of whitespace, convert to indent levels
        let mut visual_col = 0;
        for c in indent_str.chars() {
            match c {
                ' ' => visual_col += 1,
                '\t' => visual_col = (visual_col / config.tab_width + 1) * config.tab_width,
                _ => break,
            }
        }
        visual_col / config.indent_width
    }

    fn indent_string(&self, level: usize, config: &IndentConfig) -> String {
        if config.use_tabs {
            "\t".repeat(level)
        } else {
            " ".repeat(level * config.indent_width)
        }
    }
}
```

**Location**: `crates/syntax/src/indent.rs`

### Step 9: Add `@extend` support for Python-style blocks

Implement the `@extend` capture which expands a node's range to include subsequent indented lines:

```rust
impl IndentComputer {
    /// Checks if a node's range should be extended based on @extend captures.
    ///
    /// For Python and other whitespace-sensitive languages, @extend marks nodes
    /// whose scope should include subsequent lines that are more indented.
    fn should_extend(&self, node: Node, source: &str, target_line: usize) -> bool {
        // Check if node has @extend capture
        // If so, the node's scope extends to lines indented more than its start
        // ...
    }
}
```

**Location**: `crates/syntax/src/indent.rs`

### Step 10: Integrate `IndentComputer` into `SyntaxHighlighter`

Extend `SyntaxHighlighter` to optionally hold an `IndentComputer`:

```rust
pub struct SyntaxHighlighter {
    // ... existing fields ...

    // Chunk: docs/chunks/treesitter_indent - Indent computation support
    /// Optional indent computer for tree-sitter based indentation.
    /// None if the language has no indent query.
    indent_computer: Option<IndentComputer>,
}

impl SyntaxHighlighter {
    /// Computes indentation for a line.
    ///
    /// Returns `None` if no indent query is configured for this language.
    pub fn compute_indent(
        &self,
        line: usize,
        config: &IndentConfig,
    ) -> Option<String> {
        let computer = self.indent_computer.as_ref()?;
        Some(computer.compute_indent(&self.tree, &self.source, line, config))
    }
}
```

**Location**: `crates/syntax/src/highlighter.rs`

### Step 11: Expose indent computation through `Tab`

Add a method to `Tab` in workspace.rs to compute indent:

```rust
impl Tab {
    // Chunk: docs/chunks/treesitter_indent - Expose indent computation to editor
    /// Computes the indentation for a new line.
    ///
    /// Returns the indent string to insert, or empty string if:
    /// - No highlighter is configured
    /// - Language has no indent query
    /// - Computation fails
    pub fn compute_indent_for_line(
        &self,
        line: usize,
        config: &IndentConfig,
    ) -> String {
        self.highlighter
            .as_ref()
            .and_then(|hl| hl.compute_indent(line, config))
            .unwrap_or_default()
    }
}
```

**Location**: `crates/editor/src/workspace.rs`

### Step 12: Wire Enter key handling to use computed indent

Modify the `InsertNewline` command handling in `buffer_target.rs`:

```rust
Command::InsertNewline => {
    // Insert the newline
    let result = ctx.buffer.insert_newline_tracked();
    ctx.edit_info = result.edit_info;

    // Chunk: docs/chunks/treesitter_indent - Apply intelligent indentation
    // Compute and insert indent for the new line
    if let Some(indent) = ctx.compute_indent_for_new_line() {
        if !indent.is_empty() {
            // Insert indent at start of new line
            let indent_result = ctx.buffer.insert_str_tracked(&indent);
            // Merge edit info for both operations
            if let Some(indent_edit) = indent_result.edit_info {
                // The tree was updated by insert_newline; the indent insertion
                // needs a fresh edit event for the additional text
                ctx.pending_indent_edit = Some(indent_edit);
            }
        }
    }

    result.dirty_lines
}
```

This requires `EditorContext` to have access to the computed indent. The simplest approach is to pass the indent string via a new field on `EditorContext` that's set by the `EditorState` before dispatching to `BufferFocusTarget`.

**Alternative approach**: Have `EditorState::handle_key_buffer` check after the key is handled whether the command was `InsertNewline`, and if so, compute and insert the indent there. This keeps the indent logic at a higher level where we have access to the workspace and tab.

**Preferred approach**: After `handle_key` returns for Enter key, in `editor_state.rs`:
1. Check if the buffer cursor moved to a new line (implies newline was inserted)
2. Compute indent via `tab.compute_indent_for_line(cursor_line, &config)`
3. Insert the computed indent string

**Location**: `crates/editor/src/buffer_target.rs` and/or `crates/editor/src/editor_state.rs`

### Step 13: Handle multiline strings and comments

Ensure typing inside multiline strings or comments doesn't produce incorrect indentation:

- Option A: Add `@indent.ignore` captures for string and comment nodes in indent queries
- Option B: In `IndentComputer::compute_indent_delta`, check if cursor is inside a string/comment node and return 0

The Helix approach uses Option A — the indent queries explicitly mark string/comment interiors. We'll follow the same pattern.

**Location**: `crates/syntax/queries/*/indents.scm`, `crates/syntax/src/indent.rs`

### Step 14: Add unit tests for indent computation

Write unit tests following TESTING_PHILOSOPHY.md (pure logic, no platform dependencies):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_indent_after_open_brace() {
        let source = "fn main() {\n";
        let tree = parse_rust(source);
        let computer = IndentComputer::new_for_rust();
        let config = IndentConfig::default();

        // Line 1 (after the newline) should be indented
        let indent = computer.compute_indent(&tree, source, 1, &config);
        assert_eq!(indent, "    ");
    }

    #[test]
    fn test_rust_outdent_on_close_brace() {
        let source = "fn main() {\n    x\n}";
        let tree = parse_rust(source);
        let computer = IndentComputer::new_for_rust();
        let config = IndentConfig::default();

        // Line 2 (the closing brace) should be at base level
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(indent, "");
    }

    #[test]
    fn test_python_indent_after_colon() {
        let source = "def foo():\n";
        let tree = parse_python(source);
        let computer = IndentComputer::new_for_python();
        let config = IndentConfig::default();

        let indent = computer.compute_indent(&tree, source, 1, &config);
        assert_eq!(indent, "    ");
    }

    #[test]
    fn test_no_indent_in_multiline_string() {
        // Inside a multiline string, no indent should be added
        // ...
    }
}
```

**Location**: `crates/syntax/src/indent.rs`

### Step 15: Add integration tests for Enter key behavior

Write integration tests that simulate typing and verify indent:

```rust
#[test]
fn test_enter_indents_after_open_brace() {
    let mut ctx = EditorContext::new_for_test();
    ctx.load_rust_file("fn main() {");
    ctx.move_to_end();

    // Press Enter
    simulate_key(&mut ctx, Key::Return);

    // Cursor should be on new line with indent
    assert_eq!(ctx.buffer.cursor_position(), Position::new(1, 4));
    assert_eq!(ctx.buffer.line_content(1), "    ");
}
```

**Location**: `crates/editor/tests/indent_test.rs` (new file)

### Step 16: Performance verification

Verify that indent computation stays within the 8ms latency budget:

```rust
#[test]
fn test_indent_latency() {
    // Load a large (5000 line) Rust file
    let source = generate_large_rust_file(5000);
    let tree = parse_rust(&source);
    let computer = IndentComputer::new_for_rust();

    let start = std::time::Instant::now();
    for line in 0..100 {
        computer.compute_indent(&tree, &source, line, &IndentConfig::default());
    }
    let elapsed = start.elapsed();

    // 100 computations should be well under 8ms
    assert!(elapsed.as_millis() < 8, "Indent computation too slow: {:?}", elapsed);
}
```

**Location**: `crates/syntax/tests/performance.rs` or `crates/syntax/benches/`

## Dependencies

- **Depends on `incremental_parse` chunk**: This chunk (status: ACTIVE) wired up incremental tree-sitter parsing. The indent computation relies on having an up-to-date parse tree available immediately after newline insertion. With the full-reparse path, the tree would be stale during indent computation.

- **No external library additions needed**: Tree-sitter and the query API are already available via the existing `tree-sitter` dependency.

- **Helix query files**: Port indent queries from Helix's `runtime/queries/{lang}/indents.scm` (MIT licensed). Start with Rust and Python.

## Risks and Open Questions

1. **Query file porting effort**: Helix has indent queries for ~50 languages, but lite-edit only supports 13. Need to verify that the queries work with our grammar versions (may have minor differences). Start with Rust and Python to validate the approach before porting others.

2. **ERROR node handling**: Incomplete expressions produce ERROR nodes that don't match indent rules. The hybrid heuristic mitigates this by computing deltas rather than absolute levels. May need explicit `(ERROR ...)` patterns in some queries (Helix does this).

3. **Tab vs spaces consistency**: The `IndentConfig` needs to be plumbed from somewhere. Currently there's no editor-wide configuration system. For initial implementation, default to 4 spaces. Configuration can be added later.

4. **Tree freshness after newline**: After inserting a newline, the tree needs to be incrementally updated before computing indent. Verify that the incremental parse path (from `incremental_parse` chunk) provides the updated tree synchronously.

5. **Multi-edit transactions**: When inserting newline + indent, we generate two `EditEvent`s. Need to verify tree-sitter handles these correctly when applied in sequence, and that the highlight cache is properly invalidated.

6. **Python `@extend` complexity**: Python's indent queries rely heavily on `@extend` to mark block scope. This is more complex than simple brace-matching languages. Need thorough testing of Python indentation.

## Deviations

<!-- Populated during implementation -->
