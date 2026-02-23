# Implementation Plan

## Approach

This chunk adds mutation operations to the pane layout tree from `tiling_tree_model`:

1. **`move_tab`** — Moves the active tab from a source pane to a target pane (or creates a new split)
2. **`cleanup_empty_panes`** — Collapses empty panes by promoting their siblings

### Strategy

The implementation follows the investigation's verified algorithm:

1. **Find target**: Use the existing `find_target_in_direction` method to determine whether the move targets an existing pane (`ExistingPane(id)`) or requires a split (`SplitPane(id, direction)`).

2. **Execute move**: Two cases:
   - **ExistingPane**: Remove the active tab from source pane, add it to target pane.
   - **SplitPane**: Remove the active tab from source pane, create a new pane, replace the source leaf with a split node containing both.

3. **Cleanup**: After any move, run `cleanup_empty_panes` to collapse empty leaf nodes.

### Key Design Decisions

Per the investigation (H5), the following constraints apply:

- **Single-tab rejection**: Moving a tab from a pane with only one tab is only allowed if an existing target pane exists. Splitting a single-tab pane creates an empty sibling that immediately collapses — a no-op. The function should return `MoveResult::Rejected` in this case.

- **Single-tab transfer**: If a pane has one tab and there IS an existing target, the tab transfers normally, the source empties, and cleanup collapses the tree.

- **Automatic cleanup**: `cleanup_empty_panes` is called automatically at the end of `move_tab`, so callers don't need to call it separately.

### Patterns Used

- **Return type with rich information**: `MoveResult` enum provides callers with information about what happened (tab moved to existing pane, new pane created, or move rejected).

- **`&mut PaneLayoutNode` mutation**: The move operation mutates the tree in place. For split creation, this requires replacing a `Leaf` node with a `Split` node, which is done by taking ownership of the inner pane and constructing the new tree structure.

- **Idempotent cleanup**: `cleanup_empty_panes` can be called multiple times safely. It finds empty leaf panes and promotes their siblings.

### Building On

- `PaneLayoutNode::find_target_in_direction` — determines the move target
- `Pane::close_tab` — removes a tab from a pane
- `Pane::add_tab` — adds a tab to a pane
- `Direction::to_split_direction` — maps movement direction to split direction
- `Direction::is_toward_second` — determines child ordering in new splits

## Subsystem Considerations

No subsystems are directly relevant to this chunk. This is a pure data model extension that operates on the pane tree structure without touching rendering, input routing, or platform code.

## Sequence

### Step 1: Define the MoveResult enum

Add a new `MoveResult` enum to `pane_layout.rs` that captures the outcome of a move operation:

```rust
pub enum MoveResult {
    /// Tab moved to an existing pane
    MovedToExisting {
        /// The pane the tab was moved from
        source_pane_id: PaneId,
        /// The pane the tab was moved to (now focused)
        target_pane_id: PaneId,
    },
    /// Tab moved to a newly created pane via split
    MovedToNew {
        /// The pane the tab was moved from
        source_pane_id: PaneId,
        /// The newly created pane (now focused)
        new_pane_id: PaneId,
    },
    /// Move was rejected (single-tab pane with no existing target)
    Rejected,
    /// Source pane not found in tree
    SourceNotFound,
}
```

This provides callers with enough information to update focus state after the move.

Location: `crates/editor/src/pane_layout.rs`

### Step 2: Write failing tests for move_tab basic scenarios

Before implementing `move_tab`, write tests for the core scenarios from the GOAL.md:

1. **Split creation**: Two tabs in a single pane, move one right → HSplit with two panes, each containing one tab.
2. **Move to existing neighbor**: `HSplit(Pane[A, B], Pane[C])`, move tab B right → B joins Pane[C].
3. **Single-tab rejection**: Pane with one tab, no existing target → move rejected.
4. **Single-tab transfer**: Pane with one tab, existing target in direction → tab moves, source empties (test that tree collapses after cleanup).

These tests will fail until `move_tab` is implemented.

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 3: Implement remove_active_tab helper on Pane

