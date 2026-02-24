<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug occurs in `scroll_pane()` where the `EditorContext` is created with the **full
window dimensions** (`self.view_height - TAB_BAR_HEIGHT`, `self.view_width - RAIL_WIDTH`)
instead of the **pane-specific dimensions**.

When scrolling in a split pane, the `EditorContext.wrap_layout()` method uses the full
window width to compute line wrapping, which affects the `set_scroll_offset_px_wrapped()`
clamping logic. The maximum scroll offset is computed as:
`max_offset_px = (total_screen_rows - visible_rows) * line_height`

While `visible_rows` is correctly computed per-pane (via `sync_pane_viewports`), the
`total_screen_rows` calculation uses a `WrapLayout` constructed with the wrong viewport
width, causing it to under-count wrapped lines for narrower panes. This prevents
scrolling to the end of the document.

**Fix**: Compute the target pane's content dimensions before creating `EditorContext` in
`scroll_pane()`. This requires:
1. Looking up the pane's rect from `calculate_pane_rects()`
2. Computing `pane_content_height = pane_rect.height - TAB_BAR_HEIGHT`
3. Computing `pane_content_width = pane_rect.width`
4. Passing these to `EditorContext::new()` instead of full-window dimensions

This aligns with the pattern already used in `sync_pane_viewports()` and follows the
`viewport_scroll` subsystem's invariants.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll
  subsystem's `set_scroll_offset_px_wrapped()` method. The bug occurs because the
  `WrapLayout` passed to this method was constructed with incorrect dimensions. The fix
  ensures correct pane-local dimensions are used, aligning with the subsystem's
  Invariant #2: "Scroll offset is clamped to `[0.0, max_offset_px]`".

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem's
  pane rect calculation via `calculate_pane_rects()`. No deviations from renderer
  patterns; we're adding a lookup step that mirrors existing patterns in
  `sync_pane_viewports()`.

## Sequence

### Step 1: Create helper to get pane content dimensions

Add a helper method `get_pane_content_dimensions()` to `EditorState` that:
1. Takes a `PaneId`
2. Calls `calculate_pane_rects()` to get the layout
3. Finds the pane's rect and computes content dimensions:
   - `content_height = pane_rect.height - TAB_BAR_HEIGHT`
   - `content_width = pane_rect.width`
4. Returns `Option<(f32, f32)>` (height, width) or `None` if pane not found

Location: `crates/editor/src/editor_state.rs`

### Step 2: Modify scroll_pane to use pane-specific dimensions

In `scroll_pane()`:
1. Call `get_pane_content_dimensions(target_pane_id)` before creating `EditorContext`
2. Use the returned pane dimensions instead of full-window dimensions
3. Fall back to full-window dimensions if pane not found (defensive programming)

The key change:
```rust
// Before (buggy):
let content_height = self.view_height - TAB_BAR_HEIGHT;
let content_width = self.view_width - RAIL_WIDTH;

// After (fixed):
let (content_height, content_width) = self.get_pane_content_dimensions(target_pane_id)
    .unwrap_or((self.view_height - TAB_BAR_HEIGHT, self.view_width - RAIL_WIDTH));
```

Location: `crates/editor/src/editor_state.rs`

### Step 3: Add unit test for vertical split scroll bounds

Add a test that:
1. Creates an EditorState with a long file (e.g., 100 lines)
2. Creates a vertical split (two panes stacked)
3. Sets up proper viewport dimensions for both panes
4. Attempts to scroll the bottom pane to the end of the document
5. Asserts that the scroll position reaches the expected max offset

The test should verify that:
- Each pane uses its own height for scroll clamping
- A 50% vertical split allows scrolling based on half the window height
- The last line of the document is reachable in both panes

Location: `crates/editor/src/editor_state.rs` (test module)

### Step 4: Add regression test for wrapping with narrow panes

Add a test that:
1. Creates an EditorState with a file containing long lines (that will wrap)
2. Creates a horizontal split (side-by-side panes, each narrower than full window)
3. Verifies that the narrower pane computes wrap correctly:
   - More screen rows due to additional line wrapping in narrow pane
   - Higher max scroll offset to reach the document end

This tests the `view_width` parameter is correctly passed to `WrapLayout`.

Location: `crates/editor/src/editor_state.rs` (test module)

## Dependencies

None. This chunk fixes existing code and does not depend on other chunks.

## Risks and Open Questions

- **Performance**: `calculate_pane_rects()` is called on every scroll event. This should
  be fast (it's just geometry math), but worth monitoring. If it becomes a bottleneck,
  we could cache the pane rects and invalidate on resize/split events.

- **RAIL_WIDTH handling**: Confirmed that `calculate_pane_rects()` is called with
  `content_width = view_width - RAIL_WIDTH`, meaning pane rects are already in
  content-local coordinates (post-rail). We should use `pane_rect.width` directly
  as the `view_width` for `EditorContext` without any additional `RAIL_WIDTH` subtraction.

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