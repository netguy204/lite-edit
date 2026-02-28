; Chunk: docs/chunks/treesitter_indent - Python indent queries ported from Helix
;
; Tree-sitter indent query for Python. Python's indentation-based syntax
; requires careful handling of block scope. Uses Helix-style captures:
; - @indent: Increment indent level for new lines within this node
; - @outdent: Decrement indent level when this node is encountered
; - @extend: Extend the scope of the parent node

; Block statements that introduce indentation
[
  (function_definition)
  (class_definition)
  (if_statement)
  (elif_clause)
  (else_clause)
  (for_statement)
  (while_statement)
  (try_statement)
  (except_clause)
  (finally_clause)
  (with_statement)
  (match_statement)
  (case_clause)
] @indent

; Explicit block bodies
(block) @indent

; Data structures that span multiple lines
[
  (list)
  (dictionary)
  (set)
  (tuple)
  (parenthesized_expression)
  (argument_list)
  (parameters)
] @indent

; Closing brackets outdent
[
  "]"
  "}"
  ")"
] @outdent

; The colon after a block header should not add additional indent
; (the block itself handles indentation)

; Lambda expressions
(lambda) @indent

; Comprehensions
[
  (list_comprehension)
  (dictionary_comprehension)
  (set_comprehension)
  (generator_expression)
] @indent

; Multiline strings and f-strings should not affect indent
; This prevents indent from being added inside string literals
(string) @indent.ignore
(concatenated_string) @indent.ignore

; Comment handling - comments don't affect indent
(comment) @indent.ignore
