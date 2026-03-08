---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/selector_overlay.rs
- crates/editor/src/renderer/mod.rs
- crates/editor/src/renderer/status_bar.rs
- crates/editor/src/drain_loop.rs
code_references:
- ref: crates/editor/src/selector_overlay.rs#StatusBarState
  implements: "State struct for passing status message to renderer"
- ref: crates/editor/src/selector_overlay.rs#StatusBarGeometry
  implements: "Geometry calculation for status bar positioning"
- ref: crates/editor/src/selector_overlay.rs#StatusBarGlyphBuffer
  implements: "Glyph buffer for status bar rendering"
- ref: crates/editor/src/selector_overlay.rs#calculate_status_bar_geometry
  implements: "Pure geometry calculation function for full-viewport status bar"
- ref: crates/editor/src/selector_overlay.rs#calculate_status_bar_geometry_in_pane
  implements: "Pane-aware geometry calculation for multi-pane layouts"
- ref: crates/editor/src/renderer/status_bar.rs#Renderer::draw_status_bar
  implements: "Render method for status bar"
- ref: crates/editor/src/renderer/status_bar.rs#Renderer::draw_status_bar_in_pane
  implements: "Render method for multi-pane status bar"
- ref: crates/editor/src/drain_loop.rs#DrainLoop::render_if_dirty
  implements: "Integration point calling current_status_message()"
narrative: null
investigation: cross_file_goto_definition
subsystems:
  - subsystem_id: renderer
    relationship: implements
friction_entries: []
bug_type: null
depends_on: []
created_after:
- alt_screen_viewport_reset
---
# Chunk Goal

## Minor Goal

Render the status message from `EditorState::current_status_message()` in the editor UI. Currently, status messages like "Definition not found", "Indexing workspace...", "No identifier at cursor", and "Symbol index not initialized" are stored in `EditorState.status_message` but never drawn — `current_status_message()` is not called from anywhere in the render pipeline.

The status message should be rendered as a transient overlay or inline text in the editor chrome (e.g., bottom of the viewport, similar to VS Code's status bar messages). Messages auto-expire after 2 seconds (the `StatusMessage::is_expired()` mechanism already exists), so the rendering just needs to check `current_status_message()` each frame and draw it when present.

## Success Criteria

- `current_status_message()` is called from the render loop
- When a status message is set (e.g., "Definition not found" after a failed go-to-definition), the text is visibly rendered in the editor
- When the message expires (2 seconds), it disappears from the UI
- The status text does not obscure editable content or interfere with the selector overlay
- When the find-in-file mini buffer is displayed, it takes precedence — the status message is hidden or not rendered
