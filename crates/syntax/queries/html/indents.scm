; Chunk: docs/chunks/treesitter_indent - HTML indent queries
;
; Tree-sitter indent query for HTML.

; Elements indent their children
(element) @indent

; Self-closing elements don't need indent
(self_closing_tag) @outdent

; End tags outdent
(end_tag) @outdent
