// Chunk: docs/chunks/treesitter_gotodef - Rust locals query for go-to-definition
//!
//! Rust `locals.scm` query for scope-aware go-to-definition.
//!
//! Based on nvim-treesitter's Rust locals query (MIT licensed).
//! Simplified to focus on the most common go-to-def scenarios:
//! - Local variables (let bindings)
//! - Function parameters
//! - Function definitions
//! - Loop/for variables

/// Rust locals query for scope and definition tracking.
///
/// Captures:
/// - `@local.scope`: Functions, closures, blocks, loops, conditionals
/// - `@local.definition`: Let bindings, parameters, function names
/// - `@local.reference`: All identifiers
pub const LOCALS_QUERY: &str = r#"
; Scopes
; ------
; Nodes that create a new scope for name resolution

[
  (function_item)
  (closure_expression)
  (block)
  (if_expression)
  (match_expression)
  (for_expression)
  (while_expression)
  (loop_expression)
  (impl_item)
  (trait_item)
] @local.scope

; Definitions
; -----------
; Nodes that define a new name

; Function parameters - direct identifier pattern
(parameter
  pattern: (identifier) @local.definition)

; Self parameter in methods
(self_parameter) @local.definition

; Let bindings - direct identifier
(let_declaration
  pattern: (identifier) @local.definition)

; Let bindings with tuple pattern (e.g., let (a, b) = ...)
(let_declaration
  pattern: (tuple_pattern
    (identifier) @local.definition))

; For loop variable
(for_expression
  pattern: (identifier) @local.definition)

; Match arm patterns - need to navigate through match_pattern node
(match_arm
  pattern: (match_pattern
    (identifier) @local.definition))

; If-let pattern
(if_expression
  condition: (let_condition
    pattern: (identifier) @local.definition))

; While-let pattern
(while_expression
  condition: (let_condition
    pattern: (identifier) @local.definition))

; Function names (for local function definitions)
(function_item
  name: (identifier) @local.definition)

; Closure parameters - identifier inside closure_parameters
(closure_parameters
  (identifier) @local.definition)

; Const declarations
(const_item
  name: (identifier) @local.definition)

; Static declarations
(static_item
  name: (identifier) @local.definition)

; Chunk: docs/chunks/treesitter_gotodef_type_resolution - Type-defining constructs
; Struct definitions
(struct_item
  name: (type_identifier) @local.definition)

; Enum definitions
(enum_item
  name: (type_identifier) @local.definition)

; Trait definitions
(trait_item
  name: (type_identifier) @local.definition)

; Type alias definitions
(type_item
  name: (type_identifier) @local.definition)

; Union definitions
(union_item
  name: (type_identifier) @local.definition)

; References
; ----------
; All identifiers are potential references

(identifier) @local.reference

; Type identifiers (struct names, enum names, etc.) are also references
; Chunk: docs/chunks/treesitter_gotodef_type_resolution - Type identifier references
(type_identifier) @local.reference
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    #[test]
    fn test_rust_locals_query_compiles() {
        let language = tree_sitter_rust::LANGUAGE.into();
        let result = Query::new(&language, LOCALS_QUERY);
        assert!(
            result.is_ok(),
            "Rust locals query failed to compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_rust_locals_query_has_expected_captures() {
        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, LOCALS_QUERY).expect("Query should compile");

        let capture_names: Vec<_> = query.capture_names().to_vec();
        assert!(
            capture_names.contains(&"local.scope"),
            "Query should have @local.scope capture"
        );
        assert!(
            capture_names.contains(&"local.definition"),
            "Query should have @local.definition capture"
        );
        assert!(
            capture_names.contains(&"local.reference"),
            "Query should have @local.reference capture"
        );
    }
}
