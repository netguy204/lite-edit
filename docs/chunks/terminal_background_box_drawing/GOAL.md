---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/glyph_atlas.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/font.rs
  - crates/terminal/tests/integration.rs
code_references:
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::ensure_glyph
    implements: "On-demand glyph rasterization - adds non-ASCII characters to atlas when first encountered, with fallback to space for missing glyphs"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update
    implements: "Text buffer rendering with mutable atlas for on-demand glyph addition"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Viewport-aware rendering with on-demand glyph addition and background quad emission for non-default bg colors"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_wrap
    implements: "Wrapped text rendering with on-demand glyph addition and per-span background quads"
  - ref: crates/editor/src/font.rs#Font::glyph_for_char
    implements: "Character-to-glyph mapping for BMP characters, returns None for non-BMP (enabling fallback)"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- scroll_bottom_deadzone_v3
- terminal_styling_fidelity
---

# Chunk Goal

## Minor Goal

Two rendering gaps remain after the `terminal_styling_fidelity` chunk landed foreground colors and text attributes:

1. **Background colors not reaching the renderer.** Terminal programs emit background color escape sequences (e.g., ANSI `\x1b[44m` for blue background, 256-color, and truecolor RGB backgrounds), and `style_convert.rs` converts them into `Style` objects with background color data. However, the rendering pipeline does not produce visible background quads — all terminal content renders against the default editor background. TUI apps like Pi, htop, and Vim with colorschemes rely on per-cell background colors to draw status bars, highlighted lines, selection regions, and UI chrome.

2. **Box-drawing and line-drawing glyphs not rendering.** Unicode box-drawing characters (U+2500–U+257F) and other special glyphs used by TUI frameworks to draw borders, horizontal rules, and panels either render as missing-glyph placeholders or are absent entirely. These characters are essential for TUI applications — without them, borders appear broken and layouts are illegible.

Both issues likely trace to the glyph buffer and atlas pipeline: background quads may not be emitted for cells with non-default background colors, and the glyph atlas may not be rasterizing glyphs outside the basic ASCII range (0x20–0x7E) that the `glyph_rendering` chunk originally targeted.

## Success Criteria

- Per-cell background colors from terminal output are rendered as filled quads behind the glyph for that cell
- ANSI 16-color, 256-color indexed, and 24-bit truecolor backgrounds all produce visible colored rectangles
- Running a TUI app (e.g., htop, Pi) shows colored status bars, highlighted rows, and panel backgrounds
- Vim with a colorscheme shows line-highlight and visual-selection backgrounds
- Unicode box-drawing characters (U+2500–U+257F) render correctly — TUI borders and horizontal rules appear as connected lines, not missing-glyph boxes
- Other common TUI glyphs (block elements U+2580–U+259F, powerline symbols) render when present in the font
- The glyph atlas accommodates non-ASCII codepoints on demand without performance regression
- Existing ASCII glyph rendering and foreground color styling are not regressed