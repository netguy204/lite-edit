---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/drain_loop.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Multi-pane click routing - routes clicks to any pane's tab bar, not just top-left"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::update_cursor_regions
    implements: "Multi-pane cursor regions - adds pointer cursor for each pane's tab bar"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- dragdrop_file_paste
- vsplit_scroll
- workspace_initial_terminal
- workspace_session_persistence
---

# Chunk Goal

## Minor Goal

Fix two related bugs that make tabs in non-top-left panes unresponsive after
splitting. When a pane is split horizontally or vertically, panes below or to
the right of the top-left pane have their tab bars at y-coordinates greater
than `TAB_BAR_HEIGHT`. Two independent assumptions in the code break:

1. **Click routing** (`editor_state.rs` — `handle_mouse`): The gate
   `if screen_y < TAB_BAR_HEIGHT` routes a click to `handle_tab_bar_click`
   only when the y-coordinate is within the top `TAB_BAR_HEIGHT` pixels of
   the window. Tab bars for any other pane (e.g., the bottom pane in a
   vertical split with its tab bar at y=300) are above this threshold, so
   their clicks fall through to the buffer handler and are silently discarded.

2. **Cursor pointer regions** (`drain_loop.rs` — `update_cursor_regions`):
   A single pointer cursor rect is added covering only the strip at
   `y = view_height − TAB_BAR_HEIGHT` (the top-left pane's tab bar). In
   split layouts, the other panes' tab bars are at different y positions and
   receive no pointer rect, so the cursor stays as an I-beam over them.

Both defects stem from the same assumption: that there is exactly one tab bar
at the top of the content area. The fix must enumerate all pane rects and
treat each pane's top `TAB_BAR_HEIGHT` strip as a tab bar.

## Success Criteria

- Clicking a tab in any pane (top-left, top-right, bottom-left, bottom-right,
  or any pane in a deeper split) switches that pane to the clicked tab.
  Correct behaviour was already established by `split_tab_click` —
  `handle_tab_bar_click` itself is correct — only the routing to it is broken.
- The mouse pointer changes to an arrow (pointer) cursor when hovering over
  the tab bar strip of any pane in a split layout, not just the top-left pane.
- Single-pane layouts are unaffected (no regression).
- Existing `split_tab_click` unit tests continue to pass.
- New unit tests cover click routing through `handle_mouse` (not just
  `handle_tab_bar_click`) for non-top-left panes in both horizontal and
  vertical splits, verifying the full dispatch path.

## Relationship to Parent

The `split_tab_click` chunk fixed `handle_tab_bar_click` so that once a click
is routed to it, the correct pane's active tab is updated. This chunk fixes
the upstream routing so that clicks in non-top-left pane tab bars actually
reach `handle_tab_bar_click` in the first place.
