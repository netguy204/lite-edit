// Chunk: docs/chunks/treesitter_indent - Tree-sitter based indent computation

//! Intelligent auto-indentation using tree-sitter indent queries.
//!
//! This module implements the Helix-style hybrid heuristic for computing
//! indentation. Rather than computing absolute indent levels (which fails
//! for incomplete expressions), it computes the indent change (delta)
//! relative to a reference line's actual indentation.
//!
//! ## Query Captures
//!
//! The indent queries use Helix-style captures:
//! - `@indent`: Increment indent level for new lines within this node
//! - `@outdent`: Decrement indent level when this node is encountered
//! - `@indent.always`: Always increment (stacks with multiple captures)
//! - `@outdent.always`: Always decrement (stacks with multiple captures)
//! - `@extend`: Extend the scope of the parent node
//! - `@indent.ignore`: Don't compute indent inside this node (strings, comments)
//!
//! ## Algorithm
//!
//! 1. Find a reference line (typically the previous non-blank line)
//! 2. Walk ancestors from the cursor position, collecting captures
//! 3. Compute the net indent delta from captures
//! 4. Apply delta to reference line's indentation
//! 5. Return the computed indent string

use crate::edit::position_to_byte_offset;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Query, QueryCursor, Tree};

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

/// Cached capture indices for fast lookup during computation.
#[derive(Debug, Default)]
struct CaptureIndices {
    /// Index of @indent capture
    indent: Option<u32>,
    /// Index of @indent.always capture
    indent_always: Option<u32>,
    /// Index of @outdent capture
    outdent: Option<u32>,
    /// Index of @outdent.always capture
    outdent_always: Option<u32>,
    /// Index of @extend capture
    extend: Option<u32>,
    /// Index of @extend.prevent-once capture
    #[allow(dead_code)] // Reserved for future use
    extend_prevent_once: Option<u32>,
    /// Index of @indent.ignore capture (for strings/comments)
    indent_ignore: Option<u32>,
}

impl CaptureIndices {
    /// Builds capture indices from a query's capture names.
    fn from_query(query: &Query) -> Self {
        let mut indices = Self::default();
        for (i, name) in query.capture_names().iter().enumerate() {
            let idx = i as u32;
            match *name {
                "indent" => indices.indent = Some(idx),
                "indent.always" => indices.indent_always = Some(idx),
                "outdent" => indices.outdent = Some(idx),
                "outdent.always" => indices.outdent_always = Some(idx),
                "extend" => indices.extend = Some(idx),
                "extend.prevent-once" => indices.extend_prevent_once = Some(idx),
                "indent.ignore" => indices.indent_ignore = Some(idx),
                _ => {} // Ignore unknown captures
            }
        }
        indices
    }
}

/// Computes intelligent indentation using tree-sitter indent queries.
///
/// The `IndentComputer` pre-compiles the indent query and caches capture
/// indices for fast lookup during computation.
///
/// # Performance
///
/// Indent computation is designed to complete within the 8ms keystroke-to-glyph
/// budget. Typical computation times:
/// - Query execution: ~50-100µs
/// - Ancestor walk: ~10-20µs
/// - Total: ~100µs per indent computation
pub struct IndentComputer {
    /// Compiled indent query
    query: Query,
    /// Cached capture indices
    captures: CaptureIndices,
}

impl IndentComputer {
    /// Creates a new indent computer from an indent query string.
    ///
    /// Returns `None` if the query fails to compile (e.g., syntax errors
    /// or node types not present in the grammar).
    pub fn new(language: &Language, indents_query: &str) -> Option<Self> {
        if indents_query.is_empty() {
            return None;
        }

        let query = Query::new(language, indents_query).ok()?;
        let captures = CaptureIndices::from_query(&query);

        Some(Self { query, captures })
    }

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
        // Check if cursor is inside an ignored region (string/comment)
        if self.is_in_ignored_region(tree, source, line) {
            // Inside a string or comment - preserve existing indentation
            return String::new();
        }

        // Find reference line (previous non-blank line)
        let Some(ref_line) = self.find_reference_line(source, line) else {
            // No reference line (first line of file), no indent
            return String::new();
        };

