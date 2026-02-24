<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug occurs because after a horizontal split (vertical direction), each resulting pane has a reduced content height, but the tab's `Viewport` retains its old `visible_lines` count calculated from the pre-split full window height. The renderer correctly configures its own viewport per-pane at render time via `configure_viewport_for_pane()`, but the **tab's viewport** (which owns the authoritative scroll state) is not updated.

This causes scroll range clamping to use the wrong `visible_lines` value, making tabs that previously fit entirely in the viewport remain "at bottom" even when they now require scrolling.

**Fix strategy**: Propagate the per-pane content height to each tab's `Viewport` when pane layout changes. This mirrors the existing `sync_active_tab_viewport()` pattern but extends it to handle all panes in split layouts, not just the active tab in a single-pane layout.

Key insight: The `pane_scroll_isolation` chunk already established that each tab owns its own `Viewport` for independent scroll state. This chunk completes that work by ensuring those viewports are updated when the pane geometry changes.

**Testing approach**: Per TESTING_PHILOSOPHY.md, viewport calculations are pure Rust with no platform dependencies — this is testable without mocking. We'll write unit tests that:
1. Simulate a split by manually adjusting visible_lines
2. Verify scroll clamping produces correct bounds
3. Verify tabs that exceed the new visible_lines become scrollable

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS per-pane viewport synchronization following the subsystem's patterns. Key invariants to preserve:
  - `scroll_offset_px` is the single source of truth
  - Resize re-clamps scroll offset (Invariant #7)
  - `visible_lines` derivation from height is via `update_size()`

## Sequence

### Step 1: Write failing tests for post-split viewport behavior

Create tests in `crates/editor/src/viewport.rs` (or `crates/editor/tests/viewport_test.rs`) that verify:

1. **Test A**: After `update_size()` with reduced height, `visible_lines()` returns the smaller value
2. **Test B**: A scroll offset that was valid before resize is clamped to the new max after resize
3. **Test C**: A tab that was "at bottom" (scroll_offset_px = 0, content fit in viewport) before resize requires scrolling after resize if content now exceeds visible_lines

These tests document the expected behavior and will initially fail if the viewport isn't being updated correctly in split scenarios.

Location: `crates/editor/src/viewport.rs` (unit tests module) or `crates/editor/tests/viewport_test.rs`

### Step 2: Add `sync_pane_viewports()` method to EditorState

Create a method that iterates over all panes in the active workspace and updates each tab's viewport with the correct pane content height.

```rust
// Chunk: docs/chunks/split_scroll_viewport - Per-pane viewport synchronization
fn sync_pane_viewports(&mut self, pane_rects: &[PaneRect]) {
    // For each pane_rect:
    //   1. Find the pane by ID
    //   2. Compute pane_content_height = pane_rect.height - TAB_BAR_HEIGHT
    //   3. For each tab in the pane:
    //      - Get the buffer's line count
    //      - Call tab.viewport.update_size(pane_content_height, line_count)
}
```

This follows the pattern from `sync_active_tab_viewport()` but applies to all tabs in all panes.

Location: `crates/editor/src/editor_state.rs`

### Step 3: Call `sync_pane_viewports()` after split operations

Find the split pane entry points (likely in `editor_state.rs` or invoked from key handlers) and call `sync_pane_viewports()` after the pane tree is modified.

Split operations to find:
- `Command::SplitHorizontal` / `Command::SplitVertical` handlers
- Any workspace method like `split_pane()` or `add_pane()`

The call should occur after the pane tree is mutated and we have the new pane rects.

Location: `crates/editor/src/editor_state.rs` (command handlers) or `crates/editor/src/workspace.rs`

### Step 4: Call `sync_pane_viewports()` on window resize

When the window is resized, all pane geometries change. Update `update_viewport_dimensions()` to call `sync_pane_viewports()` after computing the new pane rects.

This ensures that existing splits react correctly to window resize, not just new splits.

Location: `crates/editor/src/editor_state.rs#update_viewport_dimensions`

### Step 5: Verify tests pass and add integration test

Run the tests from Step 1. If they pass, add an integration test that:
1. Creates a multi-pane layout (horizontal split)
2. Adds a buffer with more lines than fit in the split pane's visible_lines
3. Verifies that scroll input moves the viewport (scrollability)
4. Verifies that the existing scroll offset is clamped correctly

Location: `crates/editor/tests/viewport_test.rs`

### Step 6: Update GOAL.md code_paths

Add the files touched to the chunk's GOAL.md frontmatter:
- `crates/editor/src/editor_state.rs`
- `crates/editor/src/viewport.rs` (if tests added there)
- `crates/editor/tests/viewport_test.rs`

## Dependencies

- **pane_scroll_isolation** (ACTIVE): This chunk relies on the per-tab viewport ownership established in that chunk. The `configure_viewport_for_pane()` method and `set_visible_lines()` API exist from that work.
- **viewport_emacs_navigation** (ACTIVE): Uses `visible_lines()` for Page Up/Down calculations. After this fix, those will use correct per-pane values.

## Risks and Open Questions

1. **Performance**: Iterating all tabs in all panes on every resize might be noticeable with many tabs. However, tab counts are typically small (<100), and `update_size()` is O(1). Should be fine.

2. **Terminal tabs**: Terminal tabs have their own viewport behavior (auto-follow, scrollback). Need to verify that calling `update_size()` on terminal tab viewports doesn't break auto-follow behavior. The existing `terminal_viewport_init` chunk handles initial setup; we should use the same pattern.

3. **Wrapped mode**: For soft-wrapped content, `visible_lines` is based on screen rows, not buffer lines. The fix should work correctly because `update_size()` computes visible_lines from pixel height, which is independent of wrap state. The scroll clamping will use the appropriate line count.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->