# Implementation Plan

## Approach

This chunk implements the foundational data model for tiling pane layout — a binary tree structure where:
- **Leaf nodes** are `Pane` values, each owning a `Vec<Tab>` and tab-management state
- **Internal nodes** are `Split` values with a direction (Horizontal/Vertical), a ratio, and two children

The design follows the binary space partitioning model used by bspwm, as validated in the `tiling_pane_layout` investigation. The key insight is that binary trees give unambiguous directional targeting: "move right" means "go to the other child of the nearest horizontal-split ancestor."

### Strategy

1. **Pure data model first**: Define all types (`PaneId`, `SplitDirection`, `Direction`, `MoveTarget`, `Pane`, `PaneRect`, `PaneLayoutNode`) as plain Rust structs/enums with no platform dependencies.

2. **Test-driven development**: Write failing tests for layout calculation and tree traversal before implementing the logic. The testing philosophy emphasizes semantic assertions — we test that layout produces correct rectangles, not that structs exist.

3. **Pane mirrors Workspace tab API**: The `Pane` struct's tab management methods (`add_tab`, `close_tab`, `switch_tab`, etc.) deliberately mirror the existing `Workspace` API to ease the integration chunk.

4. **Separate module**: Create a new `pane_layout.rs` module in `crates/editor/src/` to keep the data model cleanly separated from the existing workspace code.

### Patterns Used

- **Recursive enum with Box**: `PaneLayoutNode` is `Leaf(Pane) | Split { ... Box<PaneLayoutNode> ... }`, standard Rust tree pattern.
- **ID-based lookup**: Panes are identified by `PaneId` for safe cross-references without lifetime complexity.
- **Trait-free design**: No traits or generics — plain structs for simplicity and debuggability.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): The `Pane` struct will eventually hold a `Viewport`, but this chunk does not touch viewport scroll logic. The next chunk (`tiling_workspace_integration`) will add the viewport field and use the subsystem's patterns.

No subsystem work is needed for this chunk — it's a self-contained data model.

## Sequence

### Step 1: Create the pane_layout module with ID types and enums

Create `crates/editor/src/pane_layout.rs` with:
- `PaneId` type alias (u64)
- `SplitDirection` enum: `Horizontal`, `Vertical`
- `Direction` enum: `Left`, `Right`, `Up`, `Down`
- `MoveTarget` enum: `ExistingPane(PaneId)` | `SplitPane(PaneId, Direction)`

Add helper methods:
- `SplitDirection::is_compatible(direction: Direction) -> bool` — Horizontal is compatible with Left/Right, Vertical with Up/Down
- `Direction::is_toward_second() -> bool` — Right/Down go toward the second child
- `Direction::opposite() -> Direction`
- `Direction::to_split_direction() -> SplitDirection`

Export the module from `crates/editor/src/lib.rs`.

Location: `crates/editor/src/pane_layout.rs`, `crates/editor/src/lib.rs`

### Step 2: Define the Pane struct with tab management

Add the `Pane` struct with fields:
- `id: PaneId`
- `workspace_id: WorkspaceId` (imported from `workspace` module)
- `tabs: Vec<Tab>` (imported from `workspace` module)
- `active_tab: usize`
- `tab_bar_view_offset: f32`

Implement tab management methods that mirror `Workspace`:
- `add_tab(&mut self, tab: Tab)` — adds tab and sets it active
- `close_tab(&mut self, index: usize) -> Option<Tab>` — removes tab, adjusts active_tab
- `switch_tab(&mut self, index: usize)` — switches active tab, clears unread
- `active_tab(&self) -> Option<&Tab>`
- `active_tab_mut(&mut self) -> Option<&mut Tab>`
- `tab_count(&self) -> usize`
- `is_empty(&self) -> bool` — returns `tabs.is_empty()`

Add constructor:
- `Pane::new(id: PaneId, workspace_id: WorkspaceId) -> Self` — empty pane

Location: `crates/editor/src/pane_layout.rs`

