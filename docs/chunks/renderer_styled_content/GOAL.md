---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/color_palette.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/shader.rs
  - crates/editor/src/lib.rs
  - crates/editor/shaders/glyph.metal
code_references:
  - ref: crates/editor/src/color_palette.rs#ColorPalette
    implements: "Color resolution for Style attributes (Named, Indexed, RGB colors) with Catppuccin Mocha theme"
  - ref: crates/editor/src/color_palette.rs#ColorPalette::resolve_style_colors
    implements: "Foreground/background color resolution with inverse and dim style transformations"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphVertex
    implements: "Per-vertex color field in vertex structure for styled text rendering"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer
    implements: "Extended quad ranges for background, underline, and styled rendering phases"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Multi-phase rendering: background quads, per-span foreground colors, underlines, cursor shapes"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::create_cursor_quad_for_shape
    implements: "Cursor shape rendering (Block, Beam, Underline) based on CursorInfo"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::create_underline_quad
    implements: "Underline quad generation for underlined text spans"
  - ref: crates/editor/src/shader.rs#VERTEX_SIZE
    implements: "Updated vertex size (32 bytes) to accommodate per-vertex color"
  - ref: crates/editor/src/shader.rs#create_vertex_descriptor
    implements: "Vertex descriptor with color attribute at offset 16"
  - ref: crates/editor/shaders/glyph.metal
    implements: "Metal shader with per-vertex color input for styled text and cursor rendering"
  - ref: crates/editor/src/renderer.rs#Renderer::render_text
    implements: "Multi-pass rendering with per-vertex colors for background, selection, glyphs, underlines, cursor"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- buffer_view_trait
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Enhance the Metal rendering pipeline to render the `Style` attributes from the `BufferView` trait. The current renderer draws white glyphs on a uniform dark background. This chunk adds:

1. **Per-cell background colors**: A background rect rendering pass before the glyph pass. Each span's `bg` color is drawn as a filled rectangle behind the text. This requires either a separate "colored quad" shader or extending the existing shader to handle untextured rectangles.

2. **Cursor shape rendering**: Currently the cursor is a block highlight. Extend to support Block (filled rectangle), Beam (thin vertical line at left edge of cell), and Underline (thin horizontal line at bottom of cell) shapes, matching the `CursorInfo` from `BufferView`.

3. **Per-span foreground colors**: The current renderer uses a single `TEXT_COLOR` uniform. Extend to pass per-vertex or per-span color so different spans can have different foreground colors.

4. **Underline rendering**: Draw underlines beneath text spans. Start with single underline (horizontal line at baseline). Stretch goal: support double, curly, dotted, dashed variants.

5. **Inverse video and dim**: When `Style::inverse` is set, swap fg/bg at render time. When `Style::dim` is set, reduce foreground alpha or brightness.

These enhancements are prerequisites for terminal display but also benefit syntax-highlighted file editing — once the renderer can handle per-span colors, syntax highlighting becomes a matter of producing the right `Style` values in `BufferView::styled_line()`.

## Success Criteria

- Renderer reads `Style` attributes from `BufferView::styled_line()` and renders them visually
- Per-cell background colors are visible: a line with mixed bg colors shows distinct colored rectangles behind each span
- Foreground colors work: different spans on the same line can have different text colors
- Cursor renders correctly in Block, Beam, and Underline shapes based on `CursorInfo::shape`
- Inverse video works: a span with `inverse: true` shows fg/bg swapped
- Dim works: a span with `dim: true` renders with reduced intensity
- Single underline renders beneath text for spans with `underline: Single`
- Rendering performance stays within budget: full screen redraw (40 lines × 120 cols) completes within 2ms
- Existing demo text still renders correctly (default styles produce identical visual output)