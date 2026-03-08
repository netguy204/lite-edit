# Implementation Plan

## Approach

Render transient status messages (e.g., "Definition not found", "Indexing workspace...") as a lightweight text overlay at the bottom of the viewport. The approach follows the existing overlay rendering pattern established by `FindStripGlyphBuffer`: create a geometry struct, a glyph buffer, and a render method that draws text on a semi-transparent background.

**Key design decisions:**

1. **Status message renders similarly to find strip** — Use the same bottom-of-viewport placement as the find strip, but simpler (no query input, no cursor). The status message is display-only.

2. **Find strip takes precedence** — When the find-in-file strip is visible (`FocusLayer::FindInFile`), status messages are not rendered. This avoids visual clutter and respects the user's active interaction.

3. **Selector overlay takes precedence** — Similarly, when a selector is active (`FocusLayer::Selector`), status messages are hidden since the overlay covers the bottom of the screen.

4. **Single-pane and multi-pane aware** — In single-pane mode, the status message spans the full content width. In multi-pane mode, it appears in the focused pane only, similar to how find strip behaves.

5. **Reuse existing glyph buffer patterns** — Follow the `FindStripGlyphBuffer` structure: lazy-initialized buffer, `update()` method that builds vertices, render method that issues Metal draw calls. This follows the renderer subsystem patterns.

6. **Pure geometry calculation** — Keep `calculate_status_bar_geometry()` as a pure function for testability per Humble View Architecture.

**Testing strategy:**

Per TESTING_PHILOSOPHY.md, the testable components are:
- `StatusMessage::is_expired()` — already tested
- `EditorState::current_status_message()` — already tested
- Geometry calculation — pure function, unit testable
- Integration: calling `current_status_message()` from render path — structural wiring

The Metal rendering itself is a humble view (not unit tested) but the geometry math and state management are fully testable.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk IMPLEMENTS a new overlay rendering feature following the subsystem's patterns for glyph buffers and render methods. The implementation will add `StatusBarGlyphBuffer` alongside existing overlay buffers and integrate into `render_with_editor`.

## Sequence

### Step 1: Add StatusBarState struct for passing status to renderer

Create a struct similar to `FindStripState` that holds the status message text for rendering.

**Location**: `crates/editor/src/selector_overlay.rs`

```rust
// Chunk: docs/chunks/gotodef_status_render - Status bar state for render pass
/// State needed to render the status bar overlay.
pub struct StatusBarState<'a> {
    /// The status message text to display
    pub text: &'a str,
}
```

### Step 2: Add StatusBarGeometry and calculate_status_bar_geometry()

Create a geometry struct and pure calculation function for status bar positioning.

**Location**: `crates/editor/src/selector_overlay.rs`

The status bar is positioned at the bottom of the viewport (or pane in multi-pane mode), same as find strip. It is one line tall with padding.

```rust
// Chunk: docs/chunks/gotodef_status_render - Status bar geometry
#[derive(Debug, Clone, Copy)]
pub struct StatusBarGeometry {
    pub strip_x: f32,
    pub strip_y: f32,
    pub strip_width: f32,
    pub strip_height: f32,
    pub text_x: f32,
    pub text_y: f32,
    pub line_height: f32,
}

pub fn calculate_status_bar_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
) -> StatusBarGeometry
```

### Step 3: Add StatusBarGlyphBuffer

Create a glyph buffer for rendering the status bar. This follows the `FindStripGlyphBuffer` pattern: persistent vertex/index buffers, background quad, text glyphs.

**Location**: `crates/editor/src/selector_overlay.rs`

```rust
// Chunk: docs/chunks/gotodef_status_render - Status bar glyph buffer
pub struct StatusBarGlyphBuffer {
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    index_count: usize,
    layout: GlyphLayout,
    persistent_vertices: Vec<GlyphVertex>,
    persistent_indices: Vec<u32>,
}

impl StatusBarGlyphBuffer {
    pub fn new(layout: GlyphLayout) -> Self;
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>>;
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>>;
    pub fn index_count(&self) -> usize;
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        text: &str,
        geometry: &StatusBarGeometry,
    );
}
```

The `update()` method builds:
1. Background rect (same color as overlays: `OVERLAY_BACKGROUND_COLOR`)
2. Text glyphs (white text, Catppuccin Mocha text color)

### Step 4: Add status_bar_buffer field to Renderer

Add the lazy-initialized status bar buffer to the Renderer struct.

**Location**: `crates/editor/src/renderer/mod.rs`

Add field:
```rust
/// The glyph buffer for status bar rendering (lazy-initialized)
// Chunk: docs/chunks/gotodef_status_render - Status bar buffer
status_bar_buffer: Option<StatusBarGlyphBuffer>,
```

Initialize in `Renderer::new()` as `None`.

### Step 5: Add draw_status_bar method to Renderer

Create the render method that draws the status bar.

**Location**: `crates/editor/src/renderer/` — create new file `status_bar.rs` or add to `find_strip.rs`

Following the renderer decomposition pattern, create `crates/editor/src/renderer/status_bar.rs`:

```rust
// Chunk: docs/chunks/gotodef_status_render - Status bar rendering

impl Renderer {
    pub(super) fn draw_status_bar(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        text: &str,
    );
}
```

