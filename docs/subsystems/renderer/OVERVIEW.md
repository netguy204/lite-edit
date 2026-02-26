---
status: DOCUMENTED
code_references:
- ref: crates/editor/src/renderer.rs#Renderer
  implements: Core Metal rendering orchestration
  compliance: COMPLIANT
- ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer
  implements: Vertex buffer construction for text rendering
  compliance: COMPLIANT
- ref: crates/editor/src/glyph_buffer.rs#GlyphVertex
  implements: Per-vertex data structure
  compliance: COMPLIANT
- ref: crates/editor/src/glyph_atlas.rs#GlyphAtlas
  implements: Texture atlas for rasterized glyphs with on-demand addition
  compliance: COMPLIANT
- ref: crates/editor/src/shader.rs#GlyphPipeline
  implements: Shader compilation and render pipeline state
  compliance: COMPLIANT
- ref: crates/editor/shaders/glyph.metal
  implements: GPU rendering contract (vertex and fragment shaders)
  compliance: COMPLIANT
chunks:
- chunk_id: metal_surface
  relationship: implements
- chunk_id: glyph_rendering
  relationship: implements
- chunk_id: viewport_rendering
  relationship: implements
- chunk_id: text_selection_rendering
  relationship: implements
- chunk_id: line_wrap_rendering
  relationship: implements
- chunk_id: selector_rendering
  relationship: implements
- chunk_id: renderer_styled_content
  relationship: implements
- chunk_id: content_tab_bar
  relationship: implements
- chunk_id: find_in_file
  relationship: implements
- chunk_id: dirty_tab_close_confirm
  relationship: implements
- chunk_id: tiling_multi_pane_render
  relationship: implements
- chunk_id: renderer_polymorphic_buffer
  relationship: implements
- chunk_id: selector_list_clipping
  relationship: implements
- chunk_id: terminal_background_box_drawing
  relationship: implements
- chunk_id: terminal_multibyte_rendering
  relationship: implements
- chunk_id: font_fallback_rendering
  relationship: implements
- chunk_id: find_strip_multi_pane
  relationship: implements
created_after:
- viewport_scroll
---
# renderer

## Intent

The renderer subsystem provides GPU-accelerated text and UI rendering for the editor. It converts buffer content, overlays, and panels into vertex buffers and issues Metal draw calls to produce visible output on screen.

The core problem is coordinating multiple rendering concerns (text content, selections, overlays, panels, cursors) into a single coherent frame that updates efficiently. The subsystem manages the Metal command queue, vertex buffer construction, shader pipeline, and draw order to ensure correct visual output.

## Scope

### In Scope

- `Renderer` struct - Metal rendering orchestration (command queue, draw passes, frame output)
- `GlyphBuffer`, `GlyphVertex` - Vertex buffer construction for text and UI elements
- `glyph.metal` shader - GPU rendering contract (vertex/fragment shaders)
- `GlyphAtlas` - Texture atlas for rasterized glyphs, with on-demand glyph addition
- `shader.rs` - Shader compilation and render pipeline state setup
- Overlay/panel glyph buffers (selector_overlay, left_rail, tab_bar, pane_frame, confirm_dialog)
- Scissor rect management for viewport clipping
- Consuming `BufferView` trait for polymorphic content access

### Out of Scope

- `Viewport` / `WrapLayout` / `Font` / font metrics - separate layout subsystem
- Color constants and themes - future theming subsystem
- `MetalView` - platform abstraction (future platform subsystem)
- Buffer implementations (`TextBuffer`, terminal buffers, etc.) - consumer of renderer

## Invariants

### Hard Invariants

1. **Atlas Availability**: Any glyph rendered in a frame must already exist in the glyph atlas before the draw call. The renderer cannot render missing glyphs.

2. **Single Frame Contract**: Each `render_*` call produces exactly one complete frame to the screen. Partial or incomplete frames are not allowed.

3. **Screen-Space Consistency**: All coordinates in a single render pass must use the same screen-space coordinate system (pixels, origin top-left, Y-down). Mixed coordinate systems would cause misalignment.

4. **Layering Contract**: Overlays (selector, dialogs, tooltips) always render on top of editor content. Nothing can draw over an overlay.

### Soft Conventions

1. **Scissor Rects for Containment**: Use scissor rects to prevent content from bleeding where it shouldn't (e.g., tab bar vs content area, query vs list items).

2. **Draw Order Within Layer**: Within a single layer (e.g., glyphs), draw order is background → selection → glyphs → cursor for visual correctness.

## Implementation Locations

### Core Renderer (crates/editor/src/renderer.rs)

The `Renderer` struct is the canonical entry point for all rendering operations. It has the following responsibilities:

- **Metal resource management**: Owning the command queue, device reference, and shader pipeline
- **Glyph buffer orchestration**: Coordinating `update_glyph_buffer*` methods to construct vertex data
- **Viewport interaction**: Working with `Viewport` (from the layout subsystem) to determine visible content
- **Overlay rendering**: Drawing selector panels, left rail, tab bars, and confirm dialogs on top of content
- **Scissor rect management**: Applying viewport clipping to prevent content bleeding
- **Multi-pane support**: Rendering multiple panes with independent scroll positions

