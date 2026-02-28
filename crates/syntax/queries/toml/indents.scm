; Chunk: docs/chunks/treesitter_indent - TOML indent queries
;
; Tree-sitter indent query for TOML.
; TOML doesn't have block structures like most languages,
; but inline tables and arrays can span multiple lines.

[
  (inline_table)
  (array)
] @indent

[
  "}"
  "]"
] @outdent
