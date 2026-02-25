<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `font_fallback_rendering` chunk established the font fallback chain: primary font → Core Text fallback lookup → replacement character. However, fallback glyphs are currently rasterized into a bitmap sized for the **primary font's metrics** (Menlo), and positioned using Menlo's descent as the baseline.

When a fallback font has different vertical metrics (taller ascent, larger em-square), the glyph overflows the bitmap and Core Graphics silently clips it. This is visible with powerline symbols, nerd font icons, and other fallback glyphs that appear with tops/bottoms cut off.

**Root cause analysis:**

1. `GlyphAtlas::new()` computes `cell_height` from `font.metrics.line_height` (Menlo's) — all glyphs share this fixed cell size
2. `add_glyph_with_source()` passes `font.metrics.descent` (Menlo's) as the baseline for fallback glyphs to `rasterize_glyph_with_ct_font()`
3. `rasterize_glyph_with_ct_font()` creates a `CGBitmapContext` of exactly `cell_width × cell_height` and draws at `y = descent` — content beyond bounds is clipped

**Fix strategy: Query and adapt to fallback font metrics**

Rather than assuming all fonts share Menlo's metrics, the fix:

1. Queries the fallback font's own metrics (ascent, descent, line_height) when rasterizing
2. Computes the ratio between the fallback font's line_height and the primary font's line_height
3. If the fallback glyph would overflow, scales it down to fit within the cell bounds
4. Centers the scaled glyph vertically within the cell for visual balance

This preserves the fixed-size cell grid (required for the terminal's character grid layout) while ensuring the full glyph is visible.

**Alternative considered but rejected:** Variable cell heights per glyph would break the grid layout assumption. The grid is fundamental to terminal rendering where every character occupies exactly one cell width and one line height.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS part of the renderer subsystem's glyph atlas functionality. The chunk extends `GlyphAtlas` to handle fallback font metrics correctly. The subsystem's invariants are preserved:
  - **Atlas Availability**: Glyphs are still rasterized and cached before rendering
  - **Single Frame Contract**: No change to the render loop
  - **Screen-Space Consistency**: Cell dimensions remain uniform

## Sequence

### Step 1: Add helper to extract metrics from any CTFont

Add a function `get_font_metrics(ct_font: &CTFont) -> (f64, f64, f64)` in `font.rs` that extracts `(ascent, descent, line_height)` from any Core Text font. This is needed to query fallback font metrics.

Location: `crates/editor/src/font.rs`

**Test (TDD):**
```rust
#[test]
fn test_get_font_metrics_menlo() {
    let font = Font::new("Menlo-Regular", 14.0, 1.0);
    let (ascent, descent, line_height) = font.get_font_metrics(font.ct_font());
    assert!(ascent > 0.0);
    assert!(descent > 0.0);
    assert!((line_height - (ascent + descent)).abs() < 1.0); // May have leading
}
```

### Step 2: Modify `rasterize_glyph_with_ct_font` to accept font metrics

Change the signature of `rasterize_glyph_with_ct_font` to accept the fallback font's own metrics `(ascent, descent, line_height)` instead of just `descent`. This is a refactoring step — the behavior doesn't change yet, but the function now has access to the full metrics.

Location: `crates/editor/src/glyph_atlas.rs`

### Step 3: Implement scaling logic for oversized fallback glyphs

When the fallback font's `line_height` exceeds the cell's `cell_height`, compute a scale factor `scale = cell_height / fallback_line_height` and apply it. Core Graphics supports scaling transforms via `CGContextScaleCTM`.

The implementation:
1. Compute scale factor: `scale = min(1.0, cell_height as f64 / fallback_line_height)`
2. If `scale < 1.0`:
   - Apply `CGContextScaleCTM(context, scale, scale)` before drawing
   - Adjust the draw position to center the scaled glyph vertically
3. If `scale >= 1.0`, use the existing baseline positioning (no change)

Location: `crates/editor/src/glyph_atlas.rs#rasterize_glyph_with_ct_font`

**Test (visual verification initially, then integration test):**
```rust
#[test]
fn test_fallback_glyph_not_clipped() {
    // Render a powerline symbol and verify the bitmap has non-zero pixels
    // near the top and bottom edges (not clipped)
    let device = get_test_device();
    let font = Font::new("Menlo-Regular", 14.0, 1.0);
    let mut atlas = GlyphAtlas::new(&device, &font);

    // Powerline separator (often comes from a tall nerd font)
    let powerline = '\u{E0B0}'; //
    let glyph = atlas.ensure_glyph(&font, powerline);

    // Should return a glyph (either actual or replacement)
    assert!(glyph.is_some());
}
```

### Step 4: Update `add_glyph_with_source` to pass fallback font metrics

Modify `add_glyph_with_source` to query the fallback font's metrics and pass them to `rasterize_glyph_with_ct_font` instead of the primary font's descent.

Location: `crates/editor/src/glyph_atlas.rs#add_glyph_with_source`

Before:
```rust
self.rasterize_glyph_with_ct_font(
    fallback_font,
    font.metrics.descent,  // Wrong: using primary font's descent
    source.glyph_id,
    glyph_width,
    glyph_height,
)
```

After:
```rust
let (fb_ascent, fb_descent, fb_line_height) = Font::get_ct_font_metrics(fallback_font);
self.rasterize_glyph_with_ct_font(
    fallback_font,
    fb_ascent,
    fb_descent,
    fb_line_height,
    source.glyph_id,
    glyph_width,
    glyph_height,
)
```

### Step 5: Refactor rasterize_glyph to use the new signature

Update `rasterize_glyph` (the primary font path) to call `rasterize_glyph_with_ct_font` with the primary font's metrics. This ensures both paths go through the same code.

Location: `crates/editor/src/glyph_atlas.rs#rasterize_glyph`

### Step 6: Add integration tests for vertical centering

Write tests that verify fallback glyphs are positioned correctly (not clipped at top/bottom) and don't regress the primary font path.

Location: `crates/editor/src/glyph_atlas.rs` (test module)

```rust
#[test]
fn test_ascii_glyphs_unaffected_by_fallback_metrics_changes() {
    // ASCII characters should render identically before and after this change
    let device = get_test_device();
    let font = Font::new("Menlo-Regular", 14.0, 1.0);
    let atlas = GlyphAtlas::new(&device, &font);

    for c in 'A'..='Z' {
        let glyph = atlas.get_glyph(c);
        assert!(glyph.is_some());
        // Dimensions should match cell size
        let (cell_w, cell_h) = atlas.cell_dimensions();
        let g = glyph.unwrap();
        assert_eq!(g.width as usize, cell_w);
        assert_eq!(g.height as usize, cell_h);
    }
}

#[test]
fn test_fallback_glyphs_fit_within_cell() {
    let device = get_test_device();
    let font = Font::new("Menlo-Regular", 14.0, 1.0);
    let mut atlas = GlyphAtlas::new(&device, &font);

    // Emoji often come from tall Apple Color Emoji font
    let emoji = '😀';
    let glyph = atlas.ensure_glyph(&font, emoji);

    assert!(glyph.is_some());
    let g = glyph.unwrap();
    let (cell_w, cell_h) = atlas.cell_dimensions();
    assert_eq!(g.width as usize, cell_w, "Glyph width should match cell width");
    assert_eq!(g.height as usize, cell_h, "Glyph height should match cell height");
}
```

### Step 7: Manual visual verification

Test with real terminal content containing powerline symbols and nerd font icons:
1. Build and run lite-edit
2. Open a terminal tab
3. Run a command that displays powerline symbols (e.g., a shell prompt with powerline)
4. Verify symbols are fully visible (not clipped at top/bottom)
5. Compare against macOS Terminal.app as reference

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments:

```rust
// Chunk: docs/chunks/fallback_glyph_metrics - Scale fallback glyphs to fit cell bounds
```

## Dependencies

- **font_fallback_rendering** (ACTIVE): This chunk extends the font fallback mechanism established there. The `GlyphFont::Fallback(CFRetained<CTFont>)` variant and `glyph_for_char_with_fallback()` must exist.
- No new external libraries required — Core Graphics `CGContextScaleCTM` is already available via the existing objc2 bindings.

## Risks and Open Questions

1. **Scaling artifacts**: Scaling down a glyph may introduce visual artifacts (blurry edges, jagged lines). Core Graphics uses high-quality interpolation by default, but we should verify visually that scaled glyphs remain legible.

2. **Apple Color Emoji special case**: Apple Color Emoji is a bitmap font (COLR/CPAL or sbix), not an outline font. Core Text's `ct_font.draw_glyphs()` may handle this differently. Testing with emoji will reveal if special handling is needed.

3. **Performance impact**: Querying fallback font metrics adds overhead to the fallback path. This should be negligible since:
   - It only fires for fallback glyphs (non-ASCII, non-Menlo characters)
   - Results are cached in the glyph atlas
   - The primary ASCII path is unchanged

4. **Vertical centering vs baseline alignment**: The current approach centers the scaled glyph vertically. An alternative is baseline alignment (preserving the baseline position and only scaling the glyph). Need to verify which looks better with real content.

5. **CGContextScaleCTM availability**: Need to verify the objc2_core_graphics crate exposes this function. If not, may need to use `CGAffineTransform` on the font instead.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->