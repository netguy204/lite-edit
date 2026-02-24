<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is in `EditorState::handle_tab_bar_click`. The current implementation
incorrectly handles multi-pane layouts by:

1. Checking only if the click is in `y < TAB_BAR_HEIGHT` in global screen space
2. Using `tabs_from_workspace()` which returns tabs from the **active** pane only
3. Using `calculate_tab_bar_geometry()` which assumes tabs start at `RAIL_WIDTH`

In a split pane layout, each pane has its own tab bar at its own position.
Clicking on a tab bar in a non-focused pane needs to:
1. Determine which pane's tab bar was clicked (hit-test against pane rectangles)
2. Switch focus to that pane if it's not already focused
3. Calculate geometry for **that pane's** tab bar, not the global tab bar
4. Switch to the clicked tab within that pane

**Strategy**: Follow the existing pattern from `handle_mouse_buffer` which
already does pane hit-testing for click-to-focus. Extend the tab bar click
handling to use pane-specific geometry via `calculate_pane_tab_bar_geometry`
and `tabs_from_pane`.

This fix builds on:
- `tiling_multi_pane_render` - Established `calculate_pane_tab_bar_geometry`
  and `tabs_from_pane` for per-pane tab bars
- `tiling_focus_keybindings` - Established click-to-focus pane switching pattern
- `pane_layout.rs` - Provides `calculate_pane_rects` and `PaneRect::contains`

## Sequence

### Step 1: Write failing tests for multi-pane tab click routing

Following TDD, write tests that express the expected behavior before fixing.
The tests should verify:
- Clicking tab in top pane of vertical split activates tab in top pane only
- Clicking tab in bottom pane of vertical split activates tab in bottom pane only
- Clicking tab in left pane of horizontal split activates tab in left pane only
- Clicking tab in right pane of horizontal split activates tab in right pane only
- Clicking tab in inactive pane switches focus to that pane AND activates the tab
- Close button clicks in inactive pane close the tab in that pane

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 2: Add helper to find pane at screen coordinates for tab bar region

Create a helper function that, given screen coordinates (x, y) in the tab bar
region of any pane, returns the `PaneId` of the pane containing that point.

This requires:
1. Calculating pane rects using bounds that start at `(RAIL_WIDTH, 0.0)`
   (the renderer bounds) rather than `(0.0, 0.0)` (the content-local bounds
   used for buffer hit-testing)
2. Checking if the y-coordinate is within that pane's tab bar height
3. Returning the pane ID if found

Signature:
```rust
fn find_pane_at_tab_bar_click(&self, screen_x: f32, screen_y: f32) -> Option<PaneId>
```

Location: `crates/editor/src/editor_state.rs`

### Step 3: Refactor handle_tab_bar_click for multi-pane support

Replace the current single-pane implementation with one that:

1. Calls `find_pane_at_tab_bar_click` to determine which pane was clicked
2. If no pane found, return early (click was outside all pane tab bars)
3. Get the pane's rectangle from `calculate_pane_rects`
4. Build `TabInfo` list from the clicked pane using `tabs_from_pane`
5. Calculate geometry using `calculate_pane_tab_bar_geometry` with the
   pane's position and dimensions
6. If a tab was clicked:
   - If this pane is not the active pane, switch `active_pane_id` first
   - Then call `switch_tab` to activate the clicked tab
7. If close button was clicked:
   - If this pane is not the active pane, switch `active_pane_id` first
   - Then call `close_tab` for that pane

Key imports to add:
```rust
use crate::tab_bar::{calculate_pane_tab_bar_geometry, tabs_from_pane};
use crate::pane_layout::calculate_pane_rects;
```

Location: `crates/editor/src/editor_state.rs`

### Step 4: Add pane-specific close_tab variant

Currently `EditorState::close_tab(index)` operates on the active pane.
For closing tabs in a specific pane (which may not be active), we need either:

Option A: A method `close_tab_in_pane(pane_id, index)` that:
- Finds the pane by ID
- Calls `pane.close_tab(index)` directly
- Handles empty pane cleanup if needed

Option B: Switch active pane first, then close, which maintains the existing
invariant that mutations happen through the active pane.

Choose Option B (switch-then-close) for consistency with existing code patterns.
This means the refactored `handle_tab_bar_click` naturally handles close buttons
by first switching focus if needed, then calling `close_tab`.

### Step 5: Verify tests pass and add edge case tests

Run the tests from Step 1. If they pass, add additional edge cases:
- Single-pane layout still works (no regression)
- Tab bar horizontal scroll offset is pane-specific
- Clicking in the divider region between panes does nothing

Location: `crates/editor/src/editor_state.rs`

## Dependencies

This chunk depends on completed work from:
- `tiling_multi_pane_render` - Provides `calculate_pane_tab_bar_geometry`, `tabs_from_pane`
- `tiling_workspace_integration` - Provides pane tree and `active_pane_id` management
- `tiling_tree_model` - Provides `PaneLayoutNode`, `calculate_pane_rects`

All these chunks are ACTIVE, so no blocking dependencies.

## Risks and Open Questions

- **Coordinate system complexity**: The renderer calculates pane rects with
  bounds starting at `(RAIL_WIDTH, 0.0)`, but `handle_mouse_buffer` uses
  content-local coordinates starting at `(0.0, 0.0)`. Need to ensure the
  tab bar click handler uses renderer-consistent bounds to match visual
  pane positions.

- **Focus change side effects**: When clicking a tab in an inactive pane,
  switching focus may trigger viewport syncing and dirty region updates.
  Need to ensure these are triggered correctly.

- **Tab bar view offset**: Each pane has its own `tab_bar_view_offset`.
  The geometry calculation must use the clicked pane's offset, not the
  active pane's offset.

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

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->