Add a method to `Pane` that removes and returns the active tab:

```rust
impl Pane {
    /// Removes and returns the active tab, if any.
    ///
    /// After removal, the active_tab index is adjusted to remain valid.
    /// This is a convenience wrapper around `close_tab(self.active_tab)`.
    pub fn remove_active_tab(&mut self) -> Option<Tab> {
        if self.tabs.is_empty() {
            None
        } else {
            self.close_tab(self.active_tab)
        }
    }
}
```

This simplifies the `move_tab` implementation.

Location: `crates/editor/src/pane_layout.rs`

### Step 4: Implement move_tab function

Add the main `move_tab` function that orchestrates the move:

```rust
pub fn move_tab(
    root: &mut PaneLayoutNode,
    source_pane_id: PaneId,
    direction: Direction,
    new_pane_id_fn: impl FnMut() -> PaneId,
) -> MoveResult
```

Algorithm:
1. Find target using `root.find_target_in_direction(source_pane_id, direction)`.
2. Check preconditions:
   - Source pane must exist.
   - Source pane must have at least one tab.
   - If source has only one tab and target is `SplitPane` → return `Rejected`.
3. Execute move based on target type.
4. Call `cleanup_empty_panes(root)`.
5. Return appropriate `MoveResult`.

The function takes a `new_pane_id_fn` closure for generating new pane IDs, allowing the caller to control ID generation (matches the existing `gen_pane_id` pattern).

Location: `crates/editor/src/pane_layout.rs`

### Step 5: Implement move to existing pane

Within `move_tab`, implement the `ExistingPane` case:

1. Get mutable references to source and target panes.
2. Remove the active tab from the source pane using `remove_active_tab()`.
3. Add the tab to the target pane using `add_tab()`.
4. Return `MovedToExisting { source_pane_id, target_pane_id }`.

**Challenge**: Rust borrow checker won't allow two mutable references to different panes in the same tree. Solution: Extract the tab first (which only needs source pane), then add to target pane. The extraction can be done with a temporary:

```rust
// Remove tab from source
let tab = {
    let source = root.get_pane_mut(source_pane_id)?;
    source.remove_active_tab()?
};
// Add to target
let target = root.get_pane_mut(target_pane_id)?;
target.add_tab(tab);
```

Location: `crates/editor/src/pane_layout.rs`

### Step 6: Implement split creation for move

Within `move_tab`, implement the `SplitPane` case:

1. Remove the active tab from the source pane.
2. Generate a new pane ID using `new_pane_id_fn()`.
3. Create a new `Pane` containing just the moved tab.
4. Find the parent of the source pane and replace the source leaf with a `Split` node.

**Challenge**: Replacing a leaf node with a split requires tree surgery. The approach:
- Walk the tree to find and replace the node containing `source_pane_id`.
- Use a helper method `replace_pane_with_split()` that:
  1. Takes ownership of the current `PaneLayoutNode` via `std::mem::replace`.
  2. Extracts the `Pane` from the `Leaf`.
  3. Builds a new `Split` node with the original pane and the new pane.
  4. Determines child ordering based on direction (Right/Down → original is First, new is Second).

Location: `crates/editor/src/pane_layout.rs`

### Step 7: Add replace_pane_with_split helper

Implement a helper method on `PaneLayoutNode`:

```rust
impl PaneLayoutNode {
    /// Replaces a leaf pane with a split containing that pane and a new pane.
    ///
    /// Returns `true` if the replacement was made, `false` if the pane wasn't found.
    fn replace_pane_with_split(
        &mut self,
        pane_id: PaneId,
        new_pane: Pane,
        direction: Direction,
    ) -> bool
}
```

This recursively searches for the pane and performs the replacement. The split direction is determined by `direction.to_split_direction()`, and child ordering is determined by `direction.is_toward_second()`.

Location: `crates/editor/src/pane_layout.rs`

### Step 8: Write failing tests for cleanup_empty_panes

Write tests for empty pane cleanup scenarios:

