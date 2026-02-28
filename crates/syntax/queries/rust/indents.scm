; Chunk: docs/chunks/treesitter_indent - Rust indent queries ported from Helix
;
; Tree-sitter indent query for Rust. Uses Helix-style captures:
; - @indent: Increment indent level for new lines within this node
; - @outdent: Decrement indent level when this node is encountered
; - @indent.always: Increment even for multiple captures on the same line
; - @extend: Extend the scope of the parent node to include this node

; Blocks and delimiters
[
  (block)
  (match_block)
  (struct_expression)
  (struct_pattern)
  (tuple_expression)
  (tuple_pattern)
  (tuple_struct_pattern)
  (tuple_type)
  (array_expression)
  (arguments)
  (parameters)
  (type_parameters)
  (type_arguments)
  (declaration_list)
  (enum_variant_list)
  (field_declaration_list)
  (field_initializer_list)
  (use_list)
  (token_tree)
] @indent

; Closing delimiters outdent
[
  "}"
  "]"
  ")"
] @outdent

; Match arms indent
(match_arm) @indent

; Closures
(closure_expression
  body: (_) @indent)

; Chained method calls - continue indent for `.`
(call_expression
  function: (field_expression
    "." @indent))

; Function body expressions without block
(function_item
  body: (_) @indent
  (#not-kind? @indent "block"))

; If/else expressions
(if_expression
  consequence: (block) @indent)
(if_expression
  alternative: (else_clause
    (block) @indent))

; Loop expressions
(loop_expression
  body: (block) @indent)
(while_expression
  body: (block) @indent)
(for_expression
  body: (block) @indent)

; Impl blocks
(impl_item
  body: (declaration_list) @indent)

; Trait blocks
(trait_item
  body: (declaration_list) @indent)

; Struct/enum definitions
(struct_item
  body: (field_declaration_list) @indent)
(enum_item
  body: (enum_variant_list) @indent)

; Where clauses extend the function signature
(where_clause) @indent

; Let bindings - only capture block-like values that span lines
; Simple values like integers don't need special indent handling

; Macros
(macro_definition
  (macro_rule
    (token_tree) @indent))
