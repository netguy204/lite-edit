; Chunk: docs/chunks/treesitter_indent - Bash indent queries
;
; Tree-sitter indent query for Bash.

; Block structures
[
  (compound_statement)
  (subshell)
  (command_substitution)
  (process_substitution)
  (array)
] @indent

; Control flow
[
  (if_statement)
  (elif_clause)
  (else_clause)
  (for_statement)
  (while_statement)
  (until_statement)
  (case_statement)
  (case_item)
  (function_definition)
] @indent

; Closing keywords and delimiters
[
  "fi"
  "done"
  "esac"
  ";;"
  "}"
  "]"
  ")"
] @outdent
