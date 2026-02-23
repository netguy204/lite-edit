---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/pane_layout.rs
- crates/editor/src/lib.rs
code_references: []
narrative: null
investigation: tiling_pane_layout
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- welcome_screen_startup
---
# Chunk Goal

## Minor Goal

Implement the binary pane layout tree data model that will underpin tiling window manager-style pane splitting in lite-edit. This is the foundational data structure for the tiling pane system — all subsequent chunks (tab movement, workspace integration, rendering, keybindings) build on it.

The tree uses a binary structure inspired by bspwm: each internal node is a `Split` with a direction (Horizontal or Vertical), a ratio (default 0.5), and exactly two children. Leaf nodes are `Pane` values, each owning its own `Vec<Tab>`, active tab index, and tab bar scroll offset. This binary model was chosen over n-ary trees because it gives unambiguous directional targeting for tab movement operations (see investigation H1).

This chunk is pure data model — no rendering, no integration with `EditorState` or `Workspace`. It is fully testable in isolation via unit tests.

## Success Criteria

- A `PaneLayoutNode` enum with two variants:
  - `Leaf(Pane)` — a pane containing tabs
  - `Split { direction: SplitDirection, ratio: f32, first: Box<PaneLayoutNode>, second: Box<PaneLayoutNode> }` — a binary split
- `SplitDirection` enum with `Horizontal` (children side-by-side, left/right) and `Vertical` (children stacked, top/bottom) variants.
- `Pane` struct owning `id: PaneId`, `workspace_id: WorkspaceId`, `tabs: Vec<Tab>`, `active_tab: usize`, `tab_bar_view_offset: f32`. Tab management methods: `add_tab`, `close_tab`, `switch_tab`, `active_tab`, `active_tab_mut`, `tab_count`, `is_empty`.
- `PaneRect` struct for screen rectangles (`x`, `y`, `width`, `height`, `pane_id`).
- Layout calculation: `calculate_pane_rects(bounds, &PaneLayoutNode) -> Vec<PaneRect>` that recursively splits rectangles according to split direction and ratio. Horizontal splits divide width, vertical splits divide height.
- Tree traversal helpers:
  - `pane_count(&self) -> usize` — total leaf count
  - `all_panes(&self) -> Vec<&Pane>` — flat list of all panes
  - `all_panes_mut(&mut self) -> Vec<&mut Pane>` — mutable version
  - `get_pane(pane_id) -> Option<&Pane>` — lookup by ID
  - `get_pane_mut(pane_id) -> Option<&mut Pane>` — mutable lookup
  - `find_target_in_direction(pane_id, Direction) -> MoveTarget` — walks up from the given pane looking for a compatible split ancestor to determine whether a directional move targets an existing pane or requires a split (used by the next chunk)
  - `nearest_leaf_toward(direction) -> PaneId` — finds the nearest leaf in a subtree in the given direction (leftmost for Right, topmost for Down, etc.)
- `Direction` enum: `Left`, `Right`, `Up`, `Down`.
- `MoveTarget` enum: `ExistingPane(PaneId)` | `SplitPane(PaneId, Direction)` — the result of a directional target search.
- `gen_pane_id(next_id: &mut u64) -> PaneId` utility.
- Comprehensive unit tests:
  - Single pane layout fills bounds
  - Horizontal split divides width by ratio
  - Vertical split divides height by ratio
  - Nested splits (HSplit containing a VSplit) produce correct rectangles
  - Non-default ratios (e.g., 0.3/0.7) work correctly
  - `find_target_in_direction` returns correct targets for the scenario: `HSplit(Pane[A], VSplit(Pane[B], Pane[C]))` — moving left from C finds A, moving right from A finds B, moving down from B finds C
  - `find_target_in_direction` returns `SplitPane` when no target exists (e.g., moving right from a pane that is already the rightmost in every horizontal ancestor)
  - `nearest_leaf_toward` returns the correct leaf for each direction
  - Pane tab management methods work correctly (add, close, switch, active tab adjustment)