The renderer's `render_with_editor` method is the primary entry point, handling the complete render pipeline: clear → left rail → content overlays → selector/confirm overlays → present.

### Glyph Buffer Construction (crates/editor/src/glyph_buffer.rs)

The `GlyphBuffer` struct manages vertex and index buffers for glyph rendering:

- **Text quads**: Each character becomes a quad with four `GlyphVertex` instances
- **Quad categories**: Organized by type (background, selection, border, glyph, underline, cursor) with separate index ranges
- **Atlas integration**: On-demand glyph addition via mutably-passed `GlyphAtlas`
- **Wrap-aware rendering**: Supports soft line wrapping via `WrapLayout` (from layout subsystem)
- **Cursor rendering**: Blinking or static cursor based on visibility flag

The `GlyphLayout` struct provides pure layout calculations (glyph width, line height, ascent) without Metal dependencies, making it testable.

### Glyph Atlas (crates/editor/src/glyph_atlas.rs)

The `GlyphAtlas` manages a Metal texture containing rasterized glyphs:

- **Pre-population**: ASCII characters are rasterized at initialization
- **On-demand addition**: Non-ASCII glyphs are added to the atlas during buffer updates
- **UV coordinate mapping**: Returns normalized texture coordinates for each glyph
- **Layout algorithm**: Packs glyphs into a 2D grid with row/column tracking for space efficiency

The atlas must be updated before any glyph is rendered, honoring the **Atlas Availability** invariant.

### Shader Pipeline (crates/editor/src/shader.rs, crates/editor/shaders/glyph.metal)

The `GlyphPipeline` struct compiles the Metal shaders and creates the render pipeline state:

- **Vertex shader**: Transforms screen-space coordinates to Metal NDC (normalized device coordinates), flips Y-axis
- **Fragment shader**: Samples the glyph atlas and applies per-vertex color with alpha blending
- **Vertex descriptor**: Maps `GlyphVertex` struct to shader attributes (position, uv, color)
- **Pipeline state**: Encapsulates compiled shaders, vertex format, and blend state

The shader defines the rendering contract for all glyph rendering in the subsystem.

### Overlay Glyph Buffers

Each overlay type has its own glyph buffer type that follows the same pattern:

- `SelectorGlyphBuffer` (crates/editor/src/selector_overlay.rs): Selector panels with query row and item list
- `LeftRailGlyphBuffer` (crates/editor/src/left_rail.rs): Workspace tiles with status indicators
- `TabBarGlyphBuffer` (crates/editor/src/tab_bar.rs): Tab labels with active/inactive styling
- `FindStripGlyphBuffer` (crates/editor/src/selector_overlay.rs): Find-in-file strip
- `WelcomeScreenGlyphBuffer` (crates/editor/src/welcome_screen.rs): Welcome screen content
- `PaneFrameBuffer` (crates/editor/src/pane_frame_buffer.rs): Pane dividers and focus borders
- `ConfirmDialogGlyphBuffer` (crates/editor/src/confirm_dialog.rs): Confirmation dialogs

All follow the same pattern: lazy-initialized, updated from widget state, rendered with scissor rects for containment.

## Chunk Relationships

Many editor chunks implement rendering features within this subsystem:

### Foundational Infrastructure

- **metal_surface**: Established the foundational Metal rendering infrastructure (window, CAMetalLayer, command queue, render passes)
- **glyph_rendering**: Built the text rendering pipeline (font loading, glyph atlas, vertex buffers, Metal shader)
- **viewport_rendering**: Connected buffer content to rendering through viewport abstraction and dirty region tracking

### Core Rendering Features

- **text_selection_rendering**: Added selection highlight rendering with separate draw pass for selection quads
- **line_wrap_rendering**: Implemented soft line wrapping with continuation row borders and wrap-aware coordinate mapping
- **renderer_styled_content**: Extended rendering to support per-vertex colors, cursor shapes (Block/Beam/Underline), underlines, and inverse/dim styling

### Overlay/Panel Rendering

- **selector_rendering**: Added selector panel overlay rendering (background, query row, separator, item list with selection highlight)
- **content_tab_bar**: Implemented tab bar rendering with active/inactive styling
- **find_in_file**: Added find strip rendering overlay
- **dirty_tab_close_confirm**: Implemented confirmation dialog rendering

### Advanced Rendering

- **tiling_multi_pane_render**: Added multiple pane rendering with independent scroll positions and pane dividers
- **renderer_polymorphic_buffer**: Refactored renderer to accept `BufferView` trait instead of owning buffer copies
- **selector_list_clipping**: Added scissor rect clipping for selector item lists to prevent bleed into query area
- **terminal_background_box_drawing**: Implemented on-demand glyph addition for terminal rendering
- **terminal_multibyte_rendering**: Added non-BMP character support via UTF-16 surrogate pairs and width-aware column positioning for CJK/wide characters
- **font_fallback_rendering**: Implemented Core Text font fallback for characters not in the primary font (hieroglyphs, emoji, math symbols) with U+FFFD replacement for truly missing glyphs

## Known Deviations

No known deviations at this time.