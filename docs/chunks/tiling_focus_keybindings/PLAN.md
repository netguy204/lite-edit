<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk wires the existing pane tree model and movement operations (from `tiling_tree_model` and `tiling_tab_movement`) to user input, completing the tiling pane sequence. The implementation follows the editor's established patterns:

1. **Keybindings** are intercepted in `EditorState::handle_key()` before delegating to focus targets — this is where `Cmd+Shift+Arrow` (tab movement) and `Cmd+Option+Arrow` (focus switching) will be handled.

2. **Mouse click-to-focus** integrates into the existing `handle_mouse()` pipeline from `tiling_workspace_integration` — the hit-test that determines which pane was clicked also sets focus.

3. **Keyboard input routing** already resolves through `active_workspace → active_pane → active_tab` thanks to `tiling_workspace_integration`. Changing `active_pane_id` automatically routes subsequent input to the correct pane.

The implementation uses pure functions from `pane_layout.rs` (`find_target_in_direction`, `move_tab`) and existing coordinate handling from `tiling_workspace_integration`'s screen-space pipeline. No ad-hoc coordinate math is introduced.

## Subsystem Considerations

No existing subsystems are directly relevant to this chunk. The work is self-contained within the editor's input handling layer.

## Sequence

### Step 1: Add `switch_focus` helper to Workspace

Create a method `Workspace::switch_focus(direction: Direction) -> bool` that:
1. Calls `pane_root.find_target_in_direction(active_pane_id, direction)`.
2. If result is `MoveTarget::ExistingPane(target_id)`, sets `active_pane_id = target_id` and returns `true`.
3. If result is `SplitPane`, returns `false` (no adjacent pane in that direction — focus stays put).

Location: `crates/editor/src/workspace.rs`

### Step 2: Add `move_active_tab` helper to Workspace

Create a method `Workspace::move_active_tab(&mut self, direction: Direction) -> MoveResult` that:
1. Captures current `active_pane_id` as the source.
2. Calls `pane_layout::move_tab(&mut self.pane_root, source_pane_id, direction, || self.gen_pane_id())`.
3. If result is `MovedToExisting { target_pane_id, .. }` or `MovedToNew { new_pane_id, .. }`, updates `active_pane_id` to the target/new pane (focus follows the moved tab).
4. Returns the `MoveResult` so the caller can determine if a redraw is needed.

Location: `crates/editor/src/workspace.rs`

### Step 3: Wire Cmd+Shift+Arrow keybindings for directional tab movement

In `EditorState::handle_key()`, add handling for `Cmd+Shift+Arrow` before delegating to focus targets:
- `Cmd+Shift+Right` → call `move_active_tab(Direction::Right)` on active workspace.
- `Cmd+Shift+Left` → call `move_active_tab(Direction::Left)`.
- `Cmd+Shift+Down` → call `move_active_tab(Direction::Down)`.
- `Cmd+Shift+Up` → call `move_active_tab(Direction::Up)`.

After a successful move (`MovedToExisting` or `MovedToNew`), set `dirty_region = DirtyRegion::FullViewport` (pane layout changed). If `Rejected` or `SourceNotFound`, no visual change — no-op.

These bindings use arrow keys, so they don't conflict with existing `Cmd+Shift+[/]` (bracket keys for tab cycling).

Location: `crates/editor/src/editor_state.rs` (within `handle_key` method, after workspace-level shortcuts but before focus routing)

### Step 4: Wire Cmd+Option+Arrow keybindings for focus switching

In `EditorState::handle_key()`, add handling for `Cmd+Option+Arrow`:
- `Cmd+Option+Right` → call `switch_focus(Direction::Right)` on active workspace.
- `Cmd+Option+Left` → call `switch_focus(Direction::Left)`.
- `Cmd+Option+Down` → call `switch_focus(Direction::Down)`.
- `Cmd+Option+Up` → call `switch_focus(Direction::Up)`.

After a successful focus switch (returns `true`), set `dirty_region = DirtyRegion::FullViewport` (focused pane indicator changes). If `false`, no-op.

Location: `crates/editor/src/editor_state.rs`

### Step 5: Add mouse click-to-focus in handle_mouse()

Extend `EditorState::handle_mouse()` (or `handle_mouse_buffer`) to set pane focus when clicking within a pane:

