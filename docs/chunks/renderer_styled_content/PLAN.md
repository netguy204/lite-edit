<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk enhances the Metal rendering pipeline to render `Style` attributes from the `BufferView` trait. The current renderer draws white glyphs on a uniform dark background using a single `TEXT_COLOR` uniform. We need to add:

1. **Per-cell background colors** via a background rect pass before the glyph pass
2. **Per-span foreground colors** via per-vertex color or separate draw calls
3. **Cursor shape rendering** (Block, Beam, Underline) based on `CursorInfo::shape`
4. **Underline rendering** beneath text spans
5. **Inverse video and dim effects** applied at render time

### Rendering Strategy: Multi-Pass with Per-Span Colors

The current architecture uses multiple draw calls with a per-draw color uniform (see `render_text()` in `renderer.rs`). This pattern works well:
- Selection quads → `SELECTION_COLOR`
- Glyph quads → `TEXT_COLOR`
- Cursor quad → `TEXT_COLOR`

We'll extend this pattern by emitting **background quads** and **foreground glyphs per span** with distinct colors. For each visible line:

1. **Background pass**: Emit one quad per span with non-default `bg` color
2. **Glyph pass**: Emit glyph quads per span with their `fg` color
3. **Underline pass**: Emit thin rectangles beneath underlined spans
4. **Cursor pass**: Emit cursor quad in the appropriate shape

Since the fragment shader already supports a `text_color` uniform that's set per draw call, we can batch quads by color. However, to keep complexity low initially, we'll use **per-vertex colors** (extending the vertex format) rather than sorting/batching by color. This requires a shader change.

### Alternative Considered: Texture-Based Approach

We could encode styles in a texture and sample per-vertex. This is more GPU-efficient but adds complexity. Defer to a future optimization chunk if performance becomes an issue.

### Key Design Decisions

1. **Per-vertex color**: Add a `color: [f32; 4]` field to `GlyphVertex`. This doubles vertex size but simplifies the pipeline — no sorting by color, single draw call per pass.

2. **Color resolution**: The `Color` enum (Default, Named, Indexed, RGB) must be resolved to RGBA at render time. Add a `ColorPalette` struct to hold the Catppuccin theme colors, resolve Named/Indexed colors via lookup.

3. **Background quads emit per-span**: Only emit background quads for spans with non-default `bg`. Most text has default background, so this is typically sparse.

4. **Cursor shapes as geometry**: Different cursor shapes are different quad geometries:
   - Block: full cell rectangle
   - Beam: thin vertical rectangle (2px wide)
   - Underline: thin horizontal rectangle at baseline

5. **Underlines as separate quads**: Underline quads are separate from glyph quads, drawn after glyphs so they appear beneath (in Z-order, glyphs are drawn last).

6. **Inverse video**: Swap `fg` and `bg` during color resolution when `Style::inverse` is set.

7. **Dim effect**: Multiply alpha by ~0.5 when `Style::dim` is set.

### Files to Touch

- `crates/editor/shaders/glyph.metal` — Add per-vertex color input
- `crates/editor/src/shader.rs` — Update vertex descriptor and `VERTEX_SIZE`
- `crates/editor/src/glyph_buffer.rs` — Extend `GlyphVertex` with color, update quad generation
- `crates/editor/src/renderer.rs` — Update render passes, add color palette
- `crates/editor/src/lib.rs` — Export new types if needed
- `crates/buffer/src/buffer_view.rs` — (Read-only) Use existing `Style` types

## Sequence

### Step 1: Add ColorPalette for resolving Color enum to RGBA

Create a `color_palette.rs` module in `crates/editor/src/` that:

1. Defines `ColorPalette` struct holding resolved RGBA values for:
   - Named colors (16 ANSI colors from Catppuccin Mocha theme)
   - Default foreground and background colors

2. Implements `resolve_color(&self, color: Color, is_foreground: bool) -> [f32; 4]`:
   - `Color::Default` → use default fg or bg from palette
   - `Color::Named(n)` → lookup in 16-color table
   - `Color::Indexed(i)` → lookup in 256-color table (build standard xterm palette)
   - `Color::Rgb { r, g, b }` → convert to normalized floats

3. Implements `resolve_style_colors(&self, style: &Style) -> (fg_rgba, bg_rgba)`:
   - Resolve fg and bg
   - If `style.inverse`, swap them
   - If `style.dim`, multiply fg alpha by 0.5

Location: `crates/editor/src/color_palette.rs`

Include unit tests for color resolution (Named, Indexed, RGB, inverse, dim).

### Step 2: Extend GlyphVertex with per-vertex color

Modify `GlyphVertex` in `glyph_buffer.rs`:

```rust
#[repr(C)]
pub struct GlyphVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],  // NEW: per-vertex RGBA color
}
```

Update `VERTEX_SIZE` in `shader.rs` to reflect the new size (16 bytes → 32 bytes).

Update the vertex descriptor in `GlyphPipeline::new()`:
- Attribute 0: position (float2, offset 0)
- Attribute 1: uv (float2, offset 8)
- Attribute 2: color (float4, offset 16)

Update `GlyphVertex::new()` to accept color parameter.

Location: `crates/editor/src/glyph_buffer.rs`, `crates/editor/src/shader.rs`

### Step 3: Update Metal shader for per-vertex color

Modify `glyph.metal`:

```metal
struct GlyphVertex {
    float2 position [[attribute(0)]];
    float2 uv [[attribute(1)]];
    float4 color [[attribute(2)]];  // NEW
};

struct FragmentInput {
    float4 position [[position]];
    float2 uv;
    float4 color;  // NEW: pass through to fragment
};
```

