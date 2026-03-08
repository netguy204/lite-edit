// Chunk: docs/chunks/treesitter_gotodef - Go-to-definition using tree-sitter locals queries
//!
//! Go-to-definition resolution using tree-sitter locals queries.
//!
//! This module provides `LocalsResolver`, which uses tree-sitter's locals queries
//! to find definitions for identifiers. The algorithm:
//!
//! 1. Find the identifier node at the cursor position (`@local.reference`)
//! 2. Walk enclosing scopes (`@local.scope`) from innermost to outermost
//! 3. For each scope, find definitions (`@local.definition`) that match the identifier
//! 4. Return the first matching definition's position
//!
//! This approach works within a single file and handles:
//! - Local variables (let bindings in Rust, assignments in Python)
//! - Function parameters
//! - Function definitions
//! - Loop variables
//! - Pattern bindings
//!
//! It does NOT handle:
//! - Cross-file definitions (imports, external modules)
//! - Standard library or third-party symbols

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCapture, QueryCursor, Tree};

// Chunk: docs/chunks/treesitter_gotodef - Capture index lookup for @local.scope, @local.definition, @local.reference
/// Index values for capture names in the locals query.
struct CaptureIndices {
    scope: Option<u32>,
    definition: Option<u32>,
    reference: Option<u32>,
}

impl CaptureIndices {
    fn from_query(query: &Query) -> Self {
        let capture_names = query.capture_names();
        Self {
            scope: capture_names
                .iter()
                .position(|n| *n == "local.scope")
                .map(|i| i as u32),
            definition: capture_names
                .iter()
                .position(|n| *n == "local.definition")
                .map(|i| i as u32),
            reference: capture_names
                .iter()
                .position(|n| *n == "local.reference")
                .map(|i| i as u32),
        }
    }
}

/// Resolves go-to-definition for identifiers using tree-sitter locals queries.
///
/// `LocalsResolver` is constructed with a compiled locals query and can be reused
/// across multiple resolution requests for efficiency.
pub struct LocalsResolver {
    query: Query,
    indices: CaptureIndices,
}

impl LocalsResolver {
    /// Creates a new LocalsResolver with the given tree-sitter query.
    ///
    /// The query must contain the standard locals captures:
    /// - `@local.scope`: Nodes that create a new scope
    /// - `@local.definition`: Nodes that define a name
    /// - `@local.reference`: Nodes that reference a name
    ///
    /// # Errors
    ///
    /// Returns `None` if the query doesn't compile or lacks required captures.
    pub fn new(language: tree_sitter::Language, locals_query: &str) -> Option<Self> {
        // Empty query means no locals support for this language
        if locals_query.is_empty() {
            return None;
        }

        let query = Query::new(&language, locals_query).ok()?;
        let indices = CaptureIndices::from_query(&query);

        // Require at least definition and reference captures
        if indices.definition.is_none() || indices.reference.is_none() {
            return None;
        }

        Some(Self { query, indices })
    }

    /// Finds the definition for the identifier at the given byte offset.
    ///
    /// Returns the byte range of the definition if found, or `None` if:
    /// - No identifier at the given position
    /// - No definition found in any enclosing scope
    /// - The position is already on a definition
    ///
    /// The byte offset should point within an identifier node.
    pub fn find_definition(
        &self,
        tree: &Tree,
        source: &[u8],
        byte_offset: usize,
    ) -> Option<std::ops::Range<usize>> {
        let root = tree.root_node();

        // Run the query to get all captures
        // NOTE: QueryCaptures implements StreamingIterator, not Iterator,
        // so we need to use a while loop with .next() instead of iterator adapters.
        let mut cursor = QueryCursor::new();
        let mut captures: Vec<QueryCapture> = Vec::new();
        let mut captures_iter = cursor.captures(&self.query, root, source);
        while let Some((mat, capture_idx)) = captures_iter.next() {
            let capture = mat.captures[*capture_idx];
            captures.push(capture);
        }

        // Find the reference at the cursor position
        let reference = self.find_reference_at(&captures, byte_offset)?;
        let ref_name = reference.node.utf8_text(source).ok()?;

        // Check if this reference is also a definition (user is on the definition itself)
        if self.is_definition(&captures, reference.node) {
            return None;
        }

        // Find enclosing scopes for the reference
        let scopes = self.find_enclosing_scopes(&captures, reference.node);

        // Search for definition in scopes from innermost to outermost
        for scope in scopes {
            if let Some(def) = self.find_definition_in_scope(&captures, scope, ref_name, source) {
                // Only return definitions that come before the reference in the same scope,
                // or any definition from an outer scope
                let def_start = def.node.start_byte();
                let ref_start = reference.node.start_byte();

                // Definition must be before reference, or in an outer scope
                if def_start < ref_start || !self.node_contains(scope, reference.node) {
                    return Some(def.node.byte_range());
                }
            }
        }

        // Also check the root scope (module-level definitions)
        if let Some(def) = self.find_definition_at_root(&captures, ref_name, source) {
            let def_start = def.node.start_byte();
            let ref_start = reference.node.start_byte();
            if def_start < ref_start {
                return Some(def.node.byte_range());
            }
        }

        None
    }

