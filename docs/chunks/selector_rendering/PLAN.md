<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds Metal rendering for the `SelectorWidget` overlay. The strategy is to extend the existing `Renderer` with a dedicated method that draws the selector panel on top of the editor content. We build on:

- **Existing glyph atlas** (`GlyphAtlas`) for text rendering
- **Existing glyph buffer patterns** (`GlyphBuffer`, `GlyphVertex`, `QuadRange`) for vertex/index construction
- **Existing solid glyph** (`atlas.solid_glyph()`) for opaque rectangles (background, selection highlight)
- **Existing dirty region system** (`DirtyRegion::FullViewport`) for triggering redraws
- **Existing cursor blink timer** in `EditorState` for the query input cursor

Per the TESTING_PHILOSOPHY.md Humble View Architecture, the renderer is a humble object — we don't unit-test GPU output. However, we'll extract testable layout math into a pure function for panel geometry calculations.

The rendering approach:
1. Draw the editor content first (existing render path)
2. Overlay the selector panel on top when active (new code)

The overlay consists of:
- Background rect (solid opaque dark grey)
- Query row with blinking cursor
- Separator line (1px horizontal line)
- Item list with selection highlight

All quads use the same shader pipeline and atlas — only the colors differ per draw call.

## Subsystem Considerations

No subsystems are documented in this codebase yet. This chunk may contribute to an emergent "overlay rendering" pattern if future features (command palette, tooltips) follow the same approach.

## Sequence

### Step 1: Define overlay layout constants and geometry helper

Create a new module `selector_overlay.rs` (or extend `glyph_buffer.rs`) with:

