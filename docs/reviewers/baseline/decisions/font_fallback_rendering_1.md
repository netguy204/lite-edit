---
decision: APPROVE
summary: All success criteria satisfied; implementation uses Core Text's built-in font matching, caches fallback glyphs in atlas, and handles missing glyphs with visible replacement characters.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Characters not in Menlo (e.g., Egyptian hieroglyphs 𓆝 𓆟 𓆞) render visibly using a fallback font

- **Status**: satisfied
- **Evidence**: `font.rs:find_fallback_font()` (lines 288-319) uses Core Text's `CTFont::for_string()` to find fonts that cover missing characters. Test `test_fallback_hieroglyphs_use_fallback_font` verifies hieroglyphs (U+131DD, U+131DF, U+131DE) are found via fallback. The test output shows "Hieroglyph '𓆝' (U+1319D) found via fallback, glyph ID: 417".

### Criterion 2: The fallback font is selected via Core Text's built-in font matching (not a hardcoded list)

- **Status**: satisfied
- **Evidence**: `font.rs:find_fallback_font()` (line 302) calls `self.ct_font.for_string(&cf_string, range)` which maps to `CTFontCreateForString` - the standard macOS system font matching API. No hardcoded font names are used.

### Criterion 3: Fallback glyphs are cached in the glyph atlas to avoid repeated font lookups

- **Status**: satisfied
- **Evidence**: `glyph_atlas.rs:ensure_glyph()` first checks `if self.glyphs.contains_key(&c)` before doing any lookup. `add_glyph_with_source()` inserts glyphs via `self.glyphs.insert(c, info)`. Test `test_fallback_glyphs_cached_in_atlas` verifies second lookup returns same UV coordinates as first.

### Criterion 4: Fallback glyphs are positioned correctly respecting their unicode-width

- **Status**: satisfied
- **Evidence**: `glyph_atlas.rs:add_glyph_with_source()` (lines 293-300) uses `rasterize_glyph_with_ct_font()` with the *primary font's descent* for baseline alignment (`font.metrics.descent`). Width is controlled by `glyph_buffer.rs` which continues to use `unicode_width::UnicodeWidthChar` as established in `terminal_multibyte_rendering` (verified by `ensure_glyph` calls at lines 434, 747, 1568 that integrate with existing width-aware rendering).

### Criterion 5: Characters that exist in Menlo are unaffected (no regression)

- **Status**: satisfied
- **Evidence**: `font.rs:glyph_for_char_with_fallback()` (lines 328-335) tries the primary font first: `if let Some(glyph_id) = self.glyph_for_char(c) { return GlyphSource { font: Primary } }`. Test `test_fallback_ascii_uses_primary_font` verifies ASCII 'A' uses primary font. Test `test_ascii_uses_primary_font_not_fallback` in glyph_atlas confirms ASCII characters are pre-populated from Menlo. All 18 font tests and 13 glyph_atlas tests pass.

### Criterion 6: Characters with no glyph in any system font render a visible placeholder (e.g., U+FFFD replacement character or a dotted box) rather than being invisible

- **Status**: satisfied
- **Evidence**: `glyph_atlas.rs:ensure_replacement_glyph()` (lines 550-572) implements the fallback chain: (1) try U+FFFD via fallback path, (2) if U+FFFD unavailable, return `self.solid_glyph()` - a fully opaque white cell. Test `test_replacement_character_rendered_for_unmapped_codepoints` verifies Private Use Area character U+F0000 returns a glyph with positive dimensions.

### Criterion 7: Rendering latency is not regressed for common ASCII text (fallback path only fires on cache miss for non-Menlo characters)

- **Status**: satisfied
- **Evidence**: ASCII glyphs are pre-populated in `GlyphAtlas::new()` (lines 122-125: `for c in ' '..='~' { atlas.add_glyph(font, c); }`). `ensure_glyph()` returns early if `self.glyphs.contains_key(&c)` (line 527-529), never invoking `glyph_for_char_with_fallback()` for cached glyphs. The fallback path is only reached on cache miss.
