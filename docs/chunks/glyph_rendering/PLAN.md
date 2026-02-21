<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk builds the text rendering pipeline on top of the existing Metal infrastructure from `metal_surface`. The pipeline has three major components:

1. **Font loading + metrics** — Use Core Text to load a monospace font (Menlo) and extract critical metrics: glyph advance width, line height, ascent, descent. These metrics drive layout.

2. **Glyph atlas** — A Metal texture that caches rasterized glyphs. Each glyph is rasterized on demand via Core Text and packed into the atlas. UV coordinates map character codes to atlas regions.

3. **Textured quad rendering** — A Metal shader that renders screen-aligned quads with glyph textures. Each glyph is a quad positioned at `(col * glyph_width, row * line_height)` with UVs into the atlas.

### Key design choices

- **Monospace simplifies everything**: No complex text shaping, no kerning, no ligatures. `x = col * advance_width` is the entire layout algorithm.
- **On-demand glyph rasterization**: We pre-populate printable ASCII (0x20-0x7E) at startup for predictable performance, but the atlas can grow for extended characters.
- **Single texture atlas**: Start with a fixed-size atlas (e.g., 1024×1024). If we exceed capacity, fail loudly for now — this is a code editor, not a Unicode explorer.
- **Humble view architecture**: The glyph layout math (computing positions and UVs) is pure Rust and fully testable. Only the Core Text rasterization and Metal draw calls are platform-dependent.

### Building on metal_surface

The existing `Renderer` creates a command queue and performs clear operations. This chunk extends it with:
- A texture atlas (`MTLTexture`)
- A render pipeline state with vertex/fragment shaders
- Vertex/index buffers for glyph quads

The existing `MetalView` provides device access and Retina scale factor — both needed for correct glyph rendering.

## Subsystem Considerations

No existing subsystems documented yet. This chunk may seed a future `text_rendering` subsystem if glyph atlas patterns recur across the codebase.

## Sequence

### Step 1: Create the font module with Core Text integration

Create `crates/editor/src/font.rs` that:
- Loads a monospace font by name via Core Text (`CTFontCreateWithName`)
- Extracts metrics: advance width (from any glyph, e.g., 'M'), line height, ascent, descent
- Accounts for display scale factor (metrics are in points, we need pixels)
- Provides a `Font` struct holding the CTFont reference and computed metrics

**Location**: `crates/editor/src/font.rs`

**Key APIs**: `CTFontCreateWithName`, `CTFontGetAdvancesForGlyphs`, `CTFontGetAscent`, `CTFontGetDescent`, `CTFontGetLeading`

**Note**: Add `objc2-core-text` dependency to Cargo.toml and link CoreText framework in build.rs.

### Step 2: Implement the glyph atlas

Create `crates/editor/src/glyph_atlas.rs` that:
- Creates a Metal texture (R8Unorm for grayscale glyphs) at a fixed size (1024×1024)
- Maintains a mapping from character → (UV rect in atlas)
- Rasterizes glyphs on demand via Core Text (`CTFontDrawGlyphs` into a CGContext)
- Uses a simple row-based packer: fill rows left-to-right, move to next row when full
- Pre-populates all printable ASCII characters (0x20-0x7E) at construction time
- Provides `get_glyph(char) -> Option<GlyphInfo>` where `GlyphInfo` contains UV coordinates and glyph metrics

**Location**: `crates/editor/src/glyph_atlas.rs`

**Key insight**: Glyph rasterization happens into a CPU buffer (via CGContext), then we upload to the Metal texture. Core Text gives us alpha coverage, which we store in the R channel.

### Step 3: Create Metal shaders for textured quad rendering

Create `crates/editor/shaders/glyph.metal`:
- **Vertex shader**: Takes per-vertex position and UV, applies orthographic projection to convert screen coordinates to NDC
- **Fragment shader**: Samples the atlas texture at the interpolated UV, outputs glyph alpha as text color (foreground) blended over background

Create `crates/editor/src/shader.rs` that:
- Compiles the shader source at runtime via `MTLDevice::newLibraryWithSource`
- Creates a render pipeline state with the vertex/fragment functions
- Configures blending (source alpha, one minus source alpha) for anti-aliased glyphs

