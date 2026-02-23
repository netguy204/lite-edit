<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk updates the renderer to support multiple panes in a tiled layout. The core insight is that the existing renderer already has many of the building blocks we need:

1. **`calculate_pane_rects()`** from `pane_layout.rs` computes screen rectangles for each pane in the tree
2. **Metal scissor rects** are already used for clipping (selector overlay, buffer content)
3. **Separate glyph buffers** for different UI components (left rail, tab bar, selector) demonstrate the pattern

The strategy is to **iterate over panes and render each independently**, applying scissor rects to confine each pane's content to its bounds. Key design decisions:

- **One tab bar per pane**: Each pane has its own tab bar (using the pane's tabs), positioned at the top of the pane rect
- **Content offset is pane-local**: Each pane's content area starts below its tab bar, not the global tab bar
- **Left rail is global**: Rendered once for the whole window (not per-pane), same as today
- **Divider lines**: Rendered as thin quads at pane boundaries
- **Focus indicator**: The focused pane gets a colored accent border or subtle tint

The implementation follows the Humble View Architecture: geometry calculations are pure functions, GPU buffer construction is separate, and rendering uses the computed geometry. This allows unit testing of layout logic without Metal dependencies.

### Coordinate Pipeline

The `tiling_workspace_integration` chunk established a coordinate pipeline where:
1. Y is flipped once at `handle_mouse` entry
2. Hit-testing uses `PaneRect` values in screen space
3. Pane-local coordinates are computed at dispatch time

This chunk follows the same pattern for rendering:
1. `calculate_pane_rects()` produces screen-space rectangles
2. Each pane's content renderer receives pane-local dimensions
3. `GlyphBuffer` offsets are set per-pane before updating

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem. Each pane's tab eventually has its own `Viewport` for scroll state. The renderer will read each pane's active tab's viewport during rendering. No changes to subsystem internals required.

## Sequence

### Step 1: Add divider color constant to color_palette.rs or renderer.rs

Define a color constant for pane dividers. Choose a subtle border color from the Catppuccin Mocha palette (e.g., `surface0` or `overlay0`).

```rust
/// Pane divider color: #313244 (Catppuccin Mocha surface0)
/// A subtle line between adjacent panes.
const PANE_DIVIDER_COLOR: [f32; 4] = [
    0.192, // 0x31 / 255
    0.196, // 0x32 / 255
    0.267, // 0x44 / 255
    1.0,
];
```

Location: `crates/editor/src/renderer.rs`

### Step 2: Add focused pane border color constant

Define a color constant for the focused pane indicator. This could be a brighter accent color (e.g., Catppuccin `blue` or `lavender`) at reduced opacity.

```rust
/// Focused pane border color: #89b4fa at 60% (Catppuccin Mocha blue)
const FOCUSED_PANE_BORDER_COLOR: [f32; 4] = [
    0.537, // 0x89 / 255
    0.706, // 0xb4 / 255
    0.980, // 0xfa / 255
    0.6,
];
```

Location: `crates/editor/src/renderer.rs`

### Step 3: Create PaneRenderingBuffer for per-pane glyph data

Create a new struct that manages vertex/index buffers for pane-level UI elements: dividers and focus borders. This is analogous to `LeftRailGlyphBuffer` and `TabBarGlyphBuffer`.

```rust
/// Manages vertex and index buffers for pane frame rendering.
///
/// Renders:
/// - Divider lines between panes (1px)
/// - Focus border around the active pane
pub struct PaneFrameBuffer {
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_count: usize,
    divider_range: QuadRange,
    focus_border_range: QuadRange,
}

impl PaneFrameBuffer {
    pub fn new() -> Self { ... }

    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        pane_rects: &[PaneRect],
        focused_pane_id: PaneId,
        atlas: &GlyphAtlas,  // For solid_glyph
    ) { ... }
}
```

Location: `crates/editor/src/pane_frame_buffer.rs` (new file)

### Step 4: Implement calculate_divider_lines() helper

Create a pure function that, given a list of `PaneRect` values, returns line segments for dividers. Dividers are placed at boundaries between adjacent panes.

```rust
/// A divider line between two adjacent panes.
pub struct DividerLine {
    pub x: f32,
    pub y: f32,
    pub width: f32,  // For vertical dividers, this is 1.0
    pub height: f32, // For horizontal dividers, this is 1.0
}

/// Calculates divider lines from pane rectangles.
///
/// Dividers appear at the boundary between adjacent panes.
/// Deduplicates overlapping dividers at shared edges.
pub fn calculate_divider_lines(pane_rects: &[PaneRect]) -> Vec<DividerLine> {
    // For each pair of panes, check if they share an edge
    // If they share a vertical edge: add a vertical divider at that x
    // If they share a horizontal edge: add a horizontal divider at that y
    // Deduplicate lines that are at the same position
}
```

This function should be unit-testable without Metal dependencies.

Location: `crates/editor/src/pane_frame_buffer.rs`

### Step 5: Implement PaneFrameBuffer::update()

Build vertex data for:
1. **Divider lines**: 1px wide/tall quads at pane boundaries using `PANE_DIVIDER_COLOR`
2. **Focus border**: 2px border around the focused pane using `FOCUSED_PANE_BORDER_COLOR`

Use the solid glyph from the atlas for solid-color rendering.

Location: `crates/editor/src/pane_frame_buffer.rs`

### Step 6: Create pane_scissor_rect() helper function

Create a helper function that generates a Metal scissor rect from a `PaneRect`. This will be used to clip each pane's rendering.

```rust
/// Creates a scissor rect for clipping a pane's content.
///
/// The rect covers the pane's bounds, constrained to the viewport.
fn pane_scissor_rect(
    pane_rect: &PaneRect,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    // Clamp to viewport bounds
    let x = (pane_rect.x as usize).min(view_width as usize);
    let y = (pane_rect.y as usize).min(view_height as usize);
    let right = ((pane_rect.x + pane_rect.width) as usize).min(view_width as usize);
    let bottom = ((pane_rect.y + pane_rect.height) as usize).min(view_height as usize);

    MTLScissorRect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}
```

Location: `crates/editor/src/renderer.rs`

### Step 7: Add pane_frame_buffer field to Renderer

Add an optional `PaneFrameBuffer` to the `Renderer` struct, similar to how `left_rail_buffer` and `tab_bar_buffer` are managed.

```rust
pub struct Renderer {
    // ... existing fields ...

    /// The glyph buffer for pane dividers and focus borders (lazy-initialized)
    pane_frame_buffer: Option<PaneFrameBuffer>,
}
```

Location: `crates/editor/src/renderer.rs`

### Step 8: Add draw_pane_frames() method to Renderer

Implement a method that renders divider lines and the focus border. This should be called after all pane content is rendered but before overlays.

```rust
fn draw_pane_frames(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    pane_rects: &[PaneRect],
    focused_pane_id: PaneId,
) {
    // Ensure buffer is initialized
    if self.pane_frame_buffer.is_none() {
        self.pane_frame_buffer = Some(PaneFrameBuffer::new());
    }

    // Update buffer with current pane layout
    let buffer = self.pane_frame_buffer.as_mut().unwrap();
    buffer.update(&self.device, pane_rects, focused_pane_id, &self.atlas);

    // Draw dividers
    // Draw focus border
}
```

Location: `crates/editor/src/renderer.rs`

### Step 9: Refactor render_with_editor() for multi-pane rendering

This is the main rendering change. The current `render_with_editor` assumes a single content area. Refactor to:

1. Get pane rects from the workspace's pane tree via `calculate_pane_rects()`
2. Draw the left rail once (unchanged)
3. For each pane rect:
   a. Apply scissor rect for this pane
   b. Draw the pane's tab bar (at top of pane rect, using pane's tabs)
   c. Apply content scissor (below tab bar, within pane)
   d. Draw the pane's content (buffer/terminal/welcome screen)
