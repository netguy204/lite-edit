; Chunk: docs/chunks/treesitter_indent - JSON indent queries
;
; Tree-sitter indent query for JSON.

[
  (object)
  (array)
] @indent

[
  "}"
  "]"
] @outdent