    /// Finds a reference capture at the given byte offset.
    fn find_reference_at<'a>(
        &self,
        captures: &'a [QueryCapture<'a>],
        byte_offset: usize,
    ) -> Option<&'a QueryCapture<'a>> {
        let ref_idx = self.indices.reference?;
        captures.iter().find(|c| {
            c.index == ref_idx
                && c.node.start_byte() <= byte_offset
                && c.node.end_byte() > byte_offset
        })
    }

    /// Checks if a node is also captured as a definition.
    fn is_definition(&self, captures: &[QueryCapture], node: Node) -> bool {
        let def_idx = match self.indices.definition {
            Some(idx) => idx,
            None => return false,
        };
        captures
            .iter()
            .any(|c| c.index == def_idx && c.node.id() == node.id())
    }

    /// Finds all scopes that contain the given node, from innermost to outermost.
    fn find_enclosing_scopes<'a>(
        &self,
        captures: &'a [QueryCapture<'a>],
        node: Node,
    ) -> Vec<Node<'a>> {
        let scope_idx = match self.indices.scope {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let mut scopes: Vec<Node<'a>> = captures
            .iter()
            .filter(|c| c.index == scope_idx && self.node_contains(c.node, node))
            .map(|c| c.node)
            .collect();

        // Sort by scope size (smallest/innermost first)
        scopes.sort_by_key(|s| s.end_byte() - s.start_byte());

        scopes
    }

    /// Checks if `outer` contains `inner`.
    fn node_contains(&self, outer: Node, inner: Node) -> bool {
        outer.start_byte() <= inner.start_byte() && outer.end_byte() >= inner.end_byte()
    }

    /// Finds a definition with the given name in the specified scope.
    fn find_definition_in_scope<'a>(
        &self,
        captures: &'a [QueryCapture<'a>],
        scope: Node,
        name: &str,
        source: &[u8],
    ) -> Option<&'a QueryCapture<'a>> {
        let def_idx = self.indices.definition?;

        captures.iter().find(|c| {
            c.index == def_idx
                && self.node_contains(scope, c.node)
                && c.node.utf8_text(source).ok() == Some(name)
        })
    }

    /// Finds a definition at the root level (module scope, not inside any explicit scope).
    fn find_definition_at_root<'a>(
        &self,
        captures: &'a [QueryCapture<'a>],
        name: &str,
        source: &[u8],
    ) -> Option<&'a QueryCapture<'a>> {
        let def_idx = self.indices.definition?;

        captures.iter().find(|c| {
            c.index == def_idx && c.node.utf8_text(source).ok() == Some(name)
        })
    }
}