4. Reset scissor to full viewport
5. Draw pane frames (dividers + focus border)
6. Draw selector overlay if active

```rust
pub fn render_with_editor(
    &mut self,
    view: &MetalView,
    editor: &Editor,
    selector: Option<&SelectorWidget>,
    selector_cursor_visible: bool,
) {
    // ... drawable setup unchanged ...

    // Render left rail (once for whole window)
    self.draw_left_rail(&encoder, view, editor);

    // Get pane rects from workspace
    let workspace = match editor.active_workspace() {
        Some(ws) => ws,
        None => { /* handle no workspace case */ }
    };

    let content_bounds = (RAIL_WIDTH, 0.0, view_width - RAIL_WIDTH, view_height);
    let pane_rects = calculate_pane_rects(content_bounds, &workspace.pane_root);
    let pane_count = pane_rects.len();

    // Single-pane case: render as before (no dividers, no focus border)
    if pane_count == 1 {
        self.render_single_pane(&encoder, view, workspace, &pane_rects[0]);
    } else {
        // Multi-pane case: iterate and render each pane
        for pane_rect in &pane_rects {
            self.render_pane(&encoder, view, workspace, pane_rect);
        }

        // Draw pane frames (dividers + focus border)
        self.draw_pane_frames(&encoder, view, &pane_rects, workspace.active_pane_id);
    }

    // Reset scissor and draw overlay
    encoder.setScissorRect(full_viewport_scissor_rect(view_width, view_height));
    if let Some(widget) = selector {
        self.draw_selector_overlay(&encoder, view, widget, selector_cursor_visible);
    }

    // ... present drawable unchanged ...
}
```

