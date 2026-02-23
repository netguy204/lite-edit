<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix leverages the existing `cleanup_empty_panes` function in `pane_layout.rs` which already handles collapsing empty panes and promoting their siblings. The missing piece is:

1. **Detection**: After closing a tab in `EditorState::close_tab`, check if the pane is now empty when `pane_count > 1`.
2. **Cleanup**: Call `cleanup_empty_panes` on the workspace's `pane_root` to collapse the empty pane.
3. **Focus transfer**: Before cleanup, determine which adjacent pane should receive focus. After cleanup, update `active_pane_id` to that pane.

The approach follows the Humble View Architecture from TESTING_PHILOSOPHY.md — all logic is testable as pure state manipulation without platform dependencies.

**Key insight**: The `cleanup_empty_panes` function returns a `CleanupResult` indicating what happened (NoChange, Collapsed, RootEmpty). Since we only call it when `pane_count > 1` and we just emptied a pane, we expect `Collapsed`. However, we must handle `RootEmpty` gracefully (though it shouldn't occur with `pane_count > 1`).

## Sequence

### Step 1: Write failing test for the crash scenario

Add a test to `editor_state.rs` that reproduces the crash:
1. Create an EditorState with a workspace containing two panes (horizontal split)
2. Each pane has exactly one tab
3. Close the tab in the active pane via `close_tab(0)`
4. Assert: No panic occurs
5. Assert: Tree is now a single pane (the other pane)
6. Assert: `active_pane_id` points to the remaining pane

Location: `crates/editor/src/editor_state.rs` (tests module)

### Step 2: Add helper to find adjacent pane for focus transfer

Before emptying a pane, we need to know which pane will receive focus. Add a method to `Workspace`:

```rust
/// Finds a pane to focus after the current active pane is removed.
/// Returns the ID of an adjacent pane, preferring the direction order: Right, Left, Down, Up.
pub fn find_fallback_focus(&self) -> Option<PaneId>
```

This uses `find_target_in_direction` to search in each direction until an existing pane is found.

Location: `crates/editor/src/workspace.rs`

### Step 3: Modify `EditorState::close_tab` to handle empty panes

Update the `close_tab` method in `EditorState` to:

1. After `pane.close_tab(index)`, check if the pane is now empty (`pane.is_empty()`)
2. If empty and `pane_count > 1`:
   a. Find fallback focus pane via `workspace.find_fallback_focus()`
   b. Call `cleanup_empty_panes(&mut workspace.pane_root)`
   c. Update `workspace.active_pane_id` to the fallback pane

The existing TODO comment at line ~2278 explicitly notes this gap: "TODO: If pane is now empty and there are multiple panes, cleanup empty panes."

Location: `crates/editor/src/editor_state.rs`

### Step 4: Write additional edge case tests

Add tests for:

1. **Three-pane layout**: HSplit(Pane[A], VSplit(Pane[B], Pane[C])), close last tab in B → tree becomes HSplit(Pane[A], Pane[C])
2. **Focus direction preference**: Verify focus moves to the expected adjacent pane based on layout
3. **Single-pane single-tab unchanged**: Closing the last tab in a single-pane layout still creates an empty tab (existing behavior)
4. **Multi-tab pane unchanged**: Closing a non-last tab in a pane doesn't trigger cleanup (no empty pane)

Location: `crates/editor/src/editor_state.rs` (tests module)

### Step 5: Verify existing tests pass

Run `cargo test -p lite-edit-editor` to ensure:
- All existing pane_layout tests pass (cleanup_empty_panes, move_tab, etc.)
- All existing editor_state tests pass
- No regressions in tab management behavior

---

**BACKREFERENCE COMMENTS**

Add the following backreference where the fix is implemented:

```rust
// Chunk: docs/chunks/pane_close_last_tab - Cleanup empty panes on last tab close
```

## Dependencies

This chunk depends on completed work from:
- `tiling_multi_pane_render` (multi-pane layouts exist)
- `tiling_focus_keybindings` (focus switching infrastructure)
- `tiling_workspace_integration` (workspace pane tree integration)

All dependencies are ACTIVE (per GOAL.md frontmatter `created_after`), so implementation can proceed.

## Risks and Open Questions

1. **Focus direction preference**: The current plan prefers Right → Left → Down → Up. This is a reasonable default but may not match user expectations in all layouts. If feedback suggests a different order, the `find_fallback_focus` method can be adjusted.

2. **Borrow checker complexity**: The cleanup operation requires mutable access to `workspace.pane_root` while also reading `workspace.active_pane_id`. This should work since we pre-compute the fallback focus before mutating, but the exact borrow sequence needs care.

3. **Active pane becomes invalid**: After `cleanup_empty_panes`, the old `active_pane_id` no longer exists in the tree. We must update it *before* any code tries to use it (like `sync_active_tab_viewport`). The sequence in close_tab must be: close tab → find fallback → cleanup → update active_pane_id → mark dirty.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.
-->