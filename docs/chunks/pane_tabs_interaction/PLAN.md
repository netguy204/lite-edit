<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Fix two related bugs that make tabs in non-top-left panes unresponsive after
splitting. Both defects stem from the assumption that there is exactly one
tab bar at the top of the content area.

The fix uses the existing `calculate_pane_rects` function to enumerate all
pane rectangles and check each pane's tab bar region, rather than assuming
a single tab bar at `y < TAB_BAR_HEIGHT`.

## Sequence

### Step 1: Fix click routing in handle_mouse

**Location:** `crates/editor/src/editor_state.rs` — `handle_mouse`

The gate `if screen_y < TAB_BAR_HEIGHT` routes clicks to `handle_tab_bar_click`
only when the y-coordinate is within the top `TAB_BAR_HEIGHT` pixels of the
window. This is incorrect for split layouts where other panes' tab bars are
at different y-coordinates.

Replace the simple y-coordinate check with a loop through all pane rects
that checks if the click is within any pane's tab bar region:
- Calculate pane rects using `calculate_pane_rects`
- Check if click is within `[pane.y, pane.y + TAB_BAR_HEIGHT)` for any pane
- If so, route to `handle_tab_bar_click`

### Step 2: Fix cursor regions in update_cursor_regions

**Location:** `crates/editor/src/drain_loop.rs` — `update_cursor_regions`

A single pointer cursor rect is added covering only the top-left pane's tab bar.
In split layouts, other panes' tab bars receive no pointer rect, so the cursor
stays as an I-beam over them.

Replace the single pointer rect with a loop through all pane rects that adds
a pointer cursor region for each pane's tab bar:
- Calculate pane rects using `calculate_pane_rects`
- For each pane, add a pointer cursor rect covering the top `TAB_BAR_HEIGHT`
- Convert from screen-space (y=0 at top) to NSView coords (y=0 at bottom)

### Step 3: Add unit tests for full click dispatch path

**Location:** `crates/editor/src/editor_state.rs` — tests module

The existing `split_tab_click` tests call `handle_tab_bar_click` directly.
Add new tests that call `handle_mouse` to verify the full dispatch path:
- `test_handle_mouse_routes_to_bottom_pane_tab_bar` — vertical split
- `test_handle_mouse_routes_to_right_pane_tab_bar` — horizontal split
- `test_handle_mouse_routes_to_top_left_pane_tab_bar` — regression test
- `test_handle_mouse_routes_to_single_pane_tab_bar` — regression test

### Step 4: Verify no regressions

Run all existing tests to ensure single-pane layouts and `split_tab_click`
tests continue to pass.

## Deviations

None — implementation followed the plan exactly.
