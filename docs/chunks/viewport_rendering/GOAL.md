---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/Cargo.toml
  - crates/editor/src/dirty_region.rs
  - crates/editor/src/viewport.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/main.rs
  - crates/editor/tests/viewport_test.rs
  - crates/editor/tests/smoke_test.rs
code_references:
  - ref: crates/editor/src/dirty_region.rs#DirtyRegion
    implements: "Screen-space dirty region enum (None, Lines, FullViewport) with merge semantics"
  - ref: crates/editor/src/dirty_region.rs#DirtyRegion::merge
    implements: "Dirty region merge logic (identity, absorption, range merging)"
  - ref: crates/editor/src/viewport.rs#Viewport
    implements: "Viewport struct with scroll_offset, visible_lines, and line_height"
  - ref: crates/editor/src/viewport.rs#Viewport::visible_range
    implements: "Computes which buffer lines are visible in viewport"
  - ref: crates/editor/src/viewport.rs#Viewport::scroll_to
    implements: "Scrolls viewport to target line with clamping"
  - ref: crates/editor/src/viewport.rs#Viewport::ensure_visible
    implements: "Auto-scroll to keep a line visible"
  - ref: crates/editor/src/viewport.rs#Viewport::buffer_line_to_screen_line
    implements: "Maps buffer coordinates to screen coordinates"
  - ref: crates/editor/src/viewport.rs#Viewport::dirty_lines_to_region
    implements: "Converts buffer-space DirtyLines to screen-space DirtyRegion"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer
    implements: "Viewport-aware buffer rendering (only visible lines)"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Buffer rendering with cursor quad generation"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::create_cursor_quad
    implements: "Cursor rendering as a block quad at cursor position"
  - ref: crates/editor/src/renderer.rs#Renderer
    implements: "Extended to hold Viewport for rendering (no longer owns TextBuffer after renderer_polymorphic_buffer)"
  - ref: crates/editor/src/renderer.rs#Renderer::update_viewport_size
    implements: "Updates viewport when window resizes"
  - ref: crates/editor/src/renderer.rs#Renderer::apply_mutation
    implements: "Converts DirtyLines to DirtyRegion for incremental rendering"
  - ref: crates/editor/src/renderer.rs#Renderer::render_dirty
    implements: "Renders based on dirty region (triggers full redraw if dirty)"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_window
    implements: "Connects TextBuffer to Renderer with viewport size initialization"
narrative: null
investigation: editor_core_architecture
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- text_buffer
- glyph_rendering
created_after: []
---

# Viewport + Buffer-to-Screen Rendering

## Minor Goal

Connect the text buffer to the glyph renderer through a viewport abstraction. This chunk closes the gap identified in the H1 investigation traces: the viewport is core state that maps buffer coordinates to screen coordinates, consumed by the render loop and mutated by commands.

The viewport determines which buffer lines are visible on screen. Given the buffer's content and the viewport's scroll offset, this chunk renders the correct slice of the buffer to the Metal surface. It also introduces the `DirtyRegion` enum from the H3 analysis, so the render loop only re-renders what changed.

A cursor is rendered at the buffer's cursor position (solid block or line cursor). The cursor is a visual element drawn by the renderer based on the cursor position from the text buffer.

After this chunk, programmatic mutations to the buffer (via test code) produce correct visual updates. The next chunk (editable_buffer) connects real keyboard input.

## Success Criteria

- A `Viewport` struct exists with: scroll offset (top visible line), visible line count (derived from window height and line height), and the ability to compute which buffer lines are visible.
- The renderer reads buffer content through the viewport: only lines in `[scroll_offset, scroll_offset + visible_lines)` are rendered.
- A `DirtyRegion` enum is implemented:
  ```rust
  enum DirtyRegion {
      None,
      Lines { from: usize, to: usize },
      FullViewport,
  }
  ```
  Dirty regions merge correctly (e.g., `Lines(3,5)` + `Lines(7,9)` → `Lines(3,9)`; any + `FullViewport` → `FullViewport`).
- When the buffer is mutated programmatically (in a test harness), the dirty region is computed and only the affected screen lines are re-rendered. Full viewport is re-rendered on scroll offset change.
- A cursor is rendered at the correct buffer position as a visible indicator (blinking not required yet — that's chunk 5).
- The window displays a buffer pre-loaded with at least 100 lines of text, with the viewport starting at line 0. Changing the scroll offset programmatically shows different slices of the buffer.
- Line numbers or a left gutter are NOT required (they're a future concern). Just text content and cursor.
- Rendering through the viewport adds negligible overhead compared to the hardcoded rendering in `glyph_rendering` — the viewport is just an offset into the buffer's line array.