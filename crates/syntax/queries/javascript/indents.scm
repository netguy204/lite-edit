; Chunk: docs/chunks/treesitter_indent - JavaScript indent queries
;
; Tree-sitter indent query for JavaScript.

; Block structures
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

; JSX elements
[
  (jsx_element)
  (jsx_self_closing_element)
  (jsx_expression)
] @indent

; Closing delimiters
[
  "}"
  "]"
  ")"
] @outdent
