; Chunk: docs/chunks/treesitter_indent - C++ indent queries
;
; Tree-sitter indent query for C++. Extends C patterns.

; Block structures (same as C plus C++-specific)
[
  (compound_statement)
  (field_declaration_list)
  (enumerator_list)
  (initializer_list)
  (argument_list)
  (parameter_list)
  (template_parameter_list)
  (template_argument_list)
] @indent

; Case statements
[
  (case_statement)
  (default_statement)
] @indent

; C++ specific
[
  (access_specifier)
] @outdent

; Lambda expressions
(lambda_expression
  body: (_) @indent)

; Namespaces
(namespace_definition
  body: (_) @indent)

; Class body
(class_specifier
  body: (_) @indent)

; Closing delimiters
[
  "}"
  "]"
  ")"
  ">"
] @outdent
