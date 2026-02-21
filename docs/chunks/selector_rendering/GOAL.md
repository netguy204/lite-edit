---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/selector_overlay.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/selector_overlay.rs#calculate_overlay_geometry
    implements: "Pure geometry calculation for overlay panel positioning and sizing"
  - ref: crates/editor/src/selector_overlay.rs#OverlayGeometry
    implements: "Computed geometry struct for overlay panel layout"
  - ref: crates/editor/src/selector_overlay.rs#SelectorGlyphBuffer
    implements: "Vertex/index buffer management for selector overlay rendering"
  - ref: crates/editor/src/selector_overlay.rs#SelectorGlyphBuffer::update_from_widget
    implements: "Builds vertex data for background, selection, separator, query, and items"
  - ref: crates/editor/src/renderer.rs#Renderer::render_with_selector
    implements: "Main entry point for rendering with optional selector overlay"
  - ref: crates/editor/src/renderer.rs#Renderer::draw_selector_overlay
    implements: "Draws the selector panel elements with appropriate colors"
narrative: file_buffer_association
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- selector_widget
created_after:
- delete_to_line_start
- ibeam_cursor
---

# Selector Overlay Rendering

## Minor Goal

Add Metal rendering for a `SelectorWidget` as a floating panel overlay. When a selector is active, it is drawn on top of the editor buffer — the editor content remains visible underneath but is visually de-emphasised by the overlay background. This chunk makes the selector interactive and visible; it connects the model from `selector_widget` to the existing glyph atlas and Metal rendering pipeline.

## Success Criteria

- **Panel geometry**: the overlay is a rectangle centered horizontally in the window, with:
  - Width: 60% of the window width (minimum 400px if the window is large).
  - Height: dynamic — one row for the query input plus one row per item, capped at 50% of the window height. Items beyond the cap are not rendered (scrolling within the list is out of scope for this chunk).
  - Vertically positioned in the upper third of the window (e.g., top edge at 20% of window height).

- **Background**: an opaque filled rectangle drawn behind all text, using a distinct background colour (e.g., dark grey `#2a2a2a`) so it visually separates from the editor.

- **Query row**: the first row inside the panel renders the widget's `query` string using the glyph atlas, with a simple blinking cursor appended (reuse the existing cursor-blink timer from `EditorState`). A visual separator (e.g., a 1px horizontal line) divides the query row from the item list.

- **Item list**: each item string is rendered as a single line of text using the glyph atlas. The selected item's row has a highlight background (e.g., accent colour `#0050a0`). Items are clipped to the panel width — long names are truncated (no ellipsis required; just clip).

- **Dirty region integration**: the renderer marks `DirtyRegion::FullViewport` whenever the overlay opens, closes, or its selected item changes, so the overlay and the editor content beneath are both redrawn correctly.

- **Renderer API**: add a method such as `Renderer::draw_selector_overlay(widget: &SelectorWidget, view_width: f32, view_height: f32)` (or equivalent) called from the main render path when a selector is active. The existing editor content is drawn first, then the overlay on top.

- **No new test infrastructure required** — the renderer is inherently visual. A manual smoke test (open the picker, see the panel, navigate items) is sufficient for this chunk. The model-layer is tested in `selector_widget`.
