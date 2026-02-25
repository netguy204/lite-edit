<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix uses Core Text's built-in font fallback mechanism via `CTFontCreateForString` (Rust binding: `CTFont::with_string_and_attributes` or equivalent). When `font.glyph_for_char(c)` returns `None` (character not in Menlo), we ask Core Text which system font covers that string and rasterize from the fallback font instead.

**Key insight**: Core Text already does the heavy lifting for font fallback—Terminal.app, Safari, and iTerm2 all use this same API. We just need to wire it into the existing glyph atlas machinery.

**Architecture changes**:

1. **font.rs**: Add `Font::find_fallback_font(c: char) -> Option<CFRetained<CTFont>>` that calls Core Text's fallback API
2. **font.rs**: Add `Font::glyph_for_char_with_fallback(c: char) -> Option<(u16, Option<CFRetained<CTFont>>)>` that returns both glyph ID and (optionally) the fallback font
3. **glyph_atlas.rs**: Modify `add_glyph` to accept an optional fallback font for rasterization
4. **glyph_atlas.rs**: Store a "fallback glyph" marker in the atlas (replacement character U+FFFD) for characters with no glyph in any font

**Fallback chain**:
1. Try primary font (Menlo) → glyph found → rasterize from Menlo
2. Try Core Text fallback → fallback font found → rasterize from fallback
3. No fallback → render replacement character U+FFFD (which itself may need fallback lookup)
4. U+FFFD not available → render a visible placeholder (dotted box or solid square)

**Performance considerations**:
- Fallback font lookups are cached per-character in the glyph atlas (lookup only on first miss)
- ASCII characters are pre-populated from Menlo at startup (no change)
- The common path (Menlo has the glyph) adds zero overhead

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS font fallback within the renderer subsystem's glyph atlas machinery. The subsystem's **Atlas Availability** invariant is preserved: all glyphs (including fallback-rasterized ones) are added to the atlas before rendering. No deviation from subsystem patterns expected.

## Sequence

### Step 1: Add `find_fallback_font` method to Font

Add a method to `font.rs` that uses Core Text's `CTFontCreateForString` to find a fallback font for a given character.

```rust
// Chunk: docs/chunks/font_fallback_rendering - Core Text fallback font lookup
pub fn find_fallback_font(&self, c: char) -> Option<CFRetained<CTFont>>
```

The method:
1. Creates a `CFString` containing the single character
2. Calls `CTFont::create_for_string(&self.ct_font, &string, CFRange { location: 0, length: string.len() })`
3. If the returned font is different from `self.ct_font`, returns it; otherwise returns `None`

**Note**: The exact Rust binding name may differ (`create_for_string_and_attributes`, `with_string`, etc.)—verify in `objc2-core-text` docs.

Location: `crates/editor/src/font.rs`

### Step 2: Add `glyph_for_char_with_fallback` method to Font

Add a method that returns both the glyph ID and the font it came from:

```rust
// Chunk: docs/chunks/font_fallback_rendering - Glyph lookup with fallback
pub fn glyph_for_char_with_fallback(&self, c: char) -> Option<GlyphSource>

pub struct GlyphSource {
    pub glyph_id: u16,
    pub font: GlyphFont,
}

pub enum GlyphFont {
    Primary,
    Fallback(CFRetained<CTFont>),
}
```

Logic:
1. Try `glyph_for_char(c)` on primary font → if found, return `GlyphSource { glyph_id, font: Primary }`
2. Try `find_fallback_font(c)` → if found, call `glyphs_for_characters` on the fallback font
3. If fallback has glyph, return `GlyphSource { glyph_id, font: Fallback(fallback_font) }`
4. Otherwise return `None`

Location: `crates/editor/src/font.rs`

### Step 3: Add unit tests for fallback font lookup

Write tests verifying:
- Egyptian hieroglyphs (𓆝 U+131DD) trigger fallback lookup and return a glyph
- ASCII characters ('A') do NOT trigger fallback (use primary font)
- A character with no glyph in any system font returns `None`

These tests run against the live macOS font system—per TESTING_PHILOSOPHY.md, glyph rasterization itself is in the "humble" category, but we can test the lookup logic.

Location: `crates/editor/src/font.rs` (test module)

### Step 4: Modify `GlyphAtlas::add_glyph` to accept optional fallback font

Change signature from:
```rust
pub fn add_glyph(&mut self, font: &Font, c: char) -> bool
```

To:
```rust
pub fn add_glyph(&mut self, font: &Font, c: char, fallback: Option<&CTFont>) -> bool
```

The `rasterize_glyph` call will use `fallback.unwrap_or(font.ct_font())` for drawing.

