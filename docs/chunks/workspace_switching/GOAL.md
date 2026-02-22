---
status: ACTIVE
ticket: null
parent_chunk: workspace_model
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Y-coordinate flip for left rail workspace tile hit-testing"
  - ref: crates/editor/src/editor_state.rs#EditorState::next_workspace
    implements: "Cmd+] workspace cycling (next workspace with wrap)"
  - ref: crates/editor/src/editor_state.rs#EditorState::prev_workspace
    implements: "Cmd+[ workspace cycling (previous workspace with wrap)"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key
    implements: "Cmd+[ and Cmd+] keyboard shortcut binding for workspace cycling"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tab_bar_content_clip
- click_scroll_fraction_alignment
---

# Chunk Goal

## Minor Goal

Workspaces created with Cmd+N cannot be switched to. There are two bugs:

1. **Left rail click hit-testing uses unflipped y-coordinates.** In `EditorState::handle_mouse`, `mouse_y` is a raw NSView coordinate (y=0 at the bottom of the window). `calculate_left_rail_geometry` produces tile rects whose y-coordinates run top-down from `TOP_MARGIN` (y=8). Passing raw `mouse_y` to `tile_rect.contains()` means clicks on workspace tiles near the top of the window are never matched — the coordinate spaces are inverted.

2. **No Cmd+[ / Cmd+] shortcut for workspace cycling.** `Cmd+Shift+[` / `Cmd+Shift+]` already cycle tabs. `Cmd+[` / `Cmd+]` (without Shift) are unbound. These should cycle to the previous/next workspace, mirroring the pattern used for tab cycling.

## Success Criteria

- Clicking a workspace tile in the left rail switches the active workspace.
  - Fix: flip the y-coordinate before hit-testing: `let flipped_y = self.view_height - mouse_y as f32` and use `tile_rect.contains(mouse_x as f32, flipped_y)`.
- `Cmd+[` cycles to the previous workspace (wraps from first to last).
- `Cmd+]` cycles to the next workspace (wraps from last to first).
- Cmd+1..9 direct workspace switching continues to work as before.
- Unit tests cover the y-flip hit-test logic and the prev/next workspace wrapping.

## Relationship to Parent

The `workspace_model` chunk implemented the workspace data model, left rail UI, and declared `EditorState::handle_mouse` as its workspace-switching entry point. The click hit-test code was written but contains a coordinate system bug: the y-coordinate is not flipped before hit-testing against geometry that is computed in top-down screen space. The Cmd+[] keyboard shortcuts were also not added during that chunk. This chunk fixes both omissions.