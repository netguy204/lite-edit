---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_size
    implements: "Computes content_height by subtracting TAB_BAR_HEIGHT before passing to viewport"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_dimensions
    implements: "Computes content_height by subtracting TAB_BAR_HEIGHT before passing to viewport"
  - ref: crates/editor/src/editor_state.rs#test_visible_lines_accounts_for_tab_bar
    implements: "Regression test verifying visible_lines is computed from content_height"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- selector_hittest_tests
- selector_smooth_render
- resize_click_alignment
- tab_bar_interaction
---

# Chunk Goal

## Minor Goal

Fix a bug where the editor cannot be scrolled to a position that fully reveals the
last line of the buffer. When scrolled all the way down, the final line is clipped
at the bottom edge of the viewport.

### Root Cause

`EditorState::update_viewport_dimensions` (and `update_viewport_size`) calls
`viewport.update_size(window_height, line_count)` with the **full** window height.
However, the text content area is only `window_height - TAB_BAR_HEIGHT` pixels tall
because the tab bar occupies space at the top of the window.

`Viewport::update_size` derives `visible_lines = floor(window_height / line_height)`,
which overcounts by roughly one line (since `TAB_BAR_HEIGHT ≈ line_height`). The
maximum scroll position is then clamped to:

```
max_offset_px = (row_count - visible_lines) * line_height
```

Because `visible_lines` is too large, `max_offset_px` is too small — the user
cannot scroll far enough to bring the last line fully into view.

### Fix

Pass `content_height = window_height - TAB_BAR_HEIGHT` to `viewport.update_size()`
in both `update_viewport_size` and `update_viewport_dimensions`. This gives the
viewport an accurate picture of the usable pixel area, so `visible_lines` and the
derived max scroll bound are correct.

Note: the `view_height` field stored on `EditorState` must remain the **full**
window height, as it is used for mouse-coordinate flipping and tab-bar/rail
hit-testing. Only the value forwarded to `viewport.update_size()` changes.

## Success Criteria

- Scrolling to the maximum position fully reveals the last line of the buffer
  with no clipping at the bottom edge.
- A regression test verifies that `visible_lines` is computed from the content
  area height (`window_height - TAB_BAR_HEIGHT`), not the full window height.
- Existing tests continue to pass; the fix does not regress click-to-cursor
  alignment or the resize re-clamp behavior from `resize_click_alignment`.
- The selector overlay geometry, mouse-coordinate flipping, and tab-bar/rail
  hit-testing continue to use the full `view_height` / `view_width` values.