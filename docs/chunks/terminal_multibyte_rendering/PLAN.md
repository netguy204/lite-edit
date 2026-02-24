<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk addresses two distinct but related rendering defects in the terminal pane:

**Problem 1: Non-BMP Character Support (U+10000+)**
The current `font.rs:glyph_for_char()` rejects characters above U+FFFF because Core Text's `glyphs_for_characters` API takes `u16` (UTF-16 code units). Non-BMP characters require surrogate pairs. The fix is to:
1. Use `CFString` from the character, which handles Unicode correctly
2. Use `CTFontGetGlyphsForCharacters` via the CFString approach, or
3. Use `CTFontCreateForString` + `CTFontGetGlyphsForCharacters` pattern for proper fallback

However, there's a simpler approach: Core Text's `CTFont::glyphs_for_characters` can accept surrogate pairs when given two `u16` values representing the high and low surrogates. We'll implement this by:
- Detecting non-BMP characters (`c > '\u{FFFF}'`)
- Computing the surrogate pair
- Passing both surrogates to `glyphs_for_characters` with count=2
- Using the resulting glyph ID

**Problem 2: Wide Character Width Tracking**
The rendering pipeline assumes all characters occupy 1 cell. The terminal emulator layer (`style_convert.rs`) correctly skips `WIDE_CHAR_SPACER` cells, but this information isn't propagated to the glyph buffer. The fix is to:
1. Add the `unicode-width` crate to track display widths
2. Modify `glyph_buffer.rs` to advance column position by character width (1 or 2)
3. Ensure selection/highlight quads also respect character widths

**Architecture Alignment**
Both fixes follow the existing humble view architecture:
- Font/atlas modifications affect glyph rasterization (testable with Metal device)
- Width tracking affects vertex buffer construction (testable via `GlyphLayout` calculations)
- Terminal cell processing already handles wide chars; we just need to propagate width info

**Testing Strategy**
Per TESTING_PHILOSOPHY.md:
- Unit tests for `glyph_for_char()` with non-BMP characters
- Unit tests for width calculation in `GlyphLayout`
- Integration tests verifying correct column advancement for wide characters
- The actual rendering is humble view code (not unit tested, verified visually)

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS glyph rendering improvements within the renderer subsystem. Specifically:
  - `GlyphAtlas` - extends to handle non-BMP characters via surrogate pair support in `Font::glyph_for_char()`
  - `GlyphBuffer` - adds character width tracking for proper wide character positioning

  The work aligns with the subsystem's invariant "Atlas Availability" - we're ensuring non-BMP glyphs can be added to the atlas before rendering. The "Screen-Space Consistency" invariant is maintained by properly tracking character widths in column positioning.

## Sequence

### Step 1: Add unicode-width dependency

Add the `unicode-width` crate to `crates/editor/Cargo.toml`. This crate provides `UnicodeWidthChar::width()` for determining display width (0, 1, or 2 cells) of Unicode characters.

Location: `crates/editor/Cargo.toml`

### Step 2: Implement non-BMP character support in Font

Extend `Font::glyph_for_char()` to handle characters above U+FFFF using UTF-16 surrogate pairs:

1. For `c > '\u{FFFF}'`:
   - Compute the high surrogate: `((code - 0x10000) >> 10) + 0xD800`
   - Compute the low surrogate: `((code - 0x10000) & 0x3FF) + 0xDC00`
   - Pass both surrogates to `glyphs_for_characters` with count=2
   - Return the glyph ID if successful

2. Update the test `test_non_bmp_characters_return_none` to verify non-BMP characters now return `Some(glyph_id)` when the font supports them, or `None` if the font lacks coverage.

Location: `crates/editor/src/font.rs`

### Step 3: Update GlyphAtlas to handle non-BMP glyph IDs

The `add_glyph` and `rasterize_glyph` methods currently take `u16` glyph IDs. For non-BMP characters, Core Graphics `CGGlyph` is still `u16`, so the glyph ID type doesn't need to change. The fix in Step 2 handles the lookup; the rasterization path remains the same.

Verify that the existing `ensure_glyph` flow works correctly with non-BMP characters by testing:
- Emoji characters (U+1F600 - grinning face)
- Egyptian hieroglyphs (U+131DD, U+131DF)

Location: `crates/editor/src/glyph_atlas.rs` (add tests, minimal code changes expected)

### Step 4: Add width-aware glyph positioning in GlyphBuffer

Modify the glyph quad emission in `update_from_buffer_with_cursor` to advance column position by character display width:

