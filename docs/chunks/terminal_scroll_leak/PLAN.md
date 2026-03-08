<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is caused by a two-part interaction between `sync_active_tab_viewport()` and `sync_pane_viewports()` in `crates/editor/src/editor_state.rs`.

**Root cause**: `sync_active_tab_viewport()` (line ~869) uses the full window height (`self.view_height - TAB_BAR_HEIGHT`) to compute `visible_lines`, regardless of whether the active tab is in a split pane. In a multi-pane layout, this gives buffer tabs an inflated `visible_rows` count. When the user scrolls to "bottom" with this inflated count, they reach `max_offset = (line_count - V_full) * line_height`. Later, when `sync_pane_viewports()` corrects the viewport to the actual pane height, `visible_rows` decreases to the correct value and the viewport suddenly shows fewer lines — making it appear the buffer scrolled up by one page.

**Event sequence that triggers the bug**:
1. Vertical split exists: terminal left, buffer right
2. User interacts with the right buffer pane → `sync_active_tab_viewport()` sets viewport to full window height (too tall)
3. User scrolls buffer to bottom (using the inflated max_offset)
4. User switches to left pane and creates a new terminal → `new_terminal_tab()` calls `sync_pane_viewports()` (line 5320)
5. `sync_pane_viewports()` corrects the right buffer's viewport to actual pane height → `visible_rows` halves → user sees fewer lines from the same scroll position → content appears to jump up

**Fix strategy** (two complementary changes):

1. **Primary fix**: Make `sync_active_tab_viewport()` pane-aware. When the active tab is in a multi-pane layout, use the actual pane content height instead of the full window height. This prevents the viewport from ever being set to incorrect dimensions.

2. **Defense in depth**: In `sync_pane_viewports()`, skip `viewport.update_size()` for non-terminal tabs whose `visible_rows` would not change. This ensures that even if a viewport is already at the correct dimensions, redundant `update_size` calls don't trigger unnecessary re-clamping.

The `viewport_scroll` subsystem's Hard Invariant #7 ("Resize re-clamps scroll offset") is correct for actual dimension changes. The issue is that `sync_active_tab_viewport()` is setting incorrect dimensions, and then the correction triggers an unwanted re-clamp. The fix ensures dimensions are always correct, so re-clamping only happens when pane geometry genuinely changes.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport scroll subsystem. It operates on `Viewport::update_size` and `RowScroller::update_size`, which implement Hard Invariant #7 (resize re-clamps). The fix preserves this invariant — we're correcting the input dimensions, not changing the clamping behavior. No deviation from subsystem patterns.

## Sequence

### Step 1: Write failing tests that reproduce the bug

Add tests to the existing test module in `crates/editor/src/editor_state.rs` that capture the two aspects of the bug:

**Test A — `test_new_terminal_preserves_sibling_buffer_scroll`**:
1. Set up an `EditorState` with view dimensions (e.g., 800×600)
2. Open a buffer with many lines (e.g., 200 lines) and scroll it to the bottom
3. Create a vertical split (move tab to create two panes)
4. Call `sync_pane_viewports()` to establish correct pane heights
5. Record the right buffer's `scroll_offset_px`
6. Call `new_terminal_tab()` (creates terminal in the left pane)
7. Assert the right buffer's `scroll_offset_px` is unchanged

**Test B — `test_sync_active_tab_viewport_uses_pane_height`**:
1. Set up an `EditorState` with a vertical split
2. Call `sync_pane_viewports()` to establish correct pane heights
3. Record the right buffer's `visible_lines` (should be based on half-height pane)
4. Switch focus to the right pane
5. Call `sync_active_tab_viewport()`
6. Assert `visible_lines` is unchanged (still based on pane height, not full window)

Location: `crates/editor/src/editor_state.rs` (in the `#[cfg(test)]` module, near existing `sync_pane_viewports` tests around line 10734)

### Step 2: Make `sync_active_tab_viewport()` pane-aware