- Layout constants:
  - `OVERLAY_WIDTH_RATIO: f32 = 0.6` (60% of window width)
  - `OVERLAY_MIN_WIDTH: f32 = 400.0` (minimum width in pixels)
  - `OVERLAY_MAX_HEIGHT_RATIO: f32 = 0.5` (50% of window height)
  - `OVERLAY_TOP_OFFSET_RATIO: f32 = 0.2` (top edge at 20% from top)
  - `OVERLAY_BACKGROUND_COLOR: [f32; 4] = [0.165, 0.165, 0.165, 1.0]` (#2a2a2a)
  - `OVERLAY_SELECTION_COLOR: [f32; 4] = [0.0, 0.314, 0.627, 1.0]` (#0050a0)

- Pure function `calculate_overlay_geometry`:
  ```rust
  pub struct OverlayGeometry {
      pub panel_x: f32,       // left edge
      pub panel_y: f32,       // top edge
      pub panel_width: f32,   // computed width
      pub panel_height: f32,  // computed height (dynamic based on items)
      pub query_row_y: f32,   // y for query text
      pub separator_y: f32,   // y for 1px separator
      pub list_origin_y: f32, // y where item list starts
      pub item_height: f32,   // height per item (line_height)
      pub visible_items: usize, // how many items fit
  }

  pub fn calculate_overlay_geometry(
      view_width: f32,
      view_height: f32,
      line_height: f32,
      item_count: usize,
  ) -> OverlayGeometry
  ```

This function is pure and testable.

Location: `crates/editor/src/selector_overlay.rs`

### Step 2: Add unit tests for `calculate_overlay_geometry`

Write tests verifying:
- Panel width is 60% of view width when view is large enough
- Panel width clamps to minimum 400px when view is too narrow
- Panel height caps at 50% of view height when many items
- Visible items count is computed correctly
- Panel is positioned at 20% from top vertically
- Panel is horizontally centered

Location: `crates/editor/src/selector_overlay.rs` (in `#[cfg(test)]` module)

### Step 3: Create `SelectorGlyphBuffer` for overlay vertex construction

Create a struct analogous to `GlyphBuffer` but specialized for the selector overlay:

```rust
pub struct SelectorGlyphBuffer {
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_count: usize,
    layout: GlyphLayout,
    // Quad ranges for different draw phases
    background_range: QuadRange,    // opaque background
    query_text_range: QuadRange,    // query string glyphs
    query_cursor_range: QuadRange,  // blinking cursor
    separator_range: QuadRange,     // 1px horizontal line
    selection_bg_range: QuadRange,  // selected item background
    item_text_range: QuadRange,     // item list glyphs
}
```

Method:
```rust
pub fn update_from_widget(
    &mut self,
    device: &ProtocolObject<dyn MTLDevice>,
    atlas: &GlyphAtlas,
    widget: &SelectorWidget,
    geometry: &OverlayGeometry,
    cursor_visible: bool,
)
```

This builds vertex data in order:
1. Background rect (single quad covering entire panel)
2. Selection highlight quad (for selected item row)
3. Separator line quad (1px tall)
4. Query text glyphs
5. Query cursor quad (if visible)
6. Item text glyphs (clipped to panel width — just don't emit glyphs past the edge)

Location: `crates/editor/src/selector_overlay.rs`

### Step 4: Add `draw_selector_overlay` method to `Renderer`

Add to `Renderer`:
```rust
pub fn draw_selector_overlay(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    widget: &SelectorWidget,
    cursor_visible: bool,
)
```

This method:
1. Calculates geometry via `calculate_overlay_geometry`
2. Updates the `SelectorGlyphBuffer` via `update_from_widget`
3. Issues draw calls for each quad range with appropriate colors:
   - Background range → `OVERLAY_BACKGROUND_COLOR`
   - Selection background range → `OVERLAY_SELECTION_COLOR`
   - Separator range → `TEXT_COLOR` (or a subdued grey)
   - Query text + cursor → `TEXT_COLOR`
   - Item text → `TEXT_COLOR`

Store the `SelectorGlyphBuffer` as a field in `Renderer` (lazy-initialized).

Location: `crates/editor/src/renderer.rs`

### Step 5: Integrate overlay rendering into main render path

Modify `Renderer::render()` to accept an optional `&SelectorWidget`:

```rust
pub fn render(&mut self, view: &MetalView, selector: Option<&SelectorWidget>, cursor_visible: bool)
```

After drawing the editor content (existing code), if `selector.is_some()`, call `draw_selector_overlay`.

Alternative: Keep `render()` unchanged and add a separate entry point `render_with_selector()`. Either way, the overlay is drawn after the editor content so it appears on top.

Location: `crates/editor/src/renderer.rs`

### Step 6: Wire dirty region for selector state changes

When the selector opens, closes, or its state changes (query, selected_index), the caller must mark `DirtyRegion::FullViewport`. This ensures the overlay and underlying editor are both redrawn.

The caller (future `file_picker` chunk) will be responsible for this. For now, document the contract in comments/docs.

Location: N/A (documentation only in this chunk; actual wiring happens in `file_picker` chunk)

### Step 7: Manual smoke test

Since this chunk involves visual output, verify manually:
1. Create a test harness or temporary code in `main.rs` that opens a `SelectorWidget` with demo items
2. Verify:
   - Panel appears centered horizontally
   - Panel positioned in upper third of window
   - Background is visible and opaque
   - Query row shows text with blinking cursor
   - Separator line visible below query
   - Items render correctly
   - Selected item has highlight background
   - Long items are clipped (not wrapped)
   - Resizing window updates overlay geometry

Remove test harness code after verification (the `file_picker` chunk will provide real integration).

## Dependencies

- **selector_widget chunk** (ACTIVE): Provides `SelectorWidget` struct with `query()`, `items()`, `selected_index()` accessors.
- No new external crates required — all Metal/glyph infrastructure exists.

## Risks and Open Questions

1. **Cursor blink coordination**: The overlay's query cursor should share the same blink state as the main editor cursor. The `cursor_visible` parameter passed to `draw_selector_overlay` should come from the same `EditorState.cursor_visible` flag. This is straightforward but needs attention during integration.

2. **Text clipping**: The plan says "just don't emit glyphs past the edge" for long item names. This is simple but slightly imprecise — a glyph whose left edge is inside but right edge is outside will be omitted entirely. For MVP this is acceptable; pixel-perfect clipping could use a scissor rect in a future iteration.

3. **Separator rendering**: A 1px line using a full-height glyph quad with squashed UVs might look fine, but could also look blurry due to texture filtering. Alternative: use a 1-pixel-tall quad sampling from the solid glyph region. Will evaluate visually.

4. **Performance**: Building vertex buffers every frame for the overlay is acceptable given the small quad count (< 100 quads typically). No optimization needed for MVP.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
