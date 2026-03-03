; Chunk: docs/chunks/treesitter_indent - Markdown indent queries
;
; Tree-sitter indent query for Markdown.
; Markdown is mostly freeform, but list items can be nested.

; List items can contain nested content
[
  (list_item)
] @indent

; Fenced code blocks should preserve their internal indentation
(fenced_code_block) @indent.ignore
