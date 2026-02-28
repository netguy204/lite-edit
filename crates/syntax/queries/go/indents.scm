; Chunk: docs/chunks/treesitter_indent - Go indent queries
;
; Tree-sitter indent query for Go.

; Block structures
[
  (block)
  (literal_value)
  (struct_type)
  (interface_type)
  (argument_list)
  (parameter_list)
  (type_parameter_list)
  (field_declaration_list)
  (import_spec_list)
  (const_spec)
  (var_spec)
  (expression_switch_statement)
  (type_switch_statement)
  (select_statement)
] @indent

; Case clauses
[
  (expression_case)
  (type_case)
  (default_case)
  (communication_case)
] @indent

; Closing delimiters
[
  "}"
  "]"
  ")"
] @outdent