**Alternative**: Store the fallback font reference inside the atlas for repeated use (more efficient if we're adding many glyphs from the same fallback). Evaluate during implementation.

Location: `crates/editor/src/glyph_atlas.rs`

### Step 5: Modify `GlyphAtlas::ensure_glyph` to use fallback path

Update `ensure_glyph` to:
1. Call `font.glyph_for_char_with_fallback(c)`
2. If `GlyphSource::Fallback(fb)`, call `add_glyph(font, c, Some(&fb))`
3. If `None`, fall through to replacement character handling (Step 6)

The key change: instead of falling back to space, we now attempt fallback font lookup first.

Location: `crates/editor/src/glyph_atlas.rs`

### Step 6: Add replacement character rendering for truly missing glyphs

For characters with no glyph in any system font:
1. First attempt: Render U+FFFD (REPLACEMENT CHARACTER) via the same fallback path
2. Second attempt: If U+FFFD also fails, render a visible placeholder—a filled rectangle using the solid glyph UV

Add a private method:
```rust
fn add_replacement_glyph(&mut self, font: &Font) -> &GlyphInfo
```

This ensures characters with no rendering are never invisible.

Location: `crates/editor/src/glyph_atlas.rs`

### Step 7: Add integration tests for fallback glyph caching

Write tests verifying:
- Fallback glyphs are cached in the atlas (second lookup returns same UV coords)
- Fallback glyphs have valid dimensions
- Replacement character is rendered for unmapped codepoints

These require Metal device access (integration test category).

Location: `crates/editor/src/glyph_atlas.rs` (test module)

### Step 8: Manual visual verification

Open the editor with a terminal session and output:
- Egyptian hieroglyphs: `echo '𓆝 𓆟 𓆞'`
- Emoji: `echo '😀 🎉 👨‍👩‍👧‍👦'`
- Mathematical symbols: `echo '∫ ∑ √ ∞'`

Verify:
- Characters render visibly (not blank spaces)
- Characters are positioned correctly (no overlap/drift)
- ASCII text before/after is unaffected

This is the "humble view" verification per TESTING_PHILOSOPHY.md—visual output cannot be unit-tested.

### Step 9: Verify performance (no regression for ASCII)

Run the editor with a large ASCII file and confirm:
- Rendering latency is not regressed
- No noticeable stutter when scrolling

The fallback path should only fire on cache miss for non-Menlo characters; ASCII should hit the pre-populated atlas directly.

## Dependencies

- **terminal_multibyte_rendering**: Already complete (ACTIVE). This chunk built the UTF-16 surrogate pair handling in `font.rs` that we extend.
- **objc2-core-text**: Already in Cargo.toml. The `CTFont::create_for_string` (or equivalent) binding must be available. If not exposed, we may need to use raw FFI to call `CTFontCreateForString` directly.

No new crate dependencies are needed—Core Text's fallback mechanism is already available through `objc2-core-text`.

## Risks and Open Questions

1. **objc2-core-text binding availability**: The exact binding name for `CTFontCreateForString` in `objc2-core-text` is uncertain. If not available, we may need to add raw FFI:
   ```rust
   extern "C" {
       fn CTFontCreateForString(
           currentFont: *const c_void,
           string: *const c_void,
           range: CFRange
       ) -> *mut c_void;
   }
   ```
   **Mitigation**: Check `objc2-core-text` source/docs first; fall back to FFI if necessary.

2. **Fallback font metrics mismatch**: The fallback font may have different advance width, ascent, or descent than Menlo. This could cause visual inconsistencies.
   **Mitigation**: Continue using Menlo's cell dimensions for atlas storage; the glyph may be slightly clipped or padded but will be positioned correctly.

3. **Fallback font caching**: If we create a new `CTFont` reference for each fallback glyph, we may leak memory or create unnecessary font instances.
   **Mitigation**: Consider caching fallback fonts by typeface name. Evaluate if needed based on memory profiling.

4. **Edge case: U+FFFD not in any font**: On a vanilla macOS system, U+FFFD should always be available. But in pathological cases, we need a last-resort placeholder.
   **Mitigation**: Use the solid white glyph (already in atlas at `\x01`) with a visible color as ultimate fallback.

5. **Wide character fallback**: Some fallback glyphs (emoji, CJK) may be wider than Menlo's advance width. The `terminal_multibyte_rendering` chunk already handles width tracking via `unicode-width`; fallback glyphs will be positioned using the same mechanism.
   **Mitigation**: Verify in Step 8 visual testing that wide fallback glyphs render at correct 2-cell width.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->