**Location**: `crates/editor/shaders/glyph.metal`, `crates/editor/src/shader.rs`

### Step 4: Build the glyph vertex buffer

Create `crates/editor/src/glyph_buffer.rs` that:
- Takes an array of (row, col, char) and produces a vertex buffer of quads
- Each quad is 4 vertices: position (x, y) and UV (u, v)
- Position is computed as: `x = col * glyph_width`, `y = viewport_height - (row * line_height) - ascent` (flip Y for Metal's coordinate system)
- Uses an index buffer for efficient rendering (6 indices per quad: 2 triangles)
- Provides `update(lines: &[&str])` to rebuild the buffer from text content

**Location**: `crates/editor/src/glyph_buffer.rs`

**Testing note**: The layout math (`col * width`, `row * height`, UV computation) is pure and testable. The buffer creation is Metal-dependent.

### Step 5: Integrate glyph rendering into the Renderer

Extend `crates/editor/src/renderer.rs`:
- On construction: create `Font`, `GlyphAtlas`, shader pipeline, buffers
- On `render()`:
  1. Clear to background (existing)
  2. Set the glyph pipeline state
  3. Bind the atlas texture
  4. Bind the vertex/index buffers
  5. Draw indexed primitives
- Add `set_content(lines: &[&str])` to update the glyph buffer with new text

**Location**: `crates/editor/src/renderer.rs`

### Step 6: Wire up hardcoded text display in main.rs

Modify `crates/editor/src/main.rs`:
- After creating the renderer, call `renderer.set_content(DEMO_TEXT)` with ~20 lines of hardcoded text
- Ensure the initial render displays the text
- Verify text remains visible after window resize

**Location**: `crates/editor/src/main.rs`

**Demo text**: A recognizable multi-line string (e.g., a Rust hello-world, a lorem ipsum, or the editor's own source code snippet).

### Step 7: Add unit tests for testable components

Create tests for the pure/testable portions of the pipeline:
- `font.rs`: Test metric extraction (advance > 0, line_height > advance, ascent + descent ≈ line_height)
- `glyph_buffer.rs`: Test position/UV calculations without Metal (mock the buffer creation, verify computed values)
- Test that ASCII range 0x20-0x7E is fully covered by the atlas

**Location**: `crates/editor/src/font.rs` (inline tests), `crates/editor/tests/glyph_layout_test.rs`

### Step 8: Add smoke test and performance validation

Create/extend `crates/editor/tests/smoke_test.rs`:
- Visual verification: launch the editor and confirm text is displayed (manual, documented in test comments)
- Performance test: measure time to render 50 lines × 120 columns (~6,000 glyphs). Assert < 2ms total (layout + GPU submission). This validates H3 from the investigation.

**Location**: `crates/editor/tests/smoke_test.rs`

## Dependencies

- **metal_surface chunk** (ACTIVE): Provides `MetalView`, `Renderer`, device access, Retina scale factor handling
- **Core Text framework**: System framework, no external crate needed beyond `objc2-core-text`
- **Core Graphics framework**: For CGContext-based glyph rasterization

## Risks and Open Questions

1. **Core Text API via objc2**: The `objc2-core-text` crate may have gaps or require manual FFI for some functions (especially `CTFontDrawGlyphs`). Fallback: use raw `objc2` FFI if needed.

2. **Glyph rasterization into Metal texture**: The flow is Core Text → CGContext (CPU) → MTLTexture. This involves a pixel format conversion (BGRA CGContext to R8 texture). May need to extract the alpha channel explicitly.

3. **Coordinate system flips**: Metal's NDC has Y up, Core Text's coordinate system has Y up, but NSView/CAMetalLayer may flip. Need careful testing at Retina scales.

4. **Atlas sizing**: A 1024×1024 R8 texture is 1MB and fits ~16K glyphs at 8×16 cell size. More than enough for ASCII, but extended Unicode could exhaust it. For this chunk, we fail loudly if exhausted — proper atlas management is future work.

5. **Shader compilation latency**: Compiling MSL at runtime on first launch adds latency. Acceptable for this chunk; precompiled metallib is a future optimization.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
