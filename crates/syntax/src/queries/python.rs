// Chunk: docs/chunks/treesitter_gotodef - Python locals query for go-to-definition
//!
//! Python `locals.scm` query for scope-aware go-to-definition.
//!
//! Based on nvim-treesitter's Python locals query (MIT licensed).
//! Simplified to focus on the most common go-to-def scenarios:
//! - Local variables (assignments)
//! - Function parameters
//! - Function and class definitions
//! - For loop variables
//! - Comprehension variables

/// Python locals query for scope and definition tracking.
///
/// Captures:
/// - `@local.scope`: Functions, classes, modules, comprehensions
/// - `@local.definition`: Parameters, assignments, function/class names
/// - `@local.reference`: All identifiers
pub const LOCALS_QUERY: &str = r#"
; Scopes
; ------
; Nodes that create a new scope for name resolution

[
  (module)
  (function_definition)
  (class_definition)
  (list_comprehension)
  (dictionary_comprehension)
  (set_comprehension)
  (generator_expression)
  (lambda)
] @local.scope

; Definitions
; -----------
; Nodes that define a new name

; Function parameters
(parameters
  (identifier) @local.definition)

; Default parameter
(default_parameter
  name: (identifier) @local.definition)

; Typed parameter
(typed_parameter
  (identifier) @local.definition)

; Typed default parameter
(typed_default_parameter
  name: (identifier) @local.definition)

; *args and **kwargs
(parameters
  (list_splat_pattern
    (identifier) @local.definition))
(parameters
  (dictionary_splat_pattern
    (identifier) @local.definition))

; Assignment targets
(assignment
  left: (identifier) @local.definition)

; Pattern assignment (tuple unpacking)
(assignment
  left: (pattern_list
    (identifier) @local.definition))
(assignment
  left: (tuple_pattern
    (identifier) @local.definition))

; Augmented assignment (+=, -=, etc.)
(augmented_assignment
  left: (identifier) @local.definition)

; For loop variable
(for_statement
  left: (identifier) @local.definition)
(for_statement
  left: (pattern_list
    (identifier) @local.definition))
(for_statement
  left: (tuple_pattern
    (identifier) @local.definition))

; Comprehension variables
(for_in_clause
  left: (identifier) @local.definition)
(for_in_clause
  left: (pattern_list
    (identifier) @local.definition))
(for_in_clause
  left: (tuple_pattern
    (identifier) @local.definition))

; With statement (context manager variable)
(with_clause
  (with_item
    value: (as_pattern
      alias: (as_pattern_target
        (identifier) @local.definition))))

; Except handler variable
(except_clause
  (as_pattern
    alias: (as_pattern_target
      (identifier) @local.definition)))

; Function name
(function_definition
  name: (identifier) @local.definition)

; Class name
(class_definition
  name: (identifier) @local.definition)

; Walrus operator (:=)
(named_expression
  name: (identifier) @local.definition)

; Import aliases
(aliased_import
  alias: (identifier) @local.definition)

; From import names
(import_from_statement
  name: (dotted_name
    (identifier) @local.definition))

; References
; ----------
; All identifiers are potential references

(identifier) @local.reference
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Query;

    #[test]
    fn test_python_locals_query_compiles() {
        let language = tree_sitter_python::LANGUAGE.into();
        let result = Query::new(&language, LOCALS_QUERY);
        assert!(
            result.is_ok(),
            "Python locals query failed to compile: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_python_locals_query_has_expected_captures() {
        let language = tree_sitter_python::LANGUAGE.into();
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