### Step 3: Define PaneRect for layout output

Add the `PaneRect` struct:
- `x: f32`
- `y: f32`
- `width: f32`
- `height: f32`
- `pane_id: PaneId`

Add helper method:
- `PaneRect::contains(&self, x: f32, y: f32) -> bool` — point-in-rect test for hit-testing

Location: `crates/editor/src/pane_layout.rs`

### Step 4: Define PaneLayoutNode enum

Add the `PaneLayoutNode` enum:
```rust
pub enum PaneLayoutNode {
    Leaf(Pane),
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<PaneLayoutNode>,
        second: Box<PaneLayoutNode>,
    },
}
```

Add constructor helper:
- `PaneLayoutNode::single_pane(pane: Pane) -> Self` — wraps a pane in a Leaf

Location: `crates/editor/src/pane_layout.rs`

### Step 5: Write failing tests for layout calculation

Before implementing `calculate_pane_rects`, write tests that will initially fail:
1. Single pane fills entire bounds
2. Horizontal split divides width by ratio (default 0.5 → equal halves)
3. Vertical split divides height by ratio
4. Nested splits (HSplit containing VSplit) produce correct rectangles
5. Non-default ratios (e.g., 0.3/0.7) work correctly

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 6: Implement calculate_pane_rects

Add the layout calculation function:
```rust
pub fn calculate_pane_rects(
    bounds: (f32, f32, f32, f32), // (x, y, width, height)
    node: &PaneLayoutNode,
) -> Vec<PaneRect>
```

Recursive algorithm:
- For `Leaf(pane)`: return `vec![PaneRect { x, y, width, height, pane_id: pane.id }]`
- For `Split`: split the bounds according to direction and ratio, recurse on both children, concatenate results

Horizontal splits divide width: first gets `(x, y, width * ratio, height)`, second gets `(x + width * ratio, y, width * (1 - ratio), height)`.

Vertical splits divide height: first gets `(x, y, width, height * ratio)`, second gets `(x, y + height * ratio, width, height * (1 - ratio))`.

Run the failing tests from Step 5 — they should now pass.

Location: `crates/editor/src/pane_layout.rs`

### Step 7: Write failing tests for tree traversal helpers

Before implementing traversal, write tests for:
1. `pane_count` returns correct leaf count
2. `all_panes` returns flat list of all panes
3. `all_panes_mut` returns mutable references
4. `get_pane(pane_id)` finds the correct pane
5. `get_pane_mut(pane_id)` returns mutable reference

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 8: Implement basic tree traversal methods

Add methods to `PaneLayoutNode`:
- `pane_count(&self) -> usize` — recursive leaf count
- `all_panes(&self) -> Vec<&Pane>` — collect all leaves
- `all_panes_mut(&mut self) -> Vec<&mut Pane>` — collect mutable references
- `get_pane(&self, pane_id: PaneId) -> Option<&Pane>` — find by ID
- `get_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Pane>` — mutable find

Run the tests from Step 7 — they should now pass.

Location: `crates/editor/src/pane_layout.rs`

### Step 9: Write failing tests for find_target_in_direction

Write comprehensive tests for directional target finding using the investigation's example tree:
```
HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
```

Test cases:
1. Moving left from C → finds A (crosses the HSplit boundary)
2. Moving right from A → finds B (leftmost leaf of VSplit)
3. Moving down from B → finds C (same VSplit, toward second)
4. Moving up from C → finds B (same VSplit, toward first)
5. Moving right from C → returns `SplitPane(C, Right)` (no target exists)
6. Moving left from A → returns `SplitPane(A, Left)` (no target exists)

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 10: Implement find_target_in_direction

Add method to `PaneLayoutNode`:
```rust
pub fn find_target_in_direction(
    &self,
    pane_id: PaneId,
    direction: Direction,
) -> MoveTarget
```

