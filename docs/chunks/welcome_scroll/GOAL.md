---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/welcome_screen.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/renderer.rs
code_references:
  - ref: crates/editor/src/welcome_screen.rs#calculate_welcome_geometry
    implements: "Accepts scroll_offset_px parameter; clamps to [0, max_scroll] and subtracts from content_y for vertical scrolling"
  - ref: crates/editor/src/welcome_screen.rs#calculate_content_dimensions
    implements: "Made pub(crate) to expose deterministic content dimensions for scroll clamping"
  - ref: crates/editor/src/workspace.rs#Tab
    implements: "Holds welcome_scroll_offset_px field tracking per-tab welcome screen scroll state"
  - ref: crates/editor/src/workspace.rs#Tab::welcome_scroll_offset_px
    implements: "Getter for the welcome screen vertical scroll offset in pixels"
  - ref: crates/editor/src/workspace.rs#Tab::set_welcome_scroll_offset_px
    implements: "Setter enforcing lower bound (>= 0); upper bound is deferred to render-time clamping"
  - ref: crates/editor/src/workspace.rs#Editor::welcome_scroll_offset_px
    implements: "Convenience accessor for the renderer to read active tab's welcome scroll offset"
  - ref: crates/editor/src/editor_state.rs#EditorState::scroll_pane
    implements: "Routes scroll events on empty file tabs to the welcome scroll offset rather than buffer viewport"
  - ref: crates/editor/src/renderer.rs#Renderer::draw_welcome_screen
    implements: "Accepts scroll_offset_px and forwards it to calculate_welcome_geometry for single-pane rendering"
  - ref: crates/editor/src/renderer.rs#Renderer::draw_welcome_screen_in_pane
    implements: "Accepts scroll_offset_px and forwards it to calculate_welcome_geometry for multi-pane rendering"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after: ["generic_yes_no_modal", "terminal_resize_sync"]
---


# Chunk Goal

## Minor Goal

The welcome screen (`welcome_screen.rs`) currently centers its content within the viewport and
clamps the top-left origin to `(0, 0)`, so content is simply clipped when the viewport is
smaller than the total content height. There is no way to see the bottom of the hotkey table
on a short window.

This chunk adds vertical scrolling to the welcome screen so users can scroll through the full
intro content regardless of viewport height. Horizontal scrolling is not needed because the
content reflows (clamps at `x = 0`), and the content width is modest.

## Success Criteria

- Scrolling (mouse wheel / trackpad scroll events) on the welcome screen moves the content
  up and down, exactly as it does for buffer content.
- A scroll offset (in pixels or lines) is tracked per-pane for the welcome screen state, reset
  to 0 whenever a new blank tab becomes active.
- The content never scrolls past the top (offset ≥ 0) or past the bottom (offset ≤
  max_scroll, where max_scroll = max(0, content_height_px − viewport_height_px)).
- When the viewport is taller than the content the welcome screen remains centered exactly as
  before — the new scroll path is only exercised when content overflows.
- No regression: the welcome screen continues to disappear when the user starts typing or
  opens a file.

## Rejected Ideas

<!-- none yet -->