The method:
1. Gets view dimensions
2. Calculates geometry via `calculate_status_bar_geometry()`
3. Lazy-initializes `status_bar_buffer` if `None`
4. Calls `update()` on the buffer
5. Issues Metal draw calls (same pattern as `draw_find_strip`)

### Step 6: Integrate status bar into render_with_editor

Modify `render_with_editor` to call `draw_status_bar` when there is a status message and no overlay is active.

**Location**: `crates/editor/src/renderer/mod.rs`

Add a new parameter to `render_with_editor`:
```rust
pub fn render_with_editor(
    &mut self,
    view: &MetalView,
    editor: &Editor,
    selector: Option<&SelectorWidget>,
    selector_cursor_visible: bool,
    find_strip: Option<FindStripState<'_>>,
    status_bar: Option<StatusBarState<'_>>,  // NEW
)
```

**Render logic:**
- If `selector.is_some()` → draw selector overlay (status bar hidden)
- Else if `find_strip.is_some()` → draw find strip (status bar hidden)
- Else if `status_bar.is_some()` → draw status bar
- The status bar renders in the same position as find strip, so they are mutually exclusive

For single-pane mode, draw after content but before selector overlay.
For multi-pane mode, draw in the focused pane's bounds (add `draw_status_bar_in_pane` similar to `draw_find_strip_in_pane`).

### Step 7: Call current_status_message() from drain_loop render path

Modify the drain_loop to pass the status message to `render_with_editor`.

**Location**: `crates/editor/src/drain_loop.rs`

In `render_if_dirty()`, for the `FocusLayer::Buffer | FocusLayer::GlobalShortcuts` branch:

```rust
FocusLayer::Buffer | FocusLayer::GlobalShortcuts => {
    self.renderer.set_cursor_visible(self.state.cursor_visible);
    // Chunk: docs/chunks/gotodef_status_render - Pass status message to renderer
    let status_msg = self.state.current_status_message();
    let status_bar = status_msg.map(|text| StatusBarState { text });
    self.renderer.render_with_editor(
        &self.metal_view,
        &self.state.editor,
        None,
        self.state.cursor_visible,
        None, // No find strip
        status_bar,
    );
}
```

Note: `current_status_message()` takes `&mut self` because it clears expired messages. This is called once per frame, which is correct.

### Step 8: Update other render_with_editor call sites

Update all call sites of `render_with_editor` to pass `None` for status_bar when an overlay is active:

**Locations:**
- `FocusLayer::Selector` branch → `status_bar: None`
- `FocusLayer::FindInFile` branch → `status_bar: None`
- `FocusLayer::ConfirmDialog` → uses `render_with_confirm_dialog`, no change needed

Also update `render_with_confirm_dialog` signature if needed (likely pass `None` for status_bar when calling internal render logic).

### Step 9: Add draw_status_bar_in_pane for multi-pane support

For multi-pane mode, status bar should appear within the focused pane's bounds.

**Location**: `crates/editor/src/renderer/status_bar.rs`

```rust
pub(super) fn draw_status_bar_in_pane(
    &mut self,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    view: &MetalView,
    text: &str,
    pane_rect: &PaneRect,
    view_width: f32,
    view_height: f32,
);
```

Add `calculate_status_bar_geometry_in_pane()` similar to `calculate_find_strip_geometry_in_pane()`.

Update `render_with_editor` multi-pane branch to call this.

### Step 10: Add unit tests

Add tests for the geometry calculation function.

**Location**: `crates/editor/src/selector_overlay.rs` (test module)

```rust
#[test]
fn test_status_bar_geometry_positions_at_bottom() {
    let geom = calculate_status_bar_geometry(1000.0, 800.0, 20.0);
    // Status bar at bottom
    assert!(geom.strip_y > 700.0);
    assert!(geom.strip_y + geom.strip_height <= 800.0);
}

#[test]
fn test_status_bar_geometry_in_pane() {
    let geom = calculate_status_bar_geometry_in_pane(
        100.0, 0.0, 400.0, 400.0,
        20.0, 1000.0, 800.0,
    );
    // Should be within pane bounds
    assert!(geom.strip_x >= 100.0);
    assert!(geom.strip_x + geom.strip_width <= 500.0);
}
```

### Step 11: Verify integration manually

Run the editor and test:
1. Trigger "Definition not found" by using go-to-definition on an unknown symbol
2. Verify the message appears at the bottom of the viewport
3. Verify the message disappears after 2 seconds
4. Open find-in-file (Cmd+F) while status message is visible → status message should hide
5. Open file picker while status message is visible → status message should hide
6. In multi-pane mode, verify status bar appears in focused pane only

## Risks and Open Questions

1. **Mutable borrow conflict**: `current_status_message()` takes `&mut self` on `EditorState`. In `drain_loop`, we already have `&mut self.state`. Need to ensure the status message string is extracted before calling render methods that might also need `&self.state`. Solution: Extract to a local `Option<String>` before the render call.

2. **Tab bar height**: In single-pane mode, the content area is offset by `TAB_BAR_HEIGHT`. Need to ensure status bar geometry accounts for this (render within content area, not overlapping tab bar). The find strip already handles this, so follow its pattern.

3. **Text truncation**: Long status messages might overflow the viewport width. Decision: truncate with ellipsis if needed, similar to how find strip handles long queries.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->
