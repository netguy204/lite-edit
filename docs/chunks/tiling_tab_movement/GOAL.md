---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/pane_layout.rs
code_references:
  - ref: crates/editor/src/pane_layout.rs#MoveResult
    implements: "Result type capturing tab move outcomes (moved to existing, moved to new, rejected, source not found)"
  - ref: crates/editor/src/pane_layout.rs#CleanupResult
    implements: "Result type for empty pane cleanup operations"
  - ref: crates/editor/src/pane_layout.rs#Pane::remove_active_tab
    implements: "Removes and returns the active tab for move operations"
  - ref: crates/editor/src/pane_layout.rs#move_tab
    implements: "Main entry point for directional tab movement with automatic cleanup"
  - ref: crates/editor/src/pane_layout.rs#move_tab_to_existing
    implements: "Moves a tab to an existing neighbor pane"
  - ref: crates/editor/src/pane_layout.rs#move_tab_to_new_split
    implements: "Moves a tab to a newly created pane via split"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::replace_pane_with_split
    implements: "Tree surgery to replace a leaf with a split node containing original and new pane"
  - ref: crates/editor/src/pane_layout.rs#cleanup_empty_panes
    implements: "Collapses empty panes by promoting siblings"
  - ref: crates/editor/src/pane_layout.rs#cleanup_empty_panes_impl
    implements: "Recursive implementation of empty pane cleanup with sibling promotion"
narrative: null
investigation: tiling_pane_layout
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- tiling_tree_model
created_after:
- welcome_screen_startup
---
# Chunk Goal

## Minor Goal

Implement directional tab movement and empty-pane cleanup on the binary pane layout tree from `tiling_tree_model`. This chunk adds the mutation operations that make the pane tree dynamic: moving a tab from one pane to another (or into a newly created split), and collapsing the tree when panes become empty.

These operations are the core of the tiling window manager interaction model. A user with multiple tabs in a pane presses `Cmd+Shift+Arrow` and the active tab moves in that direction — either into an existing adjacent pane or into a new pane created by splitting. When a pane loses its last tab through movement or closing, the tree contracts by promoting the remaining sibling.

This chunk operates purely on the data model — no keybinding wiring, no rendering. All operations are testable via unit tests against tree state.

## Success Criteria

- `move_tab(root: &mut PaneLayoutNode, source_pane_id: PaneId, direction: Direction, new_pane_id_fn: impl FnMut() -> PaneId) -> MoveResult` that:
  - Uses `find_target_in_direction` to determine the move target.
  - For `ExistingPane(target_id)`: removes the active tab from the source pane and adds it to the target pane. Returns information about which pane now has focus.
  - For `SplitPane(pane_id, direction)`: removes the active tab from the source pane, creates a new pane containing that tab, and replaces the source pane's leaf node with a `Split` node (source pane + new pane, ordered by direction). Returns information about which pane now has focus (the new pane).
  - Rejects the move if the source pane has only one tab and no existing target exists (splitting a single-tab pane is a no-op — see investigation finding).
  - Allows moving the single tab if an existing target pane is found (tab transfers to the neighbor, source pane empties, cleanup runs).
- `cleanup_empty_panes(root: &mut PaneLayoutNode)` that:
  - Finds any leaf pane with zero tabs.
  - Replaces the empty pane's parent split with the non-empty sibling (sibling promotion).
  - Handles the root case (if root is an empty pane, the caller must handle it — e.g., by creating a new empty tab).
  - Is idempotent and handles nested cleanup (though in practice only one pane empties per operation).
- `cleanup_empty_panes` is called automatically at the end of `move_tab`.
- Unit tests covering all scenarios:
  - **Split creation**: Two tabs in a single pane, move one right → HSplit with two panes, each containing one tab.
  - **Move to existing neighbor**: `HSplit(Pane[A, B], Pane[C])`, move B left → `HSplit(Pane[A], Pane[C, B])` (B joins A's pane... wait, moving B left from the left pane has no target — test the correct direction). Move C left → tab C joins Pane[A, B]. Pane on right empties, tree collapses to single pane.
  - **Nested tree navigation**: `HSplit(Pane[A], VSplit(Pane[B], Pane[C]))` — move tab from C left → tab lands in Pane A. Pane C empties, VSplit collapses, result is `HSplit(Pane[A + C's tab], Pane[B])`.
  - **Split with direction ordering**: Move right creates new pane on the right (Second child). Move left creates new pane on the left (First child). Move down creates new pane on the bottom. Move up creates new pane on the top.
  - **Single-tab rejection**: Pane with one tab, no existing target in direction → move is rejected (no-op).
  - **Single-tab transfer**: Pane with one tab, existing target in direction → tab moves, source empties, tree collapses.
  - **Deep tree collapse**: After multiple moves that empty panes, the tree collapses back to a single pane correctly.
