; Chunk: docs/chunks/treesitter_indent - C indent queries
;
; Tree-sitter indent query for C.

; Block structures
[
  (compound_statement)
  (field_declaration_list)
  (enumerator_list)
  (initializer_list)
  (argument_list)
  (parameter_list)
] @indent

; Case statements
[
  (case_statement)
  (default_statement)
] @indent

; Preprocessor regions (optional - may want different behavior)
; (preproc_if) @indent
; (preproc_ifdef) @indent

; Closing delimiters
[
  "}"
  "]"
  ")"
] @outdent
