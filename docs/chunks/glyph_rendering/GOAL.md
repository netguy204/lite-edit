---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/Cargo.toml
  - crates/editor/build.rs
  - crates/editor/src/main.rs
  - crates/editor/src/renderer/mod.rs
  - crates/editor/src/renderer/content.rs
  - crates/editor/src/font.rs
  - crates/editor/src/glyph_atlas.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/shader.rs
  - crates/editor/shaders/glyph.metal
  - crates/editor/tests/smoke_test.rs
code_references:
  - ref: crates/editor/src/font.rs#Font
    implements: "Core Text font loading and metrics extraction (advance width, line height, ascent, descent)"
  - ref: crates/editor/src/font.rs#FontMetrics
    implements: "Font metrics data structure used for layout calculations"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas
    implements: "Metal texture atlas with on-demand glyph rasterization via Core Text"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphInfo
    implements: "UV coordinates and metrics for individual glyphs in the atlas"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer
    implements: "Vertex and index buffer management for glyph quads"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphLayout
    implements: "Pure monospace layout calculation (x = col * width, y = row * height)"
  - ref: crates/editor/src/shader.rs#GlyphPipeline
    implements: "Metal render pipeline with alpha blending for anti-aliased glyphs"
  - ref: crates/editor/shaders/glyph.metal
    implements: "Vertex/fragment shaders for textured quad rendering with orthographic projection"
  - ref: crates/editor/src/renderer/content.rs#Renderer::render_text
    implements: "Text rendering integration - binds atlas, buffers, uniforms and issues draw call"
  - ref: crates/editor/src/renderer/content.rs#Renderer::set_content
    implements: "API for updating displayed text content"
narrative: null
investigation: editor_core_architecture
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- metal_surface
created_after: []
---

# Monospace Glyph Atlas + Text Rendering

## Minor Goal

Build the text rendering pipeline that turns strings into visible glyphs on the Metal surface. This is the visual half of the render loop described in the investigation — given text content, produce the GPU commands to display it.

The pipeline has three parts: (1) load a monospace font via Core Text, extracting glyph metrics (advance width, line height, ascent/descent), (2) rasterize glyphs on demand into a texture atlas stored in a Metal texture, and (3) build a vertex buffer of textured quads and render them with a Metal shader.

Layout is trivial for monospace: `x = col * glyph_width`, `y = row * line_height`. No complex text shaping, no ligatures, no bidirectional text. This is a code editor — fixed-width ASCII rendering is the critical path.

This chunk renders hardcoded multi-line text to prove the pipeline works. Connecting it to a live text buffer happens in chunk 4 (viewport_rendering).

## Success Criteria

- A monospace font is loaded via Core Text (hardcoded font name is fine, e.g., "Menlo" or "SF Mono"). Glyph metrics (advance width, line height, ascent, descent) are extracted correctly.
- A glyph atlas (Metal texture) is populated on demand as new characters are encountered. At minimum: all printable ASCII characters (0x20-0x7E).
- A Metal shader renders textured quads from the atlas. Each glyph is a screen-aligned quad with correct UV coordinates into the atlas.
- The window from `metal_surface` now displays at least 20 lines of hardcoded multi-line text, rendered in the monospace font, with correct character spacing and line spacing.
- Text is legible at the default font size and at Retina (2x) scale. The atlas and rendering account for display scale factor.
- Glyphs render with correct anti-aliasing (Core Text's native rasterization, not custom AA).
- Background color behind text is the same editor background from `metal_surface` (no visual seams or artifacts).
- Rendering 50 lines × 120 columns (~6,000 glyphs) completes in under 2ms total (layout + GPU submission), measured on any Apple Silicon Mac. This validates the H3 finding that full viewport redraws are well within the 8ms budget.