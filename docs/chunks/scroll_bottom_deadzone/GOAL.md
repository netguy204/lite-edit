---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/row_scroller.rs
- crates/editor/src/viewport.rs
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/editor/src/viewport.rs#Viewport::set_scroll_offset_px_wrapped
    implements: "Wrap-aware scroll clamping using total screen rows"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::handle_scroll
    implements: "Uses wrap-aware clamping to fix scroll deadzone at bottom"
  - ref: crates/editor/src/viewport.rs#tests::test_set_scroll_offset_px_wrapped_clamps_to_screen_rows
    implements: "Verifies wrapped content clamps based on screen rows"
  - ref: crates/editor/src/viewport.rs#tests::test_scroll_at_max_wrapped_responds_immediately
    implements: "Regression test for scroll deadzone fix"
  - ref: crates/editor/src/buffer_target.rs#tests::test_scroll_at_max_wrapped_responds_to_scroll_up
    implements: "Integration test for scroll deadzone fix"
  - ref: crates/editor/src/buffer_target.rs#tests::test_click_at_max_scroll_wrapped_maps_correctly
    implements: "Regression test for click-to-cursor at max scroll"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- tab_bar_content_clip
- click_scroll_fraction_alignment
---

# Chunk Goal

## Minor Goal

Fix two related bugs that occur when scrolled to the bottom of a file:

1. **Scroll deadzone at bottom**: When scrolled to the maximum position (bottom of file) and then scrolling back up, there is approximately one line-height of scroll input that is consumed before the viewport actually begins moving. Scrolling responds instantly at all other positions in the file.

2. **Click-to-cursor off-by-one at bottom**: When at the maximum scroll position, clicking in the buffer places the cursor approximately one line below where the user clicked. This does not occur at other scroll positions.

### Root Cause Analysis

Both symptoms point to the same underlying issue: a mismatch between how scroll offset is clamped and how the renderer/hit-testing interprets the scroll position at the maximum bound.

`RowScroller::set_scroll_offset_px` clamps the scroll offset to:
```
max_offset_px = (row_count - visible_rows) * row_height
```

When `handle_scroll` in `BufferFocusTarget` calls `viewport.set_scroll_offset_px(new_px, line_count)`, it passes **buffer line count**. But when line wrapping is enabled, the scroll offset operates in **screen row** space (via `ensure_visible_wrapped` and `first_visible_screen_row`). The clamping formula doesn't account for wrapped lines producing more screen rows than buffer lines.

Even without wrapping, there may be an off-by-one in how the max scroll position interacts with `pixel_to_buffer_position_wrapped`. At max scroll, `first_visible_line()` returns `line_count - visible_lines`. The hit-test function walks from `first_visible_line` forward, but if the clamped position leaves a gap between where the viewport thinks it is and where the renderer actually draws, clicks will land one row off.

The scroll deadzone occurs because the user scrolls past the "true" max during the down phase (the offset gets clamped but continues to be "virtually" past max), and then must scroll back through that phantom distance before the clamped offset actually decreases.

### Fix Direction

Ensure the max scroll offset correctly accounts for the actual content height (screen rows with wrapping, or buffer lines without). The hit-test coordinate mapping and the scroll clamping must agree on the same maximum position so that:
- At max scroll, the last line sits at the bottom of the viewport with no extra dead space
- Click-to-cursor mapping at max scroll uses the same coordinate basis as the renderer

## Success Criteria

- Scrolling to the bottom of a file and then scrolling back up responds immediately with no deadzone
- Clicking in the buffer when scrolled to the bottom positions the cursor at the clicked line (not one line below)
- Existing scroll behavior at non-bottom positions is unaffected
- Existing viewport tests continue to pass
- Regression tests cover both the scroll deadzone and the click offset at max scroll