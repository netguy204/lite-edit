---
status: HISTORICAL
ticket: null
parent_chunk: workspace_model
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_buffer
    implements: "Coordinate adjustment subtracting RAIL_WIDTH from x position"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- renderer_styled_content
- terminal_emulator
- workspace_model
- mini_buffer_model
---

# Chunk Goal

## Minor Goal

Fix the click-to-position-cursor regression introduced by the `workspace_model` chunk. Since the workspace model added the left rail (`RAIL_WIDTH = 56px`), `handle_mouse_buffer` forwards the raw window-coordinate mouse event to `focus_target.handle_mouse` without adjusting the x position. The buffer's column calculation treats x as content-area-relative, so every click lands roughly `RAIL_WIDTH / glyph_width` columns (~7–8 cols) to the right of where the user actually clicked.

The fix is a one-line correction in `handle_mouse_buffer`: subtract `RAIL_WIDTH` from the event's x position before forwarding it to the buffer handler, so the buffer receives a content-area-relative coordinate.

## Success Criteria

- Clicking at any visible column in the content area places the cursor at that column, not ~7–8 columns to the right
- Clicking near the left edge of the content area (immediately right of the rail) no longer places the cursor off-screen to the right
- No other mouse paths (selector overlay, left-rail tile clicks) are affected by this change
- Existing mouse handler tests continue to pass

## Relationship to Parent

The `workspace_model` chunk added the left rail and correctly gated rail clicks vs. content-area clicks in `handle_mouse`. However, it did not adjust the event x position when forwarding content-area clicks to `handle_mouse_buffer`. That omission is the sole cause of this bug. Everything else the parent chunk implemented remains correct.