Algorithm (from investigation):
1. Build the path from root to the target pane as a list of (node, child_position) pairs
2. Walk up from the pane looking for a compatible split ancestor (direction matches split direction)
3. For a compatible split:
   - If pane is in First and direction is toward Second → target is Second's nearest leaf
   - If pane is in Second and direction is toward First → target is First's nearest leaf
   - Otherwise continue walking up
4. If no compatible ancestor found → return `SplitPane(pane_id, direction)`

This requires helper methods:
- `path_to_pane(&self, pane_id: PaneId) -> Option<Vec<PathSegment>>` — returns path from root to pane
- `contains_pane(&self, pane_id: PaneId) -> bool` — checks if pane is in subtree

Run the tests from Step 9 — they should now pass.

Location: `crates/editor/src/pane_layout.rs`

### Step 11: Write failing tests for nearest_leaf_toward

Write tests for finding the nearest leaf in a direction within a subtree:
1. For a single pane, `nearest_leaf_toward(any_direction)` returns that pane
2. For `VSplit(A, B)`, `nearest_leaf_toward(Up)` returns A (topmost)
3. For `VSplit(A, B)`, `nearest_leaf_toward(Down)` returns B (bottommost)
4. For `HSplit(A, B)`, `nearest_leaf_toward(Left)` returns A (leftmost)
5. For `HSplit(A, B)`, `nearest_leaf_toward(Right)` returns B (rightmost)
6. For nested tree, correctly traverses to the edge leaf

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 12: Implement nearest_leaf_toward

Add method to `PaneLayoutNode`:
```rust
pub fn nearest_leaf_toward(&self, direction: Direction) -> PaneId
```

Algorithm:
- For `Leaf(pane)`: return `pane.id`
- For `Split`: if direction is toward First, recurse on first child; if toward Second, recurse on second

"Toward First" means Left for Horizontal splits, Up for Vertical splits.

Run the tests from Step 11 — they should now pass.

Location: `crates/editor/src/pane_layout.rs`

### Step 13: Write failing tests for Pane tab management

Write tests for `Pane`'s tab management methods:
1. `add_tab` adds to the end and sets active
2. `close_tab` removes correctly and adjusts active_tab
3. `close_tab` on last tab leaves pane empty (is_empty returns true)
4. `switch_tab` changes active and clears unread
5. `active_tab` returns None for empty pane
6. Multiple adds and closes maintain correct active_tab invariants

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 14: Add gen_pane_id utility

Add a utility function for generating unique pane IDs:
```rust
pub fn gen_pane_id(next_id: &mut u64) -> PaneId {
    let id = *next_id;
    *next_id += 1;
    id
}
```

This matches the pattern used in `Editor::gen_tab_id()`.

Location: `crates/editor/src/pane_layout.rs`

### Step 15: Final verification and documentation

1. Run `cargo test -p lite-edit-editor` to verify all tests pass
2. Run `cargo clippy -p lite-edit-editor` to check for warnings
3. Add module-level doc comment explaining the binary pane tree model
4. Add backreference comment at module level: `// Chunk: docs/chunks/tiling_tree_model`

Location: `crates/editor/src/pane_layout.rs`

## Dependencies

No external dependencies. This chunk only depends on:
- The existing `Tab` type from `crates/editor/src/workspace.rs`
- The existing `WorkspaceId` type from `crates/editor/src/workspace.rs`

## Risks and Open Questions

1. **Tab ownership transfer**: When the `tiling_tab_movement` chunk moves tabs between panes, it will need to handle ownership transfer. This chunk's `close_tab` returns `Option<Tab>` and `add_tab` takes `Tab` by value, which should work, but the interaction wasn't fully tested across panes.

2. **Mutable reference collection**: `all_panes_mut` returns `Vec<&mut Pane>`, which works in Rust but may be awkward to use. An alternative iterator-based approach could be added if needed.

3. **Path representation**: The `path_to_pane` helper builds a vector of path segments. For very deep trees this could be slow, but in practice pane trees are expected to be shallow (< 10 levels).

## Deviations

<!-- To be populated during implementation -->