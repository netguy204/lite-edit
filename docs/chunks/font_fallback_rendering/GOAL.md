---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/font.rs
- crates/editor/src/glyph_atlas.rs
- crates/editor/src/glyph_buffer.rs
code_references:
  - ref: crates/editor/src/font.rs#GlyphFont
    implements: "Enum distinguishing primary font from fallback font for glyph source tracking"
  - ref: crates/editor/src/font.rs#GlyphSource
    implements: "Result type for fallback-aware glyph lookup containing glyph ID and font source"
  - ref: crates/editor/src/font.rs#Font::find_fallback_font
    implements: "Core Text CTFontCreateForString wrapper to find fallback fonts for missing glyphs"
  - ref: crates/editor/src/font.rs#Font::glyph_for_char_with_fallback
    implements: "Main entry point for fallback-aware glyph lookup (primary → fallback → None)"
  - ref: crates/editor/src/font.rs#Font::glyph_id_from_font
    implements: "Helper to extract glyph ID from any CTFont (primary or fallback)"
  - ref: crates/editor/src/font.rs#Font::fallback_font_has_glyph
    implements: "Verifies fallback font actually contains the requested glyph"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::add_glyph_with_source
    implements: "Rasterizes and caches glyphs from primary or fallback fonts into atlas"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::rasterize_glyph_with_ct_font
    implements: "Rasterizes glyph from any CTFont with consistent baseline positioning"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::ensure_glyph
    implements: "Font fallback chain: primary → fallback → replacement character → solid placeholder"
  - ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas::ensure_replacement_glyph
    implements: "Ultimate fallback rendering U+FFFD or solid glyph for truly missing characters"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_multibyte_rendering
- terminal_pane_initial_sizing
---

# Chunk Goal

## Minor Goal

Characters not present in Menlo (the editor's primary font) are invisible. The `terminal_multibyte_rendering` chunk solved non-BMP glyph lookup and width tracking, but when a character has no glyph in Menlo, `glyph_atlas.rs:ensure_glyph` silently falls back to the space glyph — producing a correctly-positioned but completely invisible character.

This affects Egyptian hieroglyphs (𓆝 U+1319D, 𓆟 U+1319F, 𓆞 U+1319E), many emoji, mathematical symbols, and any other characters outside Menlo's coverage.

The fix is to implement **font fallback**: when the primary font lacks a glyph, query macOS Core Text for a fallback font that does contain it, rasterize from the fallback font, and render the result at the correct position. macOS provides `CTFontCreateForString` (or the equivalent `CTFont` method) which returns a font that covers a given string — this is the standard mechanism used by Terminal.app, iTerm2, and other macOS text renderers.

The key code path today:

1. `font.rs:glyph_for_char('𓆞')` → Core Text returns glyph 0 (not in Menlo) → returns `None`
2. `glyph_atlas.rs:add_glyph` → returns `false`
3. `glyph_atlas.rs:ensure_glyph` → falls back to `self.glyphs.get(&' ')` → returns invisible space glyph
4. `glyph_buffer.rs` → renders a quad sampling from the empty space texture region → invisible

After this chunk, step 1 failure should trigger a fallback font lookup, and the character should be rasterized from whichever system font covers it.

## Success Criteria

- Characters not in Menlo (e.g., Egyptian hieroglyphs 𓆝 𓆟 𓆞) render visibly using a fallback font
- The fallback font is selected via Core Text's built-in font matching (not a hardcoded list)
- Fallback glyphs are cached in the glyph atlas to avoid repeated font lookups
- Fallback glyphs are positioned correctly respecting their unicode-width
- Characters that exist in Menlo are unaffected (no regression)
- Characters with no glyph in any system font render a visible placeholder (e.g., U+FFFD replacement character or a dotted box) rather than being invisible
- Rendering latency is not regressed for common ASCII text (fallback path only fires on cache miss for non-Menlo characters)