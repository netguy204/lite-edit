---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/pane_layout.rs
- crates/editor/src/lib.rs
code_references:
  - ref: crates/editor/src/pane_layout.rs#PaneId
    implements: "Unique identifier type for panes in the layout tree"
  - ref: crates/editor/src/pane_layout.rs#gen_pane_id
    implements: "Utility function for generating unique pane IDs"
  - ref: crates/editor/src/pane_layout.rs#SplitDirection
    implements: "Direction enum for splits (Horizontal/Vertical)"
  - ref: crates/editor/src/pane_layout.rs#SplitDirection::is_compatible
    implements: "Checks if a movement direction aligns with split axis"
  - ref: crates/editor/src/pane_layout.rs#Direction
    implements: "Cardinal direction enum for pane navigation (Left/Right/Up/Down)"
  - ref: crates/editor/src/pane_layout.rs#Direction::is_toward_second
    implements: "Determines if direction goes toward second child"
  - ref: crates/editor/src/pane_layout.rs#Direction::opposite
    implements: "Returns the opposite direction"
  - ref: crates/editor/src/pane_layout.rs#Direction::to_split_direction
    implements: "Maps movement direction to compatible split direction"
  - ref: crates/editor/src/pane_layout.rs#MoveTarget
    implements: "Result enum for directional target search (ExistingPane/SplitPane)"
  - ref: crates/editor/src/pane_layout.rs#Pane
    implements: "Leaf node containing tabs with management API mirroring Workspace"
  - ref: crates/editor/src/pane_layout.rs#Pane::new
    implements: "Creates a new empty pane"
  - ref: crates/editor/src/pane_layout.rs#Pane::add_tab
    implements: "Adds a tab and makes it active"
  - ref: crates/editor/src/pane_layout.rs#Pane::close_tab
    implements: "Closes a tab and adjusts active_tab index"
  - ref: crates/editor/src/pane_layout.rs#Pane::switch_tab
    implements: "Switches active tab and clears unread state"
  - ref: crates/editor/src/pane_layout.rs#Pane::active_tab
    implements: "Returns reference to active tab"
  - ref: crates/editor/src/pane_layout.rs#Pane::active_tab_mut
    implements: "Returns mutable reference to active tab"
  - ref: crates/editor/src/pane_layout.rs#Pane::tab_count
    implements: "Returns number of tabs in pane"
  - ref: crates/editor/src/pane_layout.rs#Pane::is_empty
    implements: "Checks if pane has no tabs"
  - ref: crates/editor/src/pane_layout.rs#PaneRect
    implements: "Screen rectangle for layout output with hit-testing"
  - ref: crates/editor/src/pane_layout.rs#PaneRect::contains
    implements: "Point-in-rect test for hit-testing"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode
    implements: "Binary tree enum with Leaf(Pane) and Split variants"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::single_pane
    implements: "Constructor for leaf node"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::pane_count
    implements: "Returns total leaf count"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::all_panes
    implements: "Returns flat list of all panes"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::all_panes_mut
    implements: "Returns mutable references to all panes"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::get_pane
    implements: "Lookup pane by ID"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::get_pane_mut
    implements: "Mutable lookup pane by ID"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::contains_pane
    implements: "Check if pane exists in subtree"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::nearest_leaf_toward
    implements: "Finds nearest leaf in direction within subtree"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::find_target_in_direction
    implements: "Walks up tree to find target pane or determine split needed"
  - ref: crates/editor/src/pane_layout.rs#calculate_pane_rects
    implements: "Recursive layout calculation producing screen rectangles"
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