Modify `sync_active_tab_viewport()` (line ~869) to compute content height from the active pane's geometry rather than the full window height.

Current code:
```rust
let content_height = view_height - TAB_BAR_HEIGHT;
self.viewport_mut().update_size(content_height, line_count);
```

New approach:
1. Get the active pane ID from the workspace
2. Call `self.get_pane_content_dimensions(pane_id)` to get the actual pane height
3. Use the pane content height for `update_size`
4. Fall back to `view_height - TAB_BAR_HEIGHT` only in single-pane layouts (where pane height == window height)

This mirrors the pattern already used in `new_terminal_tab()` at lines 5229-5239, which calls `get_pane_content_dimensions()` for pane-aware sizing.

Location: `crates/editor/src/editor_state.rs`, `sync_active_tab_viewport()` method (line ~869)

### Step 3: Add dimension-change guard to `sync_pane_viewports()`

In `sync_pane_viewports()` (line ~907), add a guard before calling `tab.viewport.update_size()` for non-terminal tabs. Skip the call if the pane content height would produce the same `visible_rows` count.

Current code (line 979):
```rust
tab.viewport.update_size(pane_content_height, line_count);
```

New approach:
```rust
// For non-terminal tabs, skip update if visible_rows wouldn't change.
// This prevents unnecessary scroll re-clamping when sibling panes
// change but this pane's dimensions are stable.
let new_visible = (pane_content_height / tab.viewport.line_height()).floor() as usize;
if tab.viewport.visible_lines() != new_visible {
    tab.viewport.update_size(pane_content_height, line_count);
}
```

Terminal tabs always get `update_size` called (they already have a dimension-change guard for the PTY resize at line 961, and their viewport sync is important for correct PTY sizing).

Location: `crates/editor/src/editor_state.rs`, `sync_pane_viewports()` method, inside the per-tab loop (line ~978)

### Step 4: Verify tests pass and add edge case tests

Run the tests from Step 1 to confirm they pass. Then add additional edge case tests:

**Test C — `test_buffer_scroll_preserved_across_sibling_close`**:
1. Set up a vertical split with buffer in right pane scrolled to bottom
2. Close the left pane's tab (collapsing the split)
3. Assert the buffer's scroll position adjusts correctly for the new (larger) viewport without jumping to an unexpected position

**Test D — `test_sync_active_tab_viewport_single_pane_unchanged`**:
1. Set up a single-pane layout (no split)
2. Verify `sync_active_tab_viewport()` still uses `view_height - TAB_BAR_HEIGHT` (the fallback path)
3. This ensures the pane-aware change doesn't regress single-pane behavior

Location: `crates/editor/src/editor_state.rs` (test module)

### Step 5: Add chunk backreferences

Add backreference comments to the modified code:

- On `sync_active_tab_viewport()`: `// Chunk: docs/chunks/terminal_scroll_leak - Pane-aware viewport sync`
- On the dimension-change guard in `sync_pane_viewports()`: `// Chunk: docs/chunks/terminal_scroll_leak - Skip redundant viewport update for stable panes`

## Risks and Open Questions

- **Wrap layout interaction**: In vertical splits, pane width changes affect soft line wrapping, which changes the effective "screen row count." `viewport.update_size()` only takes height and line_count (not wrapped row count). If wrapping is enabled, skipping `update_size` based on `visible_rows` alone should still be correct because `update_size` doesn't recalculate wrap state — that happens via `set_scroll_offset_px_wrapped`. Verify this doesn't cause issues with wrapped buffers in splits.

- **get_pane_content_dimensions availability**: `get_pane_content_dimensions()` may return `None` if the pane tree hasn't been laid out yet (e.g., during initial setup). The fallback to full window height handles this case, but verify the early startup sequence works correctly.

- **Terminal viewport in sync_active_tab_viewport**: The current code early-returns for terminal tabs (`None => return` at line 884). The pane-aware change should preserve this behavior since it only affects the content_height computation, which comes before the terminal check.

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