        // Get reference line's indentation
        let ref_indent = self.line_indentation(source, ref_line);
        let ref_indent_level = self.indent_level(ref_indent, config);

        // Compute delta at new line position
        let delta = self.compute_indent_delta(tree, source, ref_line, line);

        // Apply delta to reference indent
        let new_level = (ref_indent_level as i32 + delta).max(0) as usize;

        // Generate indent string
        self.indent_string(new_level, config)
    }

    // Chunk: docs/chunks/indent_multiline - Multiline check for bracket @indent
    /// Computes the indent delta at a position by analyzing tree structure.
    ///
    /// Returns the net indent change: positive = indent, negative = outdent.
    ///
    /// The algorithm:
    /// 1. Run the indent query on the full tree
    /// 2. For @indent captures that START on the reference line:
    ///    - Container types (argument_list, list, tuple, etc.) only trigger indent
    ///      if they span multiple lines
    ///    - Block types (function_definition, block, etc.) trigger indent if the
    ///      target line is inside the node, or if the node is zero-width (placeholder)
    /// 3. For @outdent captures at the START of target line, add -1
    /// 4. Fallback: If no captures triggered, check for line-ending delimiters
    fn compute_indent_delta(
        &self,
        tree: &Tree,
        source: &str,
        ref_line: usize,
        target_line: usize,
    ) -> i32 {
        let mut cursor = QueryCursor::new();
        let root = tree.root_node();

        // Track if we've already added indent/outdent for a line
        let mut indent_added = false;
        let mut outdent_added = false;
        let mut delta = 0i32;

        // Execute query on the entire tree
        let mut matches = cursor.matches(&self.query, root, source.as_bytes());

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let capture_start_row = capture.node.start_position().row;

                // @indent: Count if the node STARTS on the reference line (once only)
                if Some(capture.index) == self.captures.indent {
                    if capture_start_row == ref_line && !indent_added {
                        // Trigger indent if ANY of these conditions hold:
                        //
                        // 1. Target line is inside the node (we're entering/inside its scope)
                        // 2. The node is zero-width (a placeholder for incomplete syntax)
                        //    - Tree-sitter inserts zero-width nodes for missing blocks
                        //    - e.g., `def foo():\n` has a zero-width `block` at the colon
                        //
                        // For container/bracket types (argument_list, list, tuple, etc.),
                        // we additionally require that the node spans multiple lines.
                        // Single-line bracket expressions like `main()` shouldn't add indent.
                        let node_end = capture.node.end_position();
                        let node_start = capture.node.start_position();
                        let target_inside_node = target_line <= node_end.row;
                        let is_zero_width =
                            node_start.row == node_end.row && node_start.column == node_end.column;

                        if target_inside_node || is_zero_width {
                            let requires_multiline =
                                Self::is_container_node(capture.node.kind());
                            let is_multiline = capture_start_row != node_end.row;

                            if !requires_multiline || is_multiline {
                                delta += 1;
                                indent_added = true;
                            }
                        }
                    }
                } else if Some(capture.index) == self.captures.indent_always {
                    // @indent.always - stacks, no limit
                    if capture_start_row == ref_line {
                        delta += 1;
                    }
                } else if Some(capture.index) == self.captures.outdent {
                    // @outdent: Count if this closing delimiter is at the START of target line
                    if capture_start_row == target_line && !outdent_added {
                        // Only count if it's at the start of the line (whitespace before it)
                        let col = capture.node.start_position().column;
                        if col == 0 || self.is_at_line_start(source, target_line, col) {
                            delta -= 1;
                            outdent_added = true;
                        }
                    }
                } else if Some(capture.index) == self.captures.outdent_always {
                    // @outdent.always - stacks
                    if capture_start_row == target_line {
                        let col = capture.node.start_position().column;
                        if col == 0 || self.is_at_line_start(source, target_line, col) {
                            delta -= 1;
                        }
                    }
                }
            }
        }

        // Fallback: If no tree-sitter captures triggered indent, check if the
        // reference line ends with an opening delimiter that should indent.
        // This handles incomplete syntax where tree-sitter hasn't formed the
        // expected nodes (e.g., `fn main() {` without a closing `}`).
        if delta == 0 && !indent_added {
            if self.line_ends_with_indent_delimiter(source, ref_line) {
                delta += 1;
            }
        }

        delta
    }

    /// Checks if a line ends with an opening delimiter that should trigger indent.
    ///
    /// This is a text-based fallback for cases where tree-sitter's error recovery
    /// doesn't create the expected nodes (e.g., incomplete blocks).
    fn line_ends_with_indent_delimiter(&self, source: &str, line: usize) -> bool {
        let content = self.line_content(source, line);
        let trimmed = content.trim_end();
        if trimmed.is_empty() {
            return false;
        }
        let last_char = trimmed.chars().last().unwrap();
        matches!(last_char, '{' | '[' | '(' | ':')
    }

    /// Checks if a column position is at the start of a line (only whitespace before it).
    fn is_at_line_start(&self, source: &str, line: usize, col: usize) -> bool {
        let content = self.line_content(source, line);
        if col > content.len() {
            return false;
        }
        content[..col].chars().all(|c| c.is_whitespace())
    }

    /// Checks if the cursor position is inside an ignored region (string/comment).
    fn is_in_ignored_region(&self, tree: &Tree, source: &str, line: usize) -> bool {
        if self.captures.indent_ignore.is_none() {
            return false;
        }

        let mut cursor = QueryCursor::new();
        let root = tree.root_node();

        // Check if the start of the line is inside an ignored node
        let byte_offset = position_to_byte_offset(source, line, 0);

        // Find the node at this position
        let Some(node) = root.descendant_for_byte_range(byte_offset, byte_offset) else {
            return false;
        };

        // Walk up to check if any ancestor is an ignored node
        let mut current = Some(node);
        while let Some(n) = current {
            // Execute query limited to this node
            cursor.set_byte_range(n.byte_range());
            let mut matches = cursor.matches(&self.query, n, source.as_bytes());

            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if Some(capture.index) == self.captures.indent_ignore {
                        // Check if our position is inside this ignored node
                        if capture.node.byte_range().contains(&byte_offset) {
                            return true;
                        }
                    }
                }
            }

            current = n.parent();
        }

        false
    }

    /// Finds a suitable reference line for the hybrid heuristic.
    ///
    /// The reference line is typically the first non-blank line above the target.
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

    /// Gets the content of a specific line.
    fn line_content<'a>(&self, source: &'a str, line: usize) -> &'a str {
        let mut current_line = 0;
        let mut line_start = 0;

        for (i, ch) in source.char_indices() {
            if current_line == line {
                // Find the end of this line
                if let Some(newline_pos) = source[line_start..].find('\n') {
                    return &source[line_start..line_start + newline_pos];
                } else {
                    return &source[line_start..];
                }
            }
            if ch == '\n' {
                current_line += 1;
                line_start = i + 1;
            }
        }

        // If we reached here, either the line doesn't exist or it's the last line
        if current_line == line {
            &source[line_start..]
        } else {
            ""
        }
    }

    /// Gets the existing indentation of a line.
    fn line_indentation<'a>(&self, source: &'a str, line: usize) -> &'a str {
        let content = self.line_content(source, line);
        let non_ws = content
            .find(|c: char| !c.is_whitespace() || c == '\n')
            .unwrap_or(content.len());
        &content[..non_ws]
    }

    /// Computes the indent level from an indentation string.
    fn indent_level(&self, indent_str: &str, config: &IndentConfig) -> usize {
        let mut visual_col = 0;
        for c in indent_str.chars() {
            match c {
                ' ' => visual_col += 1,
                '\t' => {
                    // Tab advances to next tab stop
                    visual_col = (visual_col / config.tab_width + 1) * config.tab_width
                }
                _ => break,
            }
        }
        visual_col / config.indent_width
    }

    /// Generates an indentation string for a given level.
    fn indent_string(&self, level: usize, config: &IndentConfig) -> String {
        if config.use_tabs {
            "\t".repeat(level)
        } else {
            " ".repeat(level * config.indent_width)
        }
    }

    /// Checks if a node type is a container/bracket type that should only
    /// trigger indent when spanning multiple lines.
    ///
    /// Container types group elements (arguments, list items, etc.) and should
    /// only indent their contents when the container spans multiple lines.
    /// Single-line containers like `foo()` or `[1, 2]` should not trigger
    /// extra indentation.
    ///
    /// This is in contrast to block-introducing types (function_definition,
    /// if_statement, block) which always introduce a new scope and should
    /// always trigger indentation regardless of whether they're "complete".
    fn is_container_node(kind: &str) -> bool {
        matches!(
            kind,
            // Python containers
            "argument_list"
                | "parameters"
                | "list"
                | "dictionary"
                | "set"
                | "tuple"
                | "parenthesized_expression"
                | "lambda"
                | "list_comprehension"
                | "dictionary_comprehension"
                | "set_comprehension"
                | "generator_expression"
                // Rust containers
                | "arguments"
                | "tuple_expression"
                | "tuple_pattern"
                | "tuple_struct_pattern"
                | "tuple_type"
                | "array_expression"
                | "type_parameters"
                | "type_arguments"
                | "struct_expression"
                | "struct_pattern"
                | "use_list"
                | "token_tree"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rust(source: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn parse_python(source: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    fn rust_computer() -> IndentComputer {
        let query = include_str!("../queries/rust/indents.scm");
        IndentComputer::new(&tree_sitter_rust::LANGUAGE.into(), query).unwrap()
    }

    fn python_computer() -> IndentComputer {
        let query = include_str!("../queries/python/indents.scm");
        IndentComputer::new(&tree_sitter_python::LANGUAGE.into(), query).unwrap()
    }

    #[test]
    fn test_indent_config_default() {
        let config = IndentConfig::default();
        assert_eq!(config.indent_width, 4);
        assert!(!config.use_tabs);
        assert_eq!(config.tab_width, 4);
    }

    #[test]
    fn test_indent_unit_spaces() {
        let config = IndentConfig {
            indent_width: 2,
            use_tabs: false,
            tab_width: 4,
        };
        assert_eq!(config.indent_unit(), "  ");
    }

    #[test]
    fn test_indent_unit_tabs() {
        let config = IndentConfig {
            indent_width: 4,
            use_tabs: true,
            tab_width: 4,
        };
        assert_eq!(config.indent_unit(), "\t");
    }

    #[test]
    fn test_rust_indent_after_open_brace() {
        let source = "fn main() {\n";
        let tree = parse_rust(source);
        let computer = rust_computer();
        let config = IndentConfig::default();

        // Line 1 (after the newline) should be indented
        let indent = computer.compute_indent(&tree, source, 1, &config);
        assert_eq!(indent, "    ", "Should indent after open brace");
    }

    #[test]
    fn test_rust_no_indent_first_line() {
        let source = "";
        let tree = parse_rust(source);
        let computer = rust_computer();
        let config = IndentConfig::default();

        // First line should have no indent
        let indent = computer.compute_indent(&tree, source, 0, &config);
        assert_eq!(indent, "", "First line should have no indent");
    }

    #[test]
    fn test_rust_maintain_indent() {
        let source = "fn main() {\n    let x = 1;\n";
        let tree = parse_rust(source);
        let computer = rust_computer();
        let config = IndentConfig::default();

        // Line 2 should maintain the same indent as line 1
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(indent, "    ", "Should maintain indent level");
    }

    #[test]
    fn test_python_indent_after_colon() {
        let source = "def foo():\n";
        let tree = parse_python(source);
        let computer = python_computer();
        let config = IndentConfig::default();

        // Line 1 should be indented after the colon
        let indent = computer.compute_indent(&tree, source, 1, &config);
        assert_eq!(indent, "    ", "Should indent after function def colon");
    }

    #[test]
    fn test_python_indent_in_class() {
        let source = "class Foo:\n    def bar(self):\n";
        let tree = parse_python(source);
        let computer = python_computer();
        let config = IndentConfig::default();

        // Line 2 (after method def) should be double-indented
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(indent, "        ", "Should double-indent inside class method");
    }

    #[test]
    fn test_empty_query_returns_none() {
        let result = IndentComputer::new(&tree_sitter_rust::LANGUAGE.into(), "");
        assert!(result.is_none(), "Empty query should return None");
    }

    #[test]
    fn test_line_content_extraction() {
        let computer = rust_computer();

        let source = "line 0\nline 1\nline 2";
        assert_eq!(computer.line_content(source, 0), "line 0");
        assert_eq!(computer.line_content(source, 1), "line 1");
        assert_eq!(computer.line_content(source, 2), "line 2");
    }

    #[test]
    fn test_line_indentation_extraction() {
        let computer = rust_computer();

        let source = "    indented\n\t\ttabs\nno indent";
        assert_eq!(computer.line_indentation(source, 0), "    ");
        assert_eq!(computer.line_indentation(source, 1), "\t\t");
        assert_eq!(computer.line_indentation(source, 2), "");
    }

    #[test]
    fn test_indent_level_calculation() {
        let computer = rust_computer();
        let config = IndentConfig::default();

        assert_eq!(computer.indent_level("", &config), 0);
        assert_eq!(computer.indent_level("    ", &config), 1);
        assert_eq!(computer.indent_level("        ", &config), 2);
        assert_eq!(computer.indent_level("\t", &config), 1);
        assert_eq!(computer.indent_level("\t\t", &config), 2);
    }

    #[test]
    fn test_indent_string_generation() {
        let computer = rust_computer();

        let spaces_config = IndentConfig {
            indent_width: 4,
            use_tabs: false,
            tab_width: 4,
        };
        assert_eq!(computer.indent_string(0, &spaces_config), "");
        assert_eq!(computer.indent_string(1, &spaces_config), "    ");
        assert_eq!(computer.indent_string(2, &spaces_config), "        ");

        let tabs_config = IndentConfig {
            indent_width: 4,
            use_tabs: true,
            tab_width: 4,
        };
        assert_eq!(computer.indent_string(0, &tabs_config), "");
        assert_eq!(computer.indent_string(1, &tabs_config), "\t");
        assert_eq!(computer.indent_string(2, &tabs_config), "\t\t");
    }

    // ========================================================================
    // Single-line bracket regression tests
    // Chunk: docs/chunks/indent_multiline - Multiline check for bracket @indent
    // ========================================================================

    #[test]
    fn test_python_single_line_call_no_indent() {
        // Single-line function call should NOT trigger extra indent
        // The argument_list `()` is a single-line bracket expression
        let source = "def foo():\n    main()\n";
        let tree = parse_python(source);
        let computer = python_computer();
        let config = IndentConfig::default();

        // Line 2 (after `main()`) should maintain indent, not increase
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(
            indent, "    ",
            "Single-line call should not trigger extra indent"
        );
    }

    #[test]
    fn test_python_single_line_list_no_indent() {
        // Single-line list assignment should NOT trigger extra indent
        let source = "def foo():\n    x = [1, 2, 3]\n";
        let tree = parse_python(source);
        let computer = python_computer();
        let config = IndentConfig::default();

        // Line 2 (after list assignment) should maintain indent
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(
            indent, "    ",
            "Single-line list should not trigger extra indent"
        );
    }

    #[test]
    fn test_python_multiline_call_indents() {
        // Multi-line function call SHOULD trigger indent
        let source = "def foo():\n    bar(\n";
        let tree = parse_python(source);
        let computer = python_computer();
        let config = IndentConfig::default();

        // Line 2 (inside multiline call) should be double-indented
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(
            indent, "        ",
            "Multi-line call should trigger indent"
        );
    }

    #[test]
    fn test_python_multiline_list_indents() {
        // Multi-line list SHOULD trigger indent
        let source = "def foo():\n    x = [\n";
        let tree = parse_python(source);
        let computer = python_computer();
        let config = IndentConfig::default();

        // Line 2 (inside multiline list) should be double-indented
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(
            indent, "        ",
            "Multi-line list should trigger indent"
        );
    }

    #[test]
    fn test_rust_single_line_call_no_indent() {
        // Single-line function call in Rust should NOT trigger extra indent
        let source = "fn main() {\n    foo();\n";
        let tree = parse_rust(source);
        let computer = rust_computer();
        let config = IndentConfig::default();

        // Line 2 (after `foo();`) should maintain indent, not increase
        let indent = computer.compute_indent(&tree, source, 2, &config);
        assert_eq!(
            indent, "    ",
            "Rust single-line call should not trigger extra indent"
        );
    }
}
