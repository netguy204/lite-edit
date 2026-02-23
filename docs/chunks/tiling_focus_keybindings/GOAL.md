---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/pane_layout.rs
code_references: []
narrative: null
investigation: tiling_pane_layout
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- tiling_multi_pane_render
created_after:
- welcome_screen_startup
---
# Chunk Goal

## Minor Goal

Wire up keybindings for pane operations (directional tab movement and focus switching) and mouse-click-to-focus. This is the final chunk in the tiling pane sequence — it connects the tree model and movement operations (chunks 1-2) to user input, making pane splitting fully interactive.

After this chunk, a user can: create tabs, move them directionally to create and populate panes, switch focus between panes, and click to focus — the full tiling window manager interaction model described in the investigation trigger.

## Success Criteria

- **Directional tab movement** (`Cmd+Shift+Arrow`):
  - `Cmd+Shift+Right`: moves the active tab of the focused pane to the right (into an existing pane or a new split).
  - `Cmd+Shift+Left`: moves the active tab to the left.
  - `Cmd+Shift+Down`: moves the active tab downward.
  - `Cmd+Shift+Up`: moves the active tab upward.
  - These bindings call `move_tab()` from `tiling_tab_movement` on the workspace's pane tree.
  - After a successful move, focus follows the moved tab to its new pane.
  - After a move, the dirty region is set to `FullViewport` (pane layout changed).
  - If the move is rejected (single tab, no target), no-op — no visual change.
  - These bindings do not conflict with existing `Cmd+Shift+[/]` (tab cycling) since those use bracket keys, not arrows.

- **Focus switching between panes** (`Cmd+Option+Arrow`):
  - `Cmd+Option+Right`: moves focus to the pane visually to the right of the current pane.
  - `Cmd+Option+Left`: moves focus to the pane visually to the left.
  - `Cmd+Option+Down`: moves focus to the pane below.
  - `Cmd+Option+Up`: moves focus to the pane above.
  - Focus switching uses the same `find_target_in_direction` tree walk as tab movement, but only changes `active_pane_id` without moving any tabs.
  - If no pane exists in the given direction, no-op.
  - After focus switch, the dirty region is set to `FullViewport` (focused pane indicator changes).

- **Mouse click to focus**:
  - Clicking anywhere within a pane's rectangle (tab bar or content area) sets that pane as the focused pane (`active_pane_id`).
  - This is integrated into the mouse dispatch pipeline from `tiling_workspace_integration` — the hit-test that determines which pane was clicked also sets focus before routing the click to the pane's handler.
  - Clicking within the already-focused pane does not trigger unnecessary redraws.

- **Keyboard input routing**:
  - All keyboard input (key events, not mouse) is routed to the focused pane's active tab. `EditorState::handle_key_buffer()` operates on the focused pane, not a global active tab.
  - Tab-level shortcuts (`Cmd+T`, `Cmd+W`, `Cmd+Shift+[/]`) operate within the focused pane.

- **Integration with existing shortcuts**:
  - `Cmd+T` creates a new tab in the focused pane.
  - `Cmd+Shift+T` creates a new terminal tab in the focused pane.
  - `Cmd+W` closes the active tab in the focused pane. If the pane becomes empty, cleanup runs (tree collapses).
  - `Cmd+Shift+[/]` cycles tabs within the focused pane.
  - Workspace-level shortcuts (`Cmd+[/]`, `Cmd+1-9`) are unchanged — they switch workspaces, not panes.
