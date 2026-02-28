; Chunk: docs/chunks/treesitter_indent - TypeScript indent queries
;
; Tree-sitter indent query for TypeScript. Extends JavaScript patterns
; with TypeScript-specific constructs.

; Block structures (same as JavaScript)
[
  (statement_block)
  (class_body)
  (object)
  (array)
  (arguments)
  (formal_parameters)
  (template_string)
  (named_imports)
  (export_clause)
  (switch_body)
] @indent

; Case clauses
[
  (switch_case)
  (switch_default)
] @indent

; Arrow function body
(arrow_function
  body: (_) @indent
  (#not-kind? @indent "statement_block"))

; JSX/TSX elements
[
  (jsx_element)
  (jsx_self_closing_element)
  (jsx_expression)
] @indent

; TypeScript-specific constructs
[
  (type_parameters)
  (type_arguments)
  (object_type)
  (enum_body)
  (interface_body)
] @indent

; Closing delimiters
[
  "}"
  "]"
  ")"
  ">"
] @outdent