// Chunk: docs/chunks/treesitter_symbol_index - Extract identifier at cursor for cross-file lookup
/// Extracts the identifier name at the given byte position.
///
/// Walks down the tree from the root to find the smallest node containing the byte
/// position, then checks if it or its immediate parent is an identifier-like node.
///
/// Returns the identifier text if found, `None` otherwise.
///
/// This is used for cross-file go-to-definition: when same-file resolution fails,
/// we extract the identifier name and look it up in the workspace symbol index.
pub fn identifier_at_position(tree: &Tree, source: &[u8], byte_offset: usize) -> Option<String> {
    let root = tree.root_node();

    // Find the deepest node containing this position
    let node = root.descendant_for_byte_range(byte_offset, byte_offset)?;

    // Check if this node or its parent is an identifier
    // Different languages use different node kinds for identifiers:
    // - Rust: "identifier", "type_identifier"
    // - Python: "identifier"
    // - JavaScript/TypeScript: "identifier", "property_identifier"
    // - Go: "identifier", "type_identifier"
    const IDENTIFIER_KINDS: &[&str] = &[
        "identifier",
        "type_identifier",
        "property_identifier",
        "field_identifier",
        "shorthand_property_identifier",
    ];

    // Check current node
    if IDENTIFIER_KINDS.contains(&node.kind()) {
        return node.utf8_text(source).ok().map(String::from);
    }

    // Check parent (handles cases where cursor is inside an identifier pattern)
    if let Some(parent) = node.parent() {
        if IDENTIFIER_KINDS.contains(&parent.kind()) {
            return parent.utf8_text(source).ok().map(String::from);
        }

        // Also check if we're on a name field of a definition node
        // This handles patterns like function names, struct names, etc.
        for i in 0..parent.child_count() {
            if let Some(field_name) = parent.field_name_for_child(i as u32) {
                if field_name == "name" || field_name == "identifier" {
                    if let Some(name_child) = parent.child(i) {
                        if name_child.start_byte() <= byte_offset && name_child.end_byte() > byte_offset {
                            return name_child.utf8_text(source).ok().map(String::from);
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rust_resolver() -> LocalsResolver {
        LocalsResolver::new(
            tree_sitter_rust::LANGUAGE.into(),
            crate::queries::rust::LOCALS_QUERY,
        )
        .expect("Rust resolver should be created")
    }

    fn make_python_resolver() -> LocalsResolver {
        LocalsResolver::new(
            tree_sitter_python::LANGUAGE.into(),
            crate::queries::python::LOCALS_QUERY,
        )
        .expect("Python resolver should be created")
    }

    fn make_js_resolver() -> LocalsResolver {
        LocalsResolver::new(
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::LOCALS_QUERY,
        )
        .expect("JS resolver should be created")
    }

    // Chunk: docs/chunks/tsx_goto_functions - TSX resolver and parser for go-to-definition tests
    fn make_tsx_resolver() -> LocalsResolver {
        LocalsResolver::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            crate::queries::typescript::LOCALS_QUERY,
        )
        .expect("TSX resolver should be created")
    }

    fn parse_tsx(code: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        parser.parse(code, None).unwrap()
    }

    fn parse_rust(code: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(code, None).unwrap()
    }

    fn parse_python(code: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        parser.parse(code, None).unwrap()
    }

    fn parse_js(code: &str) -> Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .unwrap();
        parser.parse(code, None).unwrap()
    }

    #[test]
    fn test_rust_local_variable() {
        let resolver = make_rust_resolver();
        let code = r#"
fn foo() {
    let x = 42;
    println!("{}", x);
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to x in the println!
        let ref_pos = code.find("x);").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for x");
        let range = result.unwrap();
        // The definition should be at the "x" in "let x = 42"
        let def_text = &code[range.clone()];
        assert_eq!(def_text, "x", "Definition should be 'x', got '{}'", def_text);
    }

    #[test]
    fn test_rust_function_parameter() {
        let resolver = make_rust_resolver();
        let code = r#"
fn greet(name: &str) {
    println!("Hello, {}", name);
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to name in the println!
        let ref_pos = code.find("name);").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for name");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "name",
            "Definition should be 'name', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_on_definition_returns_none() {
        let resolver = make_rust_resolver();
        let code = r#"
fn foo() {
    let x = 42;
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Try to go to definition when already on the definition
        let def_pos = code.find("let x").unwrap() + 4; // Position on "x" in "let x"
        let result = resolver.find_definition(&tree, source, def_pos);

        assert!(
            result.is_none(),
            "Should return None when on definition itself"
        );
    }

    #[test]
    fn test_python_local_variable() {
        let resolver = make_python_resolver();
        let code = r#"
def foo():
    x = 42
    print(x)
"#;
        let tree = parse_python(code);
        let source = code.as_bytes();

        // Find the reference to x in print(x)
        let ref_pos = code.find("print(x)").unwrap() + 6; // Position on "x"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for x");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(def_text, "x", "Definition should be 'x', got '{}'", def_text);
    }

    #[test]
    fn test_python_function_parameter() {
        let resolver = make_python_resolver();
        let code = r#"
def greet(name):
    print(name)
"#;
        let tree = parse_python(code);
        let source = code.as_bytes();

        // Find the reference to name in print(name)
        let ref_pos = code.find("print(name)").unwrap() + 6;
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for name");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "name",
            "Definition should be 'name', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_js_local_variable() {
        let resolver = make_js_resolver();
        let code = r#"
function foo() {
    let x = 42;
    console.log(x);
}
"#;
        let tree = parse_js(code);
        let source = code.as_bytes();

        // Find the reference to x in console.log(x)
        let ref_pos = code.find("log(x)").unwrap() + 4;
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for x");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(def_text, "x", "Definition should be 'x', got '{}'", def_text);
    }

    #[test]
    fn test_unknown_identifier_returns_none() {
        let resolver = make_rust_resolver();
        let code = r#"
fn foo() {
    println!("{}", unknown);
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Try to find definition for an undefined identifier
        let ref_pos = code.find("unknown").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_none(), "Should return None for unknown identifier");
    }

    #[test]
    fn test_empty_query_returns_none() {
        let result = LocalsResolver::new(tree_sitter_rust::LANGUAGE.into(), "");
        assert!(result.is_none(), "Empty query should return None");
    }

    #[test]
    fn test_rust_for_loop_variable() {
        let resolver = make_rust_resolver();
        let code = r#"
fn foo() {
    for item in items {
        println!("{}", item);
    }
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to item in println!
        let ref_pos = code.find("item);").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for item");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "item",
            "Definition should be 'item', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_nested_scope() {
        let resolver = make_rust_resolver();
        let code = r#"
fn foo() {
    let x = 1;
    {
        let x = 2;
        println!("{}", x);
    }
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to x in the inner scope
        let ref_pos = code.find("x);").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for inner x");
        let range = result.unwrap();
        // Should find the inner x (let x = 2), not the outer one
        let def_pos = code.find("let x = 2").unwrap() + 4;
        assert_eq!(
            range.start, def_pos,
            "Should find inner scope definition, not outer"
        );
    }

    #[test]
    fn test_rust_locally_defined_function() {
        let resolver = make_rust_resolver();
        let code = r#"
fn outer() {
    fn inner(x: i32) -> i32 {
        x + 1
    }
    let result = inner(42);
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to inner in the call
        let ref_pos = code.find("inner(42)").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for inner function");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "inner",
            "Definition should be 'inner', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_cursor_on_non_identifier() {
        let resolver = make_rust_resolver();
        let code = r#"
fn foo() {
    let x = 42;
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Try to find definition when cursor is on "42" (a number, not an identifier)
        let num_pos = code.find("42").unwrap();
        let result = resolver.find_definition(&tree, source, num_pos);

        assert!(
            result.is_none(),
            "Should return None when cursor is on a non-identifier"
        );
    }

    #[test]
    fn test_empty_file() {
        let resolver = make_rust_resolver();
        let code = "";
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Try to find definition in empty file
        let result = resolver.find_definition(&tree, source, 0);

        assert!(result.is_none(), "Should return None for empty file");
    }

    #[test]
    fn test_python_locally_defined_function() {
        let resolver = make_python_resolver();
        let code = r#"
def outer():
    def inner(x):
        return x + 1
    result = inner(42)
"#;
        let tree = parse_python(code);
        let source = code.as_bytes();

        // Find the reference to inner in the call
        let ref_pos = code.find("inner(42)").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(
            result.is_some(),
            "Should find definition for inner function"
        );
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "inner",
            "Definition should be 'inner', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_typescript_local_variable() {
        // TypeScript's LOCALS_QUERY may be empty or incomplete depending on tree-sitter version.
        // If so, skip this test gracefully.
        let resolver = match LocalsResolver::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::LOCALS_QUERY,
        ) {
            Some(r) => r,
            None => {
                eprintln!(
                    "Skipping TypeScript test: LOCALS_QUERY is empty or missing required captures"
                );
                return;
            }
        };

        let code = r#"
function greet(name: string): void {
    const message = "Hello, " + name;
    console.log(message);
}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let source = code.as_bytes();

        // Find the reference to message in console.log
        let ref_pos = code.find("log(message)").unwrap() + 4;
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for message");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "message",
            "Definition should be 'message', got '{}'",
            def_text
        );
    }

    // Chunk: docs/chunks/treesitter_symbol_index - Tests for identifier_at_position
    #[test]
    fn test_identifier_at_position_rust_variable() {
        let code = r#"
fn foo() {
    let name = 42;
    println!("{}", name);
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Point at "name" in println
        let pos = code.rfind("name").unwrap();
        let result = super::identifier_at_position(&tree, source, pos);

        assert_eq!(result, Some("name".to_string()));
    }

    #[test]
    fn test_identifier_at_position_rust_function_name() {
        let code = r#"
fn my_function() {
    println!("Hello");
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Point at "my_function"
        let pos = code.find("my_function").unwrap();
        let result = super::identifier_at_position(&tree, source, pos);

        assert_eq!(result, Some("my_function".to_string()));
    }

    #[test]
    fn test_identifier_at_position_python_variable() {
        let code = r#"
def greet():
    name = "World"
    print(name)
"#;
        let tree = parse_python(code);
        let source = code.as_bytes();

        // Point at "name" in print
        let pos = code.rfind("name").unwrap();
        let result = super::identifier_at_position(&tree, source, pos);

        assert_eq!(result, Some("name".to_string()));
    }

    #[test]
    fn test_identifier_at_position_no_identifier() {
        let code = r#"
fn foo() {
    // Just a comment
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Point at whitespace
        let pos = code.find("// Just").unwrap() - 1;
        let result = super::identifier_at_position(&tree, source, pos);

        assert_eq!(result, None);
    }

    // Chunk: docs/chunks/treesitter_gotodef_type_resolution - Type identifier resolution tests
    #[test]
    fn test_rust_type_identifier_in_function_parameter() {
        let resolver = make_rust_resolver();
        let code = r#"
struct Point {
    x: i32,
    y: i32,
}

fn distance(p: Point) -> f64 {
    ((p.x * p.x + p.y * p.y) as f64).sqrt()
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to Point in the function parameter
        let ref_pos = code.find("p: Point").unwrap() + 3; // Position on "Point"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for type Point");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Point",
            "Definition should be 'Point', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_enum_type_in_variable_binding() {
        let resolver = make_rust_resolver();
        let code = r#"
enum Color {
    Red,
    Green,
    Blue,
}

fn paint() {
    let fg: Color = Color::Red;
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to Color in "let fg: Color"
        let ref_pos = code.find("fg: Color").unwrap() + 4; // Position on "Color"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for enum Color");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Color",
            "Definition should be 'Color', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_type_alias_resolution() {
        let resolver = make_rust_resolver();
        let code = r#"
type Distance = f64;

fn measure() -> Distance {
    42.0
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to Distance in return type
        let ref_pos = code.find("-> Distance").unwrap() + 3; // Position on "Distance"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for type alias Distance");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Distance",
            "Definition should be 'Distance', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_trait_resolution() {
        let resolver = make_rust_resolver();
        let code = r#"
trait Drawable {
    fn draw(&self);
}

impl Drawable for Point {
    fn draw(&self) {}
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to Drawable in "impl Drawable"
        let ref_pos = code.find("impl Drawable").unwrap() + 5; // Position on "Drawable"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for trait Drawable");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Drawable",
            "Definition should be 'Drawable', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_struct_in_generic_type() {
        let resolver = make_rust_resolver();
        let code = r#"
struct Span {
    start: usize,
    end: usize,
}

fn spans() -> Vec<Span> {
    Vec::new()
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to Span in Vec<Span>
        let ref_pos = code.find("Vec<Span>").unwrap() + 4; // Position on "Span"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for Span in generic");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Span",
            "Definition should be 'Span', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_rust_on_type_definition_returns_none() {
        let resolver = make_rust_resolver();
        let code = r#"
struct Point {
    x: i32,
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Try to go to definition when already on the definition
        let def_pos = code.find("struct Point").unwrap() + 7; // Position on "Point"
        let result = resolver.find_definition(&tree, source, def_pos);

        assert!(
            result.is_none(),
            "Should return None when on type definition itself"
        );
    }

    #[test]
    fn test_rust_type_in_struct_field() {
        let resolver = make_rust_resolver();
        let code = r#"
struct Inner {
    value: i32,
}

struct Outer {
    inner: Inner,
}
"#;
        let tree = parse_rust(code);
        let source = code.as_bytes();

        // Find the reference to Inner in "inner: Inner"
        let ref_pos = code.find("inner: Inner").unwrap() + 7; // Position on "Inner"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for Inner in struct field");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Inner",
            "Definition should be 'Inner', got '{}'",
            def_text
        );
    }

    // Chunk: docs/chunks/tsx_goto_functions - TSX go-to-definition tests

    #[test]
    fn test_tsx_function_declaration() {
        let resolver = make_tsx_resolver();
        let code = r#"
function greet(name: string): string {
    return "Hello, " + name;
}

const result = greet("World");
"#;
        let tree = parse_tsx(code);
        let source = code.as_bytes();

        // Find the reference to greet in the call expression
        let ref_pos = code.find("greet(\"World\")").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for greet");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "greet",
            "Definition should be 'greet', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_tsx_arrow_function() {
        let resolver = make_tsx_resolver();
        let code = r#"
const Foo = () => {
    return <div>hello</div>;
};

const element = Foo();
"#;
        let tree = parse_tsx(code);
        let source = code.as_bytes();

        // Find the reference to Foo in the call
        let ref_pos = code.find("Foo()").unwrap();
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for Foo");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Foo",
            "Definition should be 'Foo', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_tsx_jsx_element_resolution() {
        let resolver = make_tsx_resolver();
        let code = r#"
const MyComponent = () => {
    return <div>hello</div>;
};

const App = () => {
    return <MyComponent />;
};
"#;
        let tree = parse_tsx(code);
        let source = code.as_bytes();

        // Find the reference to MyComponent inside JSX
        let ref_pos = code.find("<MyComponent />").unwrap() + 1; // Position on "MyComponent"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(
            result.is_some(),
            "Should find definition for MyComponent in JSX element"
        );
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "MyComponent",
            "Definition should be 'MyComponent', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_tsx_class_declaration() {
        let resolver = make_tsx_resolver();
        let code = r#"
class Widget {
    render() {
        return <div />;
    }
}

const w: Widget = new Widget();
"#;
        let tree = parse_tsx(code);
        let source = code.as_bytes();

        // Find the reference to Widget in the type annotation
        let ref_pos = code.find("w: Widget").unwrap() + 3; // Position on "Widget"
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for class Widget");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "Widget",
            "Definition should be 'Widget', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_tsx_typescript_local_variable_with_custom_query() {
        // This test verifies that the custom query fixes the existing
        // test_typescript_local_variable scenario (which was previously skipped
        // due to the upstream query being too minimal).
        let resolver = make_tsx_resolver();
        let code = r#"
function greet(name: string): void {
    const message = "Hello, " + name;
    console.log(message);
}
"#;
        let tree = parse_tsx(code);
        let source = code.as_bytes();

        // Find the reference to message in console.log
        let ref_pos = code.find("log(message)").unwrap() + 4;
        let result = resolver.find_definition(&tree, source, ref_pos);

        assert!(result.is_some(), "Should find definition for message");
        let range = result.unwrap();
        let def_text = &code[range.clone()];
        assert_eq!(
            def_text, "message",
            "Definition should be 'message', got '{}'",
            def_text
        );
    }

    #[test]
    fn test_identifier_at_position_tsx_jsx_element() {
        let code = r#"const App = () => <MyComponent />;"#;
        let tree = parse_tsx(code);
        let source = code.as_bytes();

        // Point at "MyComponent" inside JSX
        let pos = code.find("MyComponent").unwrap();
        let result = super::identifier_at_position(&tree, source, pos);

        assert_eq!(
            result,
            Some("MyComponent".to_string()),
            "Should extract identifier from JSX element"
        );
    }
}