Location: `crates/editor/src/renderer.rs`

### Step 10: Implement render_pane() helper method

Create a method that renders a single pane's content within its rectangle:

```rust
fn render_pane(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    workspace: &Workspace,
    pane_rect: &PaneRect,
) {
    let pane = match workspace.pane_root.get_pane(pane_rect.pane_id) {
        Some(p) => p,
        None => return,
    };

    // Apply scissor for entire pane
    let pane_scissor = pane_scissor_rect(pane_rect, view_width, view_height);
    encoder.setScissorRect(pane_scissor);

    // Draw this pane's tab bar at top of pane rect
    self.draw_pane_tab_bar(&encoder, view, pane, pane_rect);

    // Apply content scissor (below tab bar)
    let content_scissor = pane_content_scissor_rect(pane_rect, TAB_BAR_HEIGHT, view_width, view_height);
    encoder.setScissorRect(content_scissor);

    // Get the active tab's buffer view
    if let Some(tab) = pane.active_tab() {
        // Set content offsets for this pane
        self.glyph_buffer.set_x_offset(pane_rect.x);
        self.glyph_buffer.set_y_offset(pane_rect.y + TAB_BAR_HEIGHT);

        // Update glyph buffer from this tab's buffer
        // ... similar to existing logic but for pane's tab ...

        // Render text
        if self.glyph_buffer.index_count() > 0 {
            self.render_text(&encoder, view);
        }
    }
}
```

Location: `crates/editor/src/renderer.rs`

### Step 11: Create draw_pane_tab_bar() method

This is similar to `draw_tab_bar()` but renders the tab bar for a specific pane at a specific position:

```rust
fn draw_pane_tab_bar(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    pane: &Pane,
    pane_rect: &PaneRect,
) {
    if pane.tab_count() == 0 {
        return;
    }

    // Build TabInfo from pane's tabs
    let tabs: Vec<TabInfo> = pane.tabs.iter().enumerate()
        .map(|(idx, tab)| TabInfo::from_tab(tab, idx, idx == pane.active_tab))
        .collect();

    // Calculate geometry with pane rect bounds
    let geometry = calculate_tab_bar_geometry(
        pane_rect.width,
        &tabs,
        glyph_width,
        pane.tab_bar_view_offset,
    );
    // Offset geometry to pane position
    // geometry.x += pane_rect.x;
    // geometry.y += pane_rect.y;

    // Update and render tab bar buffer
    // ... similar to draw_tab_bar() but using pane geometry ...
}
```

Location: `crates/editor/src/renderer.rs`

### Step 12: Create pane_content_scissor_rect() helper

Similar to `buffer_content_scissor_rect()` but constrained to a pane's bounds:

```rust
/// Creates a scissor rect for a pane's content area (below its tab bar).
fn pane_content_scissor_rect(
    pane_rect: &PaneRect,
    tab_bar_height: f32,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    let x = (pane_rect.x as usize).min(view_width as usize);
    let y = ((pane_rect.y + tab_bar_height) as usize).min(view_height as usize);
    let right = ((pane_rect.x + pane_rect.width) as usize).min(view_width as usize);
    let bottom = ((pane_rect.y + pane_rect.height) as usize).min(view_height as usize);

    MTLScissorRect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}
```

Location: `crates/editor/src/renderer.rs`

### Step 13: Update welcome screen rendering for panes

The welcome screen should render within the focused pane if its active tab is empty. Update `draw_welcome_screen()` to accept pane-local geometry or modify the geometry calculation to account for pane bounds.

