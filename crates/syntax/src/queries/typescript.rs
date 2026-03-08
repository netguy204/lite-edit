// Chunk: docs/chunks/tsx_goto_functions - Custom locals query for TS/TSX go-to-definition
//!
//! TypeScript/TSX `locals.scm` query for scope-aware go-to-definition.
//!
//! The upstream `tree_sitter_typescript::LOCALS_QUERY` only captures
//! `required_parameter` and `optional_parameter`, which is far too minimal
//! for goto-def to work on function declarations, variable declarations
//! (including arrow functions), or class declarations.
//!
//! This custom query captures the same constructs as the JavaScript locals
//! query plus TypeScript-specific parameter forms.

/// TypeScript/TSX locals query for scope and definition tracking.
///
/// Captures:
/// - `@local.scope`: Functions, classes, blocks, loops, conditionals
/// - `@local.definition`: Variable declarations, function names, class names, parameters
/// - `@local.reference`: All identifiers and type identifiers
pub const LOCALS_QUERY: &str = r#"
; Scopes
; ------
; Nodes that create a new scope for name resolution

[
  (statement_block)
  (function_expression)
  (function_declaration)
  (arrow_function)
  (method_definition)
  (class_declaration)
  (class)
  (for_statement)
  (for_in_statement)
  (while_statement)
  (do_statement)
  (if_statement)
  (switch_case)
] @local.scope

; Definitions
; -----------
; Nodes that define a new name

; Variable declarations: const Foo = ..., let x = ..., var y = ...
(variable_declarator
  name: (identifier) @local.definition)

; Function declarations: function foo() {}
(function_declaration
  name: (identifier) @local.definition)

; Class declarations: class Foo {}
(class_declaration
  name: (type_identifier) @local.definition)

; TypeScript required parameters: (name: Type)
(required_parameter
  (identifier) @local.definition)

; TypeScript optional parameters: (name?: Type)
(optional_parameter
  (identifier) @local.definition)

; References
; ----------
; All identifiers are potential references

(identifier) @local.reference

; Type identifiers (class names, interface names, etc.) are also references
(type_identifier) @local.reference
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    #[test]
    fn test_typescript_locals_query_compiles_ts() {
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let result = Query::new(&language, LOCALS_QUERY);
        assert!(
            result.is_ok(),
            "TypeScript locals query failed to compile against LANGUAGE_TYPESCRIPT: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_typescript_locals_query_compiles_tsx() {
        let language = tree_sitter_typescript::LANGUAGE_TSX.into();
        let result = Query::new(&language, LOCALS_QUERY);
        assert!(
            result.is_ok(),
            "TypeScript locals query failed to compile against LANGUAGE_TSX: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_typescript_locals_query_has_expected_captures() {
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
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
