; Chunk: docs/chunks/treesitter_indent - CSS indent queries
;
; Tree-sitter indent query for CSS.

[
  (block)
  (declaration)
] @indent

[
  "}"
] @outdent
