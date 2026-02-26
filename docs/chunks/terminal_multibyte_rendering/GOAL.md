---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/Cargo.toml
- crates/editor/src/font.rs
- crates/editor/src/glyph_atlas.rs
- crates/editor/src/glyph_buffer.rs
code_references:
  - ref: crates/editor/src/font.rs#Font::glyph_for_char
    implements: "Non-BMP character support via UTF-16 surrogate pairs - enables glyph lookup for characters above U+FFFF"
  - ref: crates/editor/src/font.rs#tests::test_non_bmp_characters_with_surrogate_pairs
    implements: "Test for non-BMP character surrogate pair handling"
  - ref: crates/editor/src/font.rs#tests::test_non_bmp_egyptian_hieroglyphs
    implements: "Test for Egyptian hieroglyph rendering (U+131DD, U+131DF, U+131DE)"
  - ref: crates/editor/src/glyph_atlas.rs#tests::test_non_bmp_character_handling
    implements: "Test for non-BMP character atlas integration"
  - ref: crates/editor/src/glyph_atlas.rs#tests::test_non_bmp_hieroglyphs
    implements: "Test for non-BMP hieroglyph atlas integration"
  - ref: crates/editor/src/glyph_buffer.rs
    implements: "Width-aware column positioning using unicode-width crate for CJK and wide characters"
  - ref: crates/editor/src/glyph_buffer.rs#tests::test_unicode_width_cjk_characters
    implements: "Test verifying CJK characters have width 2"
  - ref: crates/editor/src/glyph_buffer.rs#tests::test_span_width_calculation
    implements: "Test for width-aware span width calculation"
  - ref: crates/editor/src/glyph_buffer.rs#tests::test_column_advancement_simulation
    implements: "Test for correct column advancement with mixed-width characters"
  - ref: crates/editor/Cargo.toml
    implements: "Added unicode-width dependency for character display width calculation"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_flood_starvation
---

# Chunk Goal

## Minor Goal

Two confirmed rendering problems in the terminal pane:

### 1. Non-BMP characters render as blank spaces

Characters above U+FFFF (e.g., Egyptian hieroglyphs 𓆝 U+131DD, 𓆟 U+131DF, 𓆞 U+131DE, and emoji like 😀 U+1F600) cannot be rasterized. `font.rs:209` gates on `(c as u32) <= 0xFFFF` and casts to `u16` for the Core Text `glyphs_for_characters` API. Non-BMP characters return `None`, and `glyph_atlas.rs` falls back to rendering a space glyph. There is even a test (`test_non_bmp_characters_return_none`) that asserts this limitation.

The fix requires using Core Text APIs that support surrogate pairs or UTF-32 codepoints for glyph lookup, and widening the internal glyph ID type from `u16` to at least `u32` (Core Graphics uses `CGGlyph` which is `u16`, but the lookup path needs to handle non-BMP input).

### 2. Wide character width tracking is missing

The GPU rendering pipeline assumes every character occupies exactly one cell width. `glyph_buffer.rs:418` iterates with `chars().enumerate()`, assigning sequential column indices without consulting character display width. Wide characters (CJK, fullwidth symbols, some emoji) occupy two terminal cells, causing:

- **Glyph positioning drift** — characters after a wide character are placed one cell too far left
- **Misaligned highlight/selection quads** — background highlights span only one cell for two-cell characters
- **Cursor misplacement** — block cursor at wrong position when wide characters precede it

The terminal emulator layer (`style_convert.rs:137`) already correctly skips alacritty's `WIDE_CHAR_SPACER` cells, but this width information is not propagated to the rendering pipeline. The `unicode-width` crate is not in the dependency tree.

### 3. Checkmark rendering defect (possibly explained by #1 or #2)

A checkmark character after "total_savings" in terminal output renders incorrectly. The narrow ✓ (U+2713) renders fine elsewhere, so the producing program likely emits a different codepoint — possibly a non-BMP checkmark (falling into problem 1) or a wide variant (falling into problem 2). The specific codepoint should be identified during planning to confirm.

## Success Criteria

- Non-BMP characters (U+10000 and above) are rasterized and rendered instead of falling back to space glyphs
- Wide characters render at the correct 2-cell width in the terminal pane
- Characters following a wide character are positioned correctly (no overlap, no gap)
- Selection/highlight quads span the full display width of wide characters
- The block cursor renders at the correct column position when wide characters are present
- The checkmark rendering defect from the screenshot is resolved
- Existing ASCII and narrow Unicode rendering is not regressed
- The fix applies to terminal pane rendering (editor buffer rendering is out of scope for this chunk)