Update vertex shader to pass color through.

Update fragment shader:
- Remove `constant float4& text_color [[buffer(0)]]` (no longer per-draw)
- Use `in.color` directly as the glyph color
- Keep atlas sampling for alpha: `return float4(in.color.rgb, in.color.a * alpha);`

Location: `crates/editor/shaders/glyph.metal`

### Step 4: Add background quad emission per span

In `glyph_buffer.rs`, modify `update_from_buffer_with_cursor()`:

**New Background Phase** (before selection quads):
1. For each visible line, call `view.styled_line(line)`
2. For each span with non-default `bg` color:
   - Calculate span's column range
   - Emit a solid quad covering those columns
   - Color = resolved bg color from palette

Track `background_range: QuadRange` for draw ordering.

Add helper: `create_span_background_quad(screen_row, start_col, end_col, color, solid_glyph, y_offset)`.

Update render passes to draw background quads first.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 5: Update glyph emission to use per-span foreground colors

Modify the glyph emission loop in `update_from_buffer_with_cursor()`:

1. For each visible line, call `view.styled_line(line)` to get `StyledLine`
2. Track column offset as we iterate spans
3. For each span:
   - Resolve fg color (applying inverse/dim via palette)
   - For each character in span:
     - Emit glyph quad with the span's resolved fg color
     - Increment column

This replaces the current approach of concatenating span text and ignoring styles.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 6: Implement cursor shape rendering

Modify cursor quad creation to respect `CursorInfo::shape`:

Add `create_cursor_quad_for_shape(screen_row, col, shape, solid_glyph, y_offset, layout)`:
- `CursorShape::Block`: Full cell rectangle (current behavior)
- `CursorShape::Beam`: Thin vertical rectangle (2px wide × line_height)
- `CursorShape::Underline`: Thin horizontal rectangle (glyph_width × 2px) at baseline
- `CursorShape::Hidden`: Emit no quad (skip)

Use `cursor_info.shape` from `BufferView::cursor_info()`.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 7: Add underline rendering

Add underline quad emission for spans with `UnderlineStyle != None`:

**New Underline Phase** (after glyph quads, before cursor):
1. For each visible line, iterate spans
2. For spans with `style.underline != UnderlineStyle::None`:
   - Calculate span's column range
   - Emit thin horizontal quad at baseline - 2px
   - Color = `style.underline_color` or fg color

Start with `UnderlineStyle::Single` only — a 1px or 2px horizontal line.

For stretch goal (curly/dotted/dashed), would need either:
- Texture-based approach (pre-rendered underline patterns)
- Procedural fragment shader

Defer fancy underlines to a future chunk; implement Single for now.

Track `underline_range: QuadRange` for draw ordering.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 8: Update renderer to use new quad ranges

Modify `render_text()` in `renderer.rs`:

1. Remove the per-draw `text_color` uniform setting (now per-vertex)
2. Update draw order:
   - Background quads (new)
   - Selection quads
   - Glyph quads
   - Underline quads (new)
   - Cursor quad

3. Each pass is a single draw call with the appropriate index range.

Since colors are now per-vertex, each pass uses the same shader with no uniform changes between draws.

Location: `crates/editor/src/renderer.rs`

### Step 9: Wire ColorPalette through rendering path

1. Add `ColorPalette` field to `GlyphBuffer`
2. Pass palette to `update_from_buffer_with_cursor()`
3. Use palette for all color resolution

Consider making palette configurable for future theming, but start with hardcoded Catppuccin Mocha.

Location: `crates/editor/src/glyph_buffer.rs`, `crates/editor/src/renderer.rs`

### Step 10: Integration testing and performance validation

1. **Verify existing behavior**: Default-styled text should render identically to before (white on dark background)

2. **Test styled rendering**: Create a mock `BufferView` that returns styled spans:
   - Lines with multiple fg colors
   - Lines with bg colors
   - Lines with inverse style
   - Lines with dim style
   - Lines with underlines

3. **Test cursor shapes**: Verify Block, Beam, Underline render correctly

4. **Performance test**: Verify full screen redraw (40 lines × 120 cols) completes within 2ms budget

Location: Unit tests in `glyph_buffer.rs`, integration tests in separate test module

## Dependencies

- **buffer_view_trait chunk** (ACTIVE): Provides `BufferView`, `Style`, `Color`, `StyledLine`, `CursorShape`, etc.
  - This chunk depends on these types being defined. The dependency is satisfied (chunk is ACTIVE).

## Risks and Open Questions

1. **Per-vertex color overhead**: Doubling vertex size from 16 to 32 bytes increases GPU bandwidth. For a full screen (40×120 = 4800 chars × 4 verts × 32 bytes = ~600KB), this is well within GPU bandwidth. But worth measuring.

2. **Color batching optimization**: If profiling shows the single-draw-call approach is slow due to unbatched colors, we could:
   - Sort quads by color and batch
   - Use instanced rendering with color as instance data
   - Defer to optimization chunk

3. **Underline positioning**: The "baseline - 2px" heuristic may not look right for all fonts. May need to derive from font metrics (descender line).

4. **Hidden text**: When `Style::hidden` is set, should we skip glyph quads entirely or render with alpha=0? Skipping saves GPU work; alpha=0 maintains consistent vertex buffer size. Start with skipping.

5. **256-color palette**: Building the standard xterm 256-color palette requires careful color math (6×6×6 color cube + grayscale). Reference xterm source or a known-good implementation.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?
-->