```rust
fn draw_welcome_screen_in_pane(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    pane_rect: &PaneRect,
) {
    // Calculate content area within pane (excluding pane's tab bar)
    let content_width = pane_rect.width;
    let content_height = pane_rect.height - TAB_BAR_HEIGHT;

    // Calculate geometry centered in pane
    let mut geometry = calculate_welcome_geometry(
        content_width,
        content_height,
        glyph_width,
        line_height,
    );

    // Offset to pane position
    geometry.content_x += pane_rect.x;
    geometry.content_y += pane_rect.y + TAB_BAR_HEIGHT;

    // Render...
}
```

Location: `crates/editor/src/renderer.rs`

### Step 14: Update wrap_layout() for pane-local width

The `wrap_layout()` method currently uses the global `content_width_px`. For multi-pane rendering, each pane may have different widths. Consider passing pane width or updating `content_width_px` before rendering each pane.

```rust
/// Creates a WrapLayout for a specific pane width.
pub fn wrap_layout_for_width(&self, content_width: f32) -> WrapLayout {
    WrapLayout::new(content_width, &self.font.metrics)
}
```

Location: `crates/editor/src/renderer.rs`

### Step 15: Update selector overlay positioning for focused pane

The selector overlay should position relative to the focused pane, not the full window. Update `draw_selector_overlay()` to accept pane bounds or calculate geometry within pane bounds.

This may require updating `calculate_overlay_geometry()` to take optional pane bounds, or computing the overlay position based on the focused pane's rect.

Location: `crates/editor/src/renderer.rs`, `crates/editor/src/selector_overlay.rs`

### Step 16: Add unit tests for pane layout rendering

Add tests for:
- `calculate_divider_lines()`: verify correct dividers for HSplit, VSplit, nested splits
- `pane_scissor_rect()`: verify clipping bounds
- Single-pane case produces no dividers or focus borders
- Divider deduplication at shared edges

Location: `crates/editor/src/pane_frame_buffer.rs` (test module)

### Step 17: Add integration tests for multi-pane rendering

While full Metal rendering can't be unit tested, verify the logic flow:
- `render_with_editor` iterates all panes when there are multiple
- Each pane gets its own scissor rect
- Pane content offsets are set correctly

These may be sanity checks in the existing test infrastructure.

Location: `crates/editor/src/renderer.rs` (test module)

### Step 18: Update code_paths in GOAL.md

Add the files touched by this implementation to the chunk's code_paths frontmatter:
- `crates/editor/src/renderer.rs`
- `crates/editor/src/pane_frame_buffer.rs` (new)
- `crates/editor/src/pane_layout.rs` (may need minor additions)
- `crates/editor/src/selector_overlay.rs` (if modified for pane positioning)

Location: `docs/chunks/tiling_multi_pane_render/GOAL.md`

## Dependencies

- **tiling_workspace_integration** (ACTIVE): Provides the pane tree model, `calculate_pane_rects()`, and the coordinate pipeline. Must be complete before this chunk.
- **tiling_tree_model** (ACTIVE): Provides `PaneLayoutNode`, `Pane`, `PaneRect` types.
- **tiling_tab_movement** (ACTIVE): Provides `PaneId` and tree operations.

All dependency chunks are already implemented (status: ACTIVE), so all required types and functions are available.

## Risks and Open Questions

1. **Performance with many panes**: Each pane requires its own glyph buffer update and render pass. With many panes (4+), this could impact frame time. Mitigation: Profile and consider shared buffer strategies if needed. Initial implementation prioritizes correctness.

2. **Tab bar buffer reuse**: Currently there's a single `tab_bar_buffer`. With multiple panes each having their own tab bar, we may need multiple buffers or a single buffer rebuilt for each pane per frame. Initial approach: rebuild the single buffer for each pane during iteration.

3. **Scroll state per pane**: Each pane has its own viewport for its active tab. Need to ensure the renderer reads the correct viewport for each pane, not the global renderer viewport.

4. **Focus indicator design**: The exact visual treatment (border color, thickness, position) is an implementation choice. Starting with a 2px colored border on the inside of the focused pane's rect. Can iterate based on visual feedback.

5. **Selector overlay z-order**: The selector draws on top of all panes. Ensure it's not clipped by any pane's scissor rect (reset to full viewport before drawing overlay).

6. **Welcome screen in split**: If the focused pane has an empty tab, the welcome screen should render within that pane's bounds. Need to ensure it doesn't overflow into adjacent panes.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
