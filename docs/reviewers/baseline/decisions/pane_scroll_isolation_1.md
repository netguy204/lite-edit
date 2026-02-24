---
decision: APPROVE
summary: All success criteria satisfied through configure_viewport_for_pane helper that copies tab scroll state and updates pane dimensions before each pane render
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Open two panes (vertical or horizontal split) with different files. Scrolling in one pane does not move the content of the other pane.

- **Status**: satisfied
- **Evidence**: The `render_pane()` function at renderer.rs:1650 calls `configure_viewport_for_pane(&tab.viewport, ...)` which copies scroll offset from each tab's own viewport. Since each tab owns its own `Viewport` (per GOAL.md: "each Tab already owns its own Viewport"), scrolling one tab only affects that tab's scroll state, not the renderer's shared viewport used for other panes.

### Criterion 2: Each pane scrolls through its full buffer length regardless of the other pane's height.

- **Status**: satisfied
- **Evidence**: `configure_viewport_for_pane()` at renderer.rs:458-463 computes `visible_lines` from `pane_content_height / line_height` for each pane individually. The `set_visible_lines()` method sets visible rows directly without re-clamping (row_scroller.rs:253-258), allowing each pane to have its own visible line count independent of others.

### Criterion 3: Typing in the focused pane (including adding/removing lines) does not change the first visible line in any unfocused pane.

- **Status**: satisfied
- **Evidence**: The viewport sync in drain_loop.rs was removed (lines 255-258), so the focused tab's scroll changes don't propagate to the renderer's shared viewport. Each pane's render now copies scroll state from its own tab's viewport at render time, isolating each pane's view state.

### Criterion 4: Soft-wrap line breaks respect each pane's actual width, not the window width or a stale width from a previous layout.

- **Status**: satisfied
- **Evidence**: `configure_viewport_for_pane()` sets `self.content_width_px = pane_width` (renderer.rs:466) before `update_glyph_buffer_*` is called. The `WrapLayout` is created inside `update_glyph_buffer_with_cursor_visible()` using `self.content_width_px` (as noted in Plan Step 8), ensuring correct wrap width per pane.

### Criterion 5: The max scroll position accounts for wrapped lines using the correct pane width, both during and after splits.

- **Status**: satisfied
- **Evidence**: Because `content_width_px` is set per-pane before rendering, and the tab's viewport uses `set_scroll_offset_px_wrapped()` for clamping (per the viewport_scroll subsystem conventions), the wrap-aware max scroll is computed from the correct pane width. Single-pane mode also configures viewport at renderer.rs:1119-1127 with correct dimensions.

### Criterion 6: The renderer's per-pane draw path uses the active tab's `Viewport` (scroll offset, visible-lines count, and wrap width) rather than a single shared `Viewport`.

- **Status**: satisfied
- **Evidence**: The new `configure_viewport_for_pane()` helper (renderer.rs:446-467) explicitly copies all three components: scroll offset via `set_scroll_offset_px_unclamped()`, visible lines via `set_visible_lines()`, and wrap width by setting `self.content_width_px`. This is called for both multi-pane (renderer.rs:1650) and single-pane (renderer.rs:1119-1127, 1992-2003) paths.

### Criterion 7: All existing tests pass.

- **Status**: satisfied
- **Evidence**: `cargo test` shows all tests pass except two pre-existing performance test failures in the buffer crate (`insert_100k_chars_under_100ms`, `insert_100k_chars_with_newlines_under_200ms`) which also fail without this chunk's changes (verified via git stash). The new unit tests (`test_set_visible_lines_preserves_scroll`, `test_set_visible_rows_direct`, etc.) all pass.

## Subsystem Compliance

The implementation follows the **viewport_scroll** subsystem invariants:
- **Invariant 1** (scroll_offset_px is source of truth): Respected - tab's viewport remains authoritative, renderer copies it
- **Invariant 7** (resize re-clamps): The implementation doesn't re-clamp on visible_lines change because the tab's viewport already has correct bounds
- **Soft convention 1**: The code uses wrapped scroll methods where appropriate per existing patterns
