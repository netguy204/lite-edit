---
decision: APPROVE
summary: All five success criteria satisfied - implementation correctly extracts mouse position from NSEvent, routes scroll to pane under cursor using calculate_pane_rects, preserves focus during hover-scroll, maintains backward compatibility for single-pane, and includes comprehensive tests.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: The `scrollWheel:` handler in `metal_view.rs` extracts the mouse location from the NSEvent and includes it in the `ScrollDelta` (or a wrapper struct) passed to the scroll handler callback.

- **Status**: satisfied
- **Evidence**: `metal_view.rs:553-575` extracts mouse position from NSEvent via `locationInWindow()`, converts to view coordinates with `convertPoint:fromView:`, applies scale factor, flips Y coordinate from NSView bottom-left to top-left origin, and calls `ScrollDelta::with_position(-dx, -dy, x_px, y_px)`. The `ScrollDelta` struct in `crates/input/src/lib.rs:139-150` adds `mouse_position: Option<(f64, f64)>` field with constructors `new()` (no position) and `with_position()` (with position).

### Criterion 2: The scroll event routing logic (`handle_scroll` in `editor_state.rs` or the controller in `main.rs`) uses the mouse position + `calculate_pane_rects` to determine which pane the cursor is over, and routes the scroll to that pane's viewport.

- **Status**: satisfied
- **Evidence**: `editor_state.rs:1688-1692` calls `find_pane_at_scroll_position(&delta)` then `scroll_pane(target_pane_id, delta)`. The `find_pane_at_scroll_position()` method at lines 1711-1772 uses `calculate_pane_rects(bounds, &workspace.pane_root)` to get pane rectangles, iterates through them with `pane_rect.contains(content_x, content_y)`, and returns the pane ID under the cursor (or default focused pane if cursor is outside content area).

### Criterion 3: When the cursor is over a non-focused pane, scrolling that pane does NOT change which pane is focused (hover-scroll, not hover-focus).

- **Status**: satisfied
- **Evidence**: The `scroll_pane()` method at lines 1776-1801 directly accesses the target pane via `ws.pane_root.get_pane_mut(target_pane_id)` and scrolls its active tab's viewport without modifying `ws.active_pane_id`. The test `test_scroll_multi_pane_hits_non_focused_pane` at lines 6527-6595 explicitly verifies: "Scroll should target pane under cursor, not focused pane" and "Focus should remain on pane1".

### Criterion 4: When only a single pane exists, behavior is unchanged from today.

- **Status**: satisfied
- **Evidence**: When only one pane exists, `find_pane_at_scroll_position()` returns the focused pane ID (line 1726), and `scroll_pane()` scrolls that pane normally. Tests `test_scroll_without_position_uses_focused_pane`, `test_find_pane_at_scroll_position_in_content_area_single_pane`, and `test_scroll_with_position_scrolls_correct_pane_single_pane_setup` verify single-pane behavior is preserved.

### Criterion 5: Existing scroll tests continue to pass; new tests verify hover-targeted scroll routing in a multi-pane layout.

- **Status**: satisfied
- **Evidence**: Running `cargo test -p lite-edit test_scroll` shows 34 scroll-related tests passing. New tests added at lines 6430-6638 include: `test_scroll_without_position_uses_focused_pane`, `test_find_pane_at_scroll_position_returns_focused_when_no_position`, `test_find_pane_at_scroll_position_outside_content_area_returns_focused`, `test_find_pane_at_scroll_position_in_rail_returns_focused`, `test_find_pane_at_scroll_position_in_content_area_single_pane`, `test_scroll_with_position_scrolls_correct_pane_single_pane_setup`, `test_scroll_delta_with_position_constructor`, `test_scroll_delta_new_has_no_position`, `test_scroll_multi_pane_hits_non_focused_pane`, and `test_scroll_multi_pane_outside_panes_returns_focused`.