1. **No empty panes**: Tree unchanged.
2. **Single split with one empty leaf**: `HSplit(Pane[], Pane[A])` → `Pane[A]`.
3. **Nested tree collapse**: `HSplit(Pane[A], VSplit(Pane[], Pane[B]))` → `HSplit(Pane[A], Pane[B])`.
4. **Root is empty pane**: Cleanup returns/marks this case (caller handles it).
5. **Deep collapse**: Multiple empties collapse correctly.

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 9: Implement cleanup_empty_panes

Add the cleanup function:

```rust
pub fn cleanup_empty_panes(root: &mut PaneLayoutNode) -> CleanupResult
```

Where `CleanupResult` indicates what happened:
```rust
pub enum CleanupResult {
    /// No changes made
    NoChange,
    /// Empty panes were collapsed
    Collapsed,
    /// Root pane is empty (caller must handle)
    RootEmpty,
}
```

Algorithm:
- If root is `Leaf` with empty pane → return `RootEmpty`.
- If root is `Split`, recursively cleanup children.
- After recursing, check if either child is a `Leaf` with empty pane.
- If so, replace this `Split` with the non-empty sibling.
- Use `std::mem::replace` to swap nodes.

Location: `crates/editor/src/pane_layout.rs`

### Step 10: Write integration tests for move + cleanup scenarios

Add tests that verify the full move-then-cleanup workflow:

1. **Split creation followed by cleanup of original**: Start with `Pane[A, B]`, move B right → `HSplit(Pane[A], Pane[B])`. Tree structure is correct.

2. **Nested tree navigation with cleanup**: `HSplit(Pane[A], VSplit(Pane[B], Pane[C]))`, move tab from C left → tab joins Pane A, Pane C empties, VSplit collapses → `HSplit(Pane[A + tab], Pane[B])`.

3. **Deep tree collapse**: Series of moves that progressively empty panes and collapse the tree back to a single pane.

4. **Direction ordering**: Move right creates new pane as Second child. Move left creates new pane as First child. Verify with layout rect positions.

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 11: Write test for workspace_id preservation

Ensure that when a new pane is created during a split, it inherits the workspace_id from the source pane:

```rust
#[test]
fn test_move_tab_preserves_workspace_id() {
    let workspace_id = 42;
    // Create a pane with specific workspace_id
    // Move tab to create split
    // Verify new pane has same workspace_id
}
```

Location: `crates/editor/src/pane_layout.rs` (in `#[cfg(test)]` module)

### Step 12: Final verification and documentation

1. Run `cargo test -p lite-edit` to verify all tests pass.
2. Run `cargo clippy -p lite-edit` to check for warnings.
3. Add doc comments to `move_tab` and `cleanup_empty_panes` explaining usage and edge cases.
4. Add backreference comment for the new functions: `// Chunk: docs/chunks/tiling_tab_movement`

Location: `crates/editor/src/pane_layout.rs`

## Dependencies

- **tiling_tree_model** (completed): Provides `PaneLayoutNode`, `Pane`, `Direction`, `MoveTarget`, `find_target_in_direction`, `nearest_leaf_toward`, and all the foundational tree types and traversal methods.

## Risks and Open Questions

1. **Borrow checker complexity**: Getting mutable references to two different panes in the same tree requires careful ordering. The plan addresses this by extracting the tab first, then adding it, but the `replace_pane_with_split` operation requires taking ownership via `std::mem::replace`, which adds complexity.

2. **Root node handling**: When the root is a single pane that becomes empty, `cleanup_empty_panes` can't replace it with "nothing." The function will return `RootEmpty` and the caller must handle this (e.g., create a new welcome tab). This is consistent with the investigation's finding.

3. **Tab ID uniqueness**: When tabs move between panes, they keep their original IDs. This should be fine since tab IDs are globally unique within an editor, but worth verifying during integration testing.

4. **active_tab adjustment after move**: When a tab is removed from a pane, `Pane::close_tab` already adjusts `active_tab` to remain valid. When a tab is added, `Pane::add_tab` makes it active. This is the expected behavior per the investigation.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->