<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk addresses two rendering gaps in the terminal:

1. **Background colors not rendering** — The `terminal_styling_fidelity` chunk added per-span background quad emission to `update_from_buffer_with_wrap`. However, background quads may not be appearing visually. We need to verify the pipeline is working end-to-end and fix any remaining issues.

2. **Box-drawing glyphs not rendering** — The glyph atlas was originally built to handle printable ASCII (0x20-0x7E). Unicode box-drawing characters (U+2500–U+257F), block elements (U+2580–U+259F), and other TUI glyphs are not pre-populated. When these characters are encountered, `GlyphAtlas::get_glyph()` returns `None`, and the character is silently skipped.

### Strategy

**Background rendering** is already implemented in `glyph_buffer.rs` Phase 1 (Background Quads) and drawn in `renderer.rs` `render_text()`. We will verify this is working correctly and add integration tests to cover background color scenarios.

**Box-drawing glyph rendering** requires extending the atlas to support on-demand rasterization of non-ASCII characters. The atlas already has `ensure_glyph()` which calls `add_glyph()` when a character is missing. However, the rendering path in `glyph_buffer.rs` uses `get_glyph()` which doesn't trigger on-demand addition. We will:

1. Change the rendering path to use `ensure_glyph()` via a mutable atlas reference
2. Verify Core Text can rasterize box-drawing and block element characters from the font
3. Test that the atlas doesn't overflow with reasonable terminal workloads

This approach maintains performance (on-demand rasterization only for new characters) while extending coverage beyond ASCII.

## Subsystem Considerations

No existing subsystems are directly relevant. This chunk builds on the glyph rendering infrastructure from `glyph_rendering` and the terminal styling work from `terminal_styling_fidelity`.

## Sequence

### Step 1: Add integration tests for background colors in terminal output

Location: `crates/terminal/tests/integration.rs`

Add tests verifying:
- Background colors are captured in `StyledLine` spans (this is already covered)
- 256-color indexed backgrounds work
- RGB truecolor backgrounds work
- Combined foreground + background scenarios work

These tests verify the data pipeline before rendering. If backgrounds show up in `styled_line()` but don't render, the issue is in the glyph buffer or renderer.

### Step 2: Add test for glyph atlas on-demand extension

Location: `crates/editor/src/glyph_atlas.rs`

Add tests verifying:
- `ensure_glyph()` adds non-ASCII characters on demand
- Box-drawing characters (e.g., `'─'` U+2500, `'│'` U+2502, `'┌'` U+250C) can be rasterized
- Block elements (e.g., `'█'` U+2588, `'▀'` U+2580) can be rasterized
- Characters outside BMP (> U+FFFF) gracefully fall back to space

### Step 3: Change glyph buffer rendering to use mutable atlas

Location: `crates/editor/src/glyph_buffer.rs`

The current rendering paths (`update_from_buffer_with_cursor`, `update_from_buffer_with_wrap`) take `&GlyphAtlas` (immutable). To support on-demand glyph addition:

1. Change signature to take `&mut GlyphAtlas`
2. Change `atlas.get_glyph(c)` calls to `atlas.ensure_glyph(&font, c)`
3. This requires also passing `&Font` to the rendering methods

Update call sites in:
- `GlyphBuffer::update()`
- `GlyphBuffer::update_from_buffer()`
- `GlyphBuffer::update_from_buffer_with_cursor()`
- `GlyphBuffer::update_from_buffer_with_wrap()`

And in `Renderer`:
- `update_glyph_buffer()` must pass `&mut self.atlas` and `&self.font`

### Step 4: Update Renderer to pass mutable atlas and font reference

Location: `crates/editor/src/renderer.rs`

Modify `Renderer::update_glyph_buffer()` to pass mutable atlas and font:

```rust
fn update_glyph_buffer(&mut self, view: &dyn BufferView) {
    // ... existing code ...
    self.glyph_buffer.update_from_buffer_with_wrap(
        &self.device,
        &mut self.atlas,  // Changed: mutable
        &self.font,       // Added: font for on-demand rasterization
        view,
        &self.viewport,
        &wrap_layout,
        self.cursor_visible,
        y_offset,
    );
}
```

This change propagates through all rendering paths that use the glyph buffer.

### Step 5: Add integration test for box-drawing characters in terminal

Location: `crates/terminal/tests/integration.rs`

Add tests that:
1. Spawn a command that outputs box-drawing characters
2. Verify the characters appear in `styled_line()` output
3. This confirms the terminal emulator preserves Unicode characters

Example: `printf "┌──┐\n│  │\n└──┘\n"` should produce a small box.

### Step 6: Visual verification of terminal rendering

Test the full pipeline by running:

1. `htop` or `top` — TUI with status bars, borders, and colored sections
2. `vim` with a colorscheme — syntax highlighting with background highlights
3. `echo -e '\e[44mBlue BG\e[0m'` — blue background should be visible
4. `printf "┌──────┐\n│ Test │\n└──────┘\n"` — box should render with connected lines
5. Custom test: `printf '\e[48;5;196m RED BG \e[0m'` — 256-color red background

Document any visual issues found for follow-up.

### Step 7: Verify Menlo font has box-drawing glyphs

Location: `crates/editor/src/font.rs` (test code)

Add a test that verifies the font we use (Menlo-Regular) has glyphs for common box-drawing characters:
- U+2500 `─` (horizontal line)
- U+2502 `│` (vertical line)
- U+250C `┌` (top-left corner)
- U+2510 `┐` (top-right corner)
- U+2514 `└` (bottom-left corner)
- U+2518 `┘` (bottom-right corner)
- U+2588 `█` (full block)

If any are missing from Menlo, we may need a fallback font strategy (future work).

## Dependencies

- `terminal_styling_fidelity` chunk (ACTIVE): Provides the per-span background quad emission we're verifying
- `glyph_rendering` chunk (ACTIVE): Provides the atlas infrastructure we're extending
- `renderer_styled_content` chunk (ACTIVE): Provides per-vertex color rendering

## Risks and Open Questions

1. **Font coverage**: Menlo may not have all box-drawing or block element glyphs. If `glyph_for_char()` returns `None`, we fall back to space. This is acceptable but produces broken TUI layouts. A fallback font could be added in future work.

2. **Atlas capacity**: The atlas is 1024x1024 pixels. Adding many non-ASCII glyphs could exhaust space. We should monitor atlas utilization and add a warning or metric. The current warning in `add_glyph()` is sufficient for now.

3. **Performance**: On-demand glyph rasterization happens synchronously during rendering. For the first render of a TUI app with many unique characters, this could cause a stutter. The atlas caches glyphs, so subsequent renders are fast.

4. **Non-BMP characters**: Characters outside the Basic Multilingual Plane (> U+FFFF) like emoji cannot be looked up with a single `u16` in Core Text's glyph API. These gracefully fall back to space. Proper emoji support would require switching to UTF-16 surrogate pair handling.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
