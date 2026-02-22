---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/glyph_buffer.rs
  - crates/terminal/src/style_convert.rs
  - crates/editor/src/color_palette.rs
  - crates/terminal/tests/integration.rs
code_references:
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_wrap
    implements: "Per-span foreground colors, background quads, and underline quads in wrapped rendering path"
  - ref: crates/terminal/src/style_convert.rs#cell_to_style
    implements: "Terminal cell to Style conversion with proper DIM flag detection"
  - ref: crates/terminal/src/style_convert.rs#row_to_styled_line
    implements: "Row-to-StyledLine conversion preserving per-span styles across span boundaries"
  - ref: crates/editor/src/color_palette.rs#ColorPalette::resolve_style_colors
    implements: "Style to RGBA resolution including inverse and dim transformations"
  - ref: crates/terminal/tests/integration.rs
    implements: "Integration tests for styled terminal output (ANSI colors, indexed colors, RGB, attributes)"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- terminal_input_render_bug
---

# Chunk Goal

## Minor Goal

Terminal styling (colors, bold, inverse, dim, etc.) is not visible when running programs in the embedded terminal. Apps like Vim with syntax highlighting and TUI apps like Pi render without colors or shading — everything appears as unstyled monochrome text.

The PTY already advertises full color support (`TERM=xterm-256color`, `COLORTERM=truecolor` in `crates/terminal/src/pty.rs:75-76`), so applications are emitting color escape sequences. The color pipeline also exists end-to-end: `style_convert.rs` converts alacritty cell attributes to `Style`, and `ColorPalette` resolves `Style` colors to RGBA. The problem is somewhere between these conversions and what actually reaches the GPU — styles are being computed but not rendered visibly.

This chunk will diagnose and fix the rendering pipeline so that terminal styling is faithfully displayed. This likely involves the glyph buffer's style application path (`GlyphBuffer::update_from_buffer_with_cursor`), the Metal shader's per-vertex color handling, or the connection between `ColorPalette::resolve_style_colors` and the actual vertex data written for terminal content.

## Success Criteria

- ANSI colors (16 named, 256 indexed, RGB truecolor) are visually rendered in terminal output
- Bold, italic, dim, inverse, underline, and strikethrough attributes produce visible styling differences
- Running `vim` with syntax highlighting shows colored source code
- Running a TUI app (e.g., Pi, htop) shows colors and UI shading
- The fix does not regress text buffer (editor) rendering