1. Before routing the click to a specific handler, check which pane contains the click point using `calculate_pane_rects()` and `PaneRect::contains()`.
2. If the clicked pane is different from `active_pane_id`, update `active_pane_id` to the clicked pane.
3. Only trigger a dirty region if focus actually changed (avoid unnecessary redraws when clicking within the already-focused pane).
4. Then proceed with the existing click handling (tab bar click, content click, etc.) — the click will now route to the correct pane.

The coordinate pipeline from `tiling_workspace_integration` already computes `PaneRect` values in screen space and transforms to pane-local coordinates at dispatch. This step adds focus-switching logic before dispatch.

Location: `crates/editor/src/editor_state.rs` (within `handle_mouse` and/or `handle_mouse_buffer`)

### Step 6: Ensure existing tab/workspace shortcuts use focused pane

Verify that the following shortcuts operate on the focused pane (they should, thanks to `tiling_workspace_integration`):
- `Cmd+T` creates a new tab in the focused pane.
- `Cmd+Shift+T` creates a new terminal tab in the focused pane.
- `Cmd+W` closes the active tab in the focused pane (if pane becomes empty, cleanup runs).
- `Cmd+Shift+[/]` cycles tabs within the focused pane.

No code changes expected — just verification that the pane-aware delegate pattern works correctly.

Location: `crates/editor/src/editor_state.rs` (verification only)

### Step 7: Add unit tests for switch_focus

Create tests in `crates/editor/src/workspace.rs` (in the existing `#[cfg(test)]` module):
- **Focus switch to existing neighbor**: `HSplit(Pane[A], Pane[B])`, focus A, switch right → focus B.
- **Focus switch blocked**: `HSplit(Pane[A], Pane[B])`, focus B, switch right → returns false, focus stays B.
- **Focus switch across nested tree**: `HSplit(Pane[A], VSplit(Pane[B], Pane[C]))`, focus C, switch left → focus A.
- **Focus switch vertical**: `VSplit(Pane[A], Pane[B])`, focus A, switch down → focus B.
- **Focus switch single pane**: Single pane, switch any direction → returns false.

### Step 8: Add unit tests for move_active_tab keybinding integration

Create tests in `crates/editor/src/editor_state.rs` (in the existing `#[cfg(test)]` module):
- **Tab movement creates split**: Single pane with 2 tabs, Cmd+Shift+Right → tree has 2 panes, tab moved.
- **Tab movement to existing neighbor**: `HSplit(Pane[A, B], Pane[C])`, focus A, Cmd+Shift+Right on B → B joins pane with C.
- **Tab movement rejection**: Single pane with 1 tab, Cmd+Shift+Right → no-op, tree unchanged.
- **Focus follows moved tab**: After successful move, `active_pane_id` is the target pane.

### Step 9: Add unit tests for mouse click-to-focus

Create tests in `crates/editor/src/editor_state.rs`:
- **Click unfocused pane switches focus**: Setup `HSplit(Pane[A], Pane[B])` with A focused. Click in B's content area → `active_pane_id` is B.
- **Click focused pane no-op**: Click in A when A is focused → no change, no dirty region triggered for focus.
- **Click tab bar of unfocused pane**: Click in B's tab bar region → focus switches to B, tab switching logic runs.

### Step 10: Integration verification

Run the full test suite to ensure:
- All existing tests pass (backward compatibility with single-pane workspaces).
- New keybindings don't conflict with existing shortcuts.
- Terminal tabs and agent tabs continue to work within panes.
- Welcome screen and selector overlay render relative to the focused pane.

## Dependencies

- **tiling_multi_pane_render**: The renderer already supports multiple panes with divider lines and focus indicators. This chunk adds the input handling that changes which pane is focused.
- **tiling_tab_movement**: Provides `move_tab()` and `find_target_in_direction()` functions used by this chunk.
- **tiling_workspace_integration**: Provides the pane-aware workspace model and coordinate handling pipeline.

## Risks and Open Questions

- **Arrow key conflicts on macOS**: Need to verify that `Cmd+Option+Arrow` isn't intercepted by macOS for Mission Control or window management. If so, consider alternative bindings (e.g., `Ctrl+Cmd+Arrow`). Test on a clean macOS installation.

- **Edge case: Empty pane after tab close via Cmd+W**: When `Cmd+W` closes the last tab in a focused pane, cleanup should collapse the pane and switch focus to the promoted sibling. Verify this works correctly.

- **Focus indicator update timing**: Ensure the dirty region is set correctly so the focus border updates immediately on focus switch, not on the next frame.

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
