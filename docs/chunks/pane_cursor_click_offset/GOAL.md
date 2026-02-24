---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/pane_layout.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/pane_layout.rs#HitZone
    implements: "Enum distinguishing tab bar vs content zone hits"
  - ref: crates/editor/src/pane_layout.rs#PaneHit
    implements: "Struct containing pane hit result with pane-local coordinates"
  - ref: crates/editor/src/pane_layout.rs#resolve_pane_hit
    implements: "Shared pane hit resolution using renderer-consistent bounds"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_buffer
    implements: "Fixed coordinate transformation using resolve_pane_hit for non-primary panes"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Tab bar click routing using resolve_pane_hit"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- dragdrop_file_paste
- vsplit_scroll
- workspace_initial_terminal
- workspace_session_persistence
---

# Chunk Goal

## Minor Goal

Fix cursor positioning when clicking in non-primary panes of a split layout,
and introduce a shared pane coordinate resolution function that eliminates the
coordinate-space mismatch across mouse handling code.

**The bug:** Mouse clicks in the right pane of a vertical split place the
cursor shifted to the right. Clicks in the bottom pane of a horizontal split
place the cursor shifted downward. Only the top-left pane positions correctly.

**Root cause:** `handle_mouse_buffer` computes pane rects using bounds
`(0, 0, W, H−TAB_BAR_HEIGHT)` — a content-local coordinate space — but the
renderer uses `(RAIL_WIDTH, 0, W, H)` — screen space. When the clicked pane
is found, the code passes content-area-global coordinates to the buffer hit
test without subtracting the pane's origin. For the top-left pane (origin 0,0)
the error is zero; for any other pane the offset is wrong.

**The refactoring:** The `pane_tabs_interaction` chunk (now ACTIVE) already
fixed tab-bar click routing and cursor regions by switching those sites to
renderer-consistent bounds. But `handle_mouse_buffer` was not updated, and
there are now 3 separate `calculate_pane_rects` call sites using 2 different
coordinate spaces. This chunk introduces a `resolve_pane_hit()` function in
`pane_layout.rs` that:

1. Uses renderer-consistent bounds `(RAIL_WIDTH, 0, W, H)`
2. Finds which pane (if any) contains the click
3. Determines the hit zone (TabBar or Content)
4. Returns pane-local coordinates (x, y relative to pane's content origin)

Both `handle_mouse` (tab routing) and `handle_mouse_buffer` (cursor
positioning, focus switching) are refactored to use this shared function.

## Success Criteria

- Clicking anywhere in the right pane of a vertical split positions the cursor at the character under the click, not offset to the right.
- Clicking anywhere in the bottom pane of a horizontal split positions the cursor at the character under the click, not offset downward.
- Clicking in the top-left pane continues to work correctly (no regression).
- Works for 2-pane vertical, 2-pane horizontal, and multi-pane combinations.
- Terminal tabs in non-primary panes also receive correct pane-local coordinates.
- A `resolve_pane_hit()` function (or equivalent) exists in `pane_layout.rs` that returns `(PaneId, HitZone, pane_local_x, pane_local_y)` using renderer-consistent bounds.
- `handle_mouse` tab-bar routing uses `resolve_pane_hit()` instead of inline `calculate_pane_rects` + iteration.
- `handle_mouse_buffer` focus switching and coordinate transformation use `resolve_pane_hit()`.
- All existing `pane_tabs_interaction` tests continue to pass.
- New tests verify cursor positioning in non-primary panes through the full `handle_mouse` dispatch path.

