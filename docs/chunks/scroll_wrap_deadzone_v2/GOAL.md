---
status: ACTIVE
ticket: null
parent_chunk: scroll_bottom_deadzone
code_paths:
- crates/editor/src/buffer_target.rs
- crates/editor/src/viewport.rs
code_references:
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped
    implements: "Fixed screen row to buffer line mapping by using Viewport::buffer_line_for_screen_row instead of misinterpreting screen row as buffer line index"
  - ref: crates/editor/src/viewport.rs#Viewport::buffer_line_for_screen_row
    implements: "Authoritative screen row to buffer line conversion shared between renderer and click handling"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on:
- scroll_bottom_deadzone
created_after:
- scroll_bottom_deadzone
- terminal_tab_spawn
- workspace_switching
- word_triclass_boundaries
---

# Chunk Goal

## Minor Goal

The `scroll_bottom_deadzone` chunk introduced wrap-aware scroll clamping via `set_scroll_offset_px_wrapped`, which computes `max_offset_px` from the total number of screen rows. This fix appears to have made the core problem worse for files with many wrapped lines: the viewport stops visually scrolling well before reaching the actual bottom of the file, creating a larger deadzone than existed before the fix.

### Observed Symptoms

When scrolling down through a file with many wrapped lines (e.g., a markdown file with long paragraphs):

1. **Premature scroll stop**: The viewport stops visually scrolling before the last lines of the file are visible. The user continues to scroll but the view is frozen — the scroll input is consumed with no visual effect.

2. **Unresponsive scroll-up**: After saturating the deadzone by scrolling down into it, scrolling back up does not immediately respond. The user must "unwind" the phantom scroll distance before the viewport begins moving.

3. **Click-to-cursor misalignment**: When clicking in the buffer while in the deadzone, the cursor is placed significantly below where the user clicked. One observation showed the cursor appearing ~10 lines below the click point, suggesting the deadzone had grown to roughly 10 screen rows of phantom scroll space.

### Likely Root Cause

The `set_scroll_offset_px_wrapped` implementation computes total screen rows by summing `screen_rows_for_line` across all buffer lines. If there is a systematic over-count — for example, double-counting wrapped segments, an off-by-one per line in the screen row calculation, or incorrect handling of the wrap width — the computed `max_offset_px` will be larger than the actual rendered content height. This creates a gap between the clamped maximum and the true bottom, which is exactly the deadzone.

The investigation should focus on:
- Whether `compute_total_screen_rows` agrees with what the renderer actually draws
- Whether the wrap column / wrap width used in clamping matches what the renderer uses
- Whether there's an off-by-one in `screen_rows_for_line` (e.g., counting the base row plus wraps vs. just wraps)
- Whether `visible_lines()` used in `max_rows = total_screen_rows - visible_lines` is computed in the same units as `total_screen_rows`

## Success Criteria

- Scrolling to the bottom of a file with many wrapped lines shows the last line at the bottom of the viewport with no extra dead space below
- Scrolling back up from the bottom responds immediately with no deadzone at any file size or wrap density
- Clicking in the buffer at the maximum scroll position places the cursor at the clicked location (not offset by any number of lines)
- The computed `max_offset_px` in `set_scroll_offset_px_wrapped` matches the actual rendered content height (total screen rows as drawn by the renderer minus visible rows, times line height)
- All existing scroll and viewport tests continue to pass
- Regression test: a file where wrapped lines produce significantly more screen rows than buffer lines (e.g., 2x+) scrolls correctly to the true bottom

## Relationship to Parent

The parent chunk (`scroll_bottom_deadzone`) introduced `set_scroll_offset_px_wrapped` and routed `handle_scroll` through it to fix a ~1 line deadzone at the bottom of files. The approach of using total screen rows for clamping is correct in principle, but the implementation appears to over-count screen rows, producing a `max_offset_px` that exceeds the actual content height. This creates a larger deadzone than the original bug.

What remains valid from the parent:
- The diagnosis that scroll clamping must account for wrapped lines (not just buffer line count)
- The routing of `handle_scroll` through a wrap-aware clamping path
- The test structure and test cases (though the assertions may need updating)

What needs to change:
- The screen row counting logic in `compute_total_screen_rows` or `screen_rows_for_line` — it must produce a count that exactly matches what the renderer draws
- Possibly the relationship between `visible_lines()` and the total screen row count (unit mismatch)

