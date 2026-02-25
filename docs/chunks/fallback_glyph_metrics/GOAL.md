---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/font.rs
- crates/editor/src/glyph_atlas.rs
code_references:
  - ref: crates/editor/src/font.rs#Font::get_ct_font_metrics
    implements: "Extract ascent/descent/line_height from any CTFont for fallback metric queries"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::rasterize_glyph_with_ct_font
    implements: "Scale and center fallback glyphs when font line_height exceeds cell bounds"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::add_glyph_with_source
    implements: "Query fallback font metrics and pass to rasterization"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::rasterize_glyph
    implements: "Refactored to call rasterize_glyph_with_ct_font with primary font metrics"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on:
- font_fallback_rendering
created_after:
- font_fallback_rendering
---

# Chunk Goal

## Minor Goal

The `font_fallback_rendering` chunk made previously invisible characters visible by querying Core Text for fallback fonts. However, fallback glyphs are rasterized into a fixed-size bitmap cell derived from the **primary font's metrics** (Menlo), and positioned using Menlo's descent as the baseline. When a fallback font has different vertical metrics — taller ascent, different descent, larger overall em — the glyph overflows the bitmap context and Core Graphics silently clips it.

This is visible in practice: powerline symbols, nerd font icons, and other fallback-rendered glyphs appear with their tops or bottoms cut off compared to the same characters in macOS Terminal.app.

The specific code path:

1. `GlyphAtlas::new()` computes `cell_height` from `font.metrics.line_height` (Menlo's) — **all glyphs share this fixed cell size**
2. `add_glyph_with_source()` passes `font.metrics.descent` (Menlo's) as the baseline for fallback glyphs
3. `rasterize_glyph_with_ct_font()` creates a `CGBitmapContext` of exactly `cell_width × cell_height` pixels and draws at `y = descent` — any glyph content extending beyond the bitmap bounds is silently discarded

The fix: when rasterizing a fallback glyph, use the **fallback font's own metrics** to determine baseline positioning, and **scale the glyph down** if it would exceed the cell bounds. This ensures the full glyph is visible while maintaining the grid-aligned cell layout.

## Success Criteria

- Fallback glyphs render without clipping — the full glyph outline is visible within the cell
- Fallback glyphs are vertically centered or baseline-aligned appropriately within the primary font's cell height
- Powerline/nerd font symbols (e.g., branch icon , separator arrows ) render correctly when the terminal uses a patched font that falls back through Core Text
- No regression for primary font (Menlo) glyphs — they continue to use Menlo's own metrics
- No regression for fallback glyphs that already fit within the cell (e.g., glyphs from fonts with similar metrics to Menlo)
- The glyph atlas cell size remains uniform (no variable-height cells) to preserve the grid layout