1. Import `unicode_width::UnicodeWidthChar`
2. In Phase 3 (Glyph Quads), after emitting a glyph quad:
   - Use `c.width().unwrap_or(1)` to get the display width
   - Advance `col` by the width instead of always `1`

This affects:
- Glyph positioning (characters after wide chars shift right correctly)
- Background quad positioning (Phase 1)
- Underline quad positioning (Phase 4)

Location: `crates/editor/src/glyph_buffer.rs`

### Step 5: Update background and underline phases for width tracking

The background (Phase 1) and underline (Phase 4) phases iterate by span, counting `span.text.chars().count()` for column advancement. This needs to use width-aware counting:

1. Replace `span.text.chars().count()` with `span.text.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>()`
2. This ensures background highlights span the correct number of cells for wide characters

Location: `crates/editor/src/glyph_buffer.rs`

### Step 6: Verify selection quad positioning respects width

Selection rendering (Phase 2) uses `sel_start.col` and `sel_end.col` positions from `BufferView::selection_range()`. The terminal buffer already tracks selection in terminal grid coordinates (which account for wide characters via the spacer cell mechanism).

Verify that selection highlighting works correctly for wide characters:
- Selection should span 2 cells visually for a wide character
- The existing `WIDE_CHAR_SPACER` skip in `style_convert.rs` should already handle this

Location: `crates/editor/src/glyph_buffer.rs`, `crates/terminal/src/style_convert.rs` (verification, likely no changes needed)

### Step 7: Identify and document the checkmark character

Investigate the checkmark rendering defect from the screenshot. The narrow ✓ (U+2713) renders fine, so the issue is likely:
- A different checkmark variant (heavy checkmark ✔ U+2714, ballot box ☑ U+2611, etc.)
- A non-BMP checkmark or emoji variant (✅ U+2705 is emoji)

If the checkmark is a non-BMP character, Step 2 fixes it. If it's a BMP character with rendering issues, document the finding and verify the fix.

Location: Investigation/documentation in this PLAN.md

### Step 8: Add unit tests for width-aware layout

Add tests to verify:
1. `glyph_for_char()` returns valid glyph IDs for supported non-BMP characters
2. `GlyphLayout` positioning calculations account for character widths
3. Column advancement in `update_from_buffer_with_cursor` is width-aware

Tests should use CJK characters (e.g., '中' U+4E2D, width=2) and emoji where font coverage permits.

Location: `crates/editor/src/font.rs`, `crates/editor/src/glyph_buffer.rs`

### Step 9: Update existing tests

Update or remove tests that assert the old behavior:
- `test_non_bmp_characters_return_none` - Update to verify non-BMP support
- `test_non_bmp_character_falls_back_to_space` in `glyph_atlas.rs` - Update to verify proper rendering

Location: `crates/editor/src/font.rs`, `crates/editor/src/glyph_atlas.rs`

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments:

```rust
// Chunk: docs/chunks/terminal_multibyte_rendering - Non-BMP character glyph lookup
// Chunk: docs/chunks/terminal_multibyte_rendering - Width-aware column positioning
```

## Dependencies

- **External crate**: `unicode-width` - for determining character display widths
- **Font coverage**: The system font (Menlo, Intel One Mono) must have glyphs for the test characters. Non-BMP characters without font coverage will still fall back to space glyph, but the lookup path will be correct.

## Risks and Open Questions

1. **Font coverage for non-BMP characters**: Most monospace fonts (Menlo, Intel One Mono) may not include glyphs for Egyptian hieroglyphs or many emoji. The fix ensures the *lookup path* is correct; actual rendering depends on font coverage. Font fallback (using a different font for missing glyphs) is out of scope.

2. **Performance of width lookup**: `UnicodeWidthChar::width()` is a table lookup, so performance impact should be minimal. However, it's called per-character in the hot rendering path. If profiling shows issues, we can cache widths per span.

3. **Surrogate pair handling edge cases**: The surrogate pair calculation must be correct for all valid non-BMP codepoints. The standard formulas are well-established, but careful testing with edge cases (U+10000, U+10FFFF) is needed.

4. **Interaction with terminal WIDE_CHAR_SPACER**: The terminal layer already skips spacer cells. We need to verify that width-aware positioning in the renderer doesn't double-count (once from spacer skip, once from width calculation). The current `row_to_styled_line` skips spacers but produces text with the actual characters, so width calculation should be correct.

5. **Selection behavior on wide characters**: When selecting text containing wide characters, does the selection column index refer to the start of the wide character or the spacer cell? Need to verify behavior matches expectations.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->