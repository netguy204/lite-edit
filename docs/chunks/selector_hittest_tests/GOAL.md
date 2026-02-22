---
status: FUTURE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/selector.rs
  - crates/editor/src/editor_state.rs
code_references: []
narrative: file_picker_viewport
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: ["selector_row_scroller", "selector_smooth_render"]
created_after: ["renderer_styled_content", "terminal_emulator", "terminal_file_backed_scrollback", "workspace_model", "file_picker_mini_buffer", "mini_buffer_model"]
---

<!--
╔══════════════════════════════════════════════════════════════════════════════╗
║  DO NOT DELETE THIS COMMENT BLOCK until the chunk complete command is run.   ║
║                                                                              ║
║  AGENT INSTRUCTIONS: When editing this file, preserve this entire comment    ║
║  block. Only modify the frontmatter YAML and the content sections below      ║
║  (Minor Goal, Success Criteria, Relationship to Parent). Use targeted edits  ║
║  that replace specific sections rather than rewriting the entire file.       ║
╚══════════════════════════════════════════════════════════════════════════════╝
-->

# Chunk Goal

## Minor Goal

Harden the file picker's click hit-testing with property-based tests that prove
correctness across the full space of scroll positions, item heights, and list
geometries — and add explicit regression tests for both original bugs (coordinate
flip and scroll rounding) so they can never regress silently.

By this point the model (`selector_row_scroller`) and the renderer
(`selector_smooth_render`) are both correct. This chunk adds the tests that
verify they stay correct.

**Property: clicking the centre of a rendered row selects exactly that row.**

The renderer places item `i` (where `i` is relative to `first_visible_item()`)
at:
```
y = list_origin_y - scroll_fraction_px + i * item_height
```
The vertical centre of that row is at `y + item_height / 2`. The hit-test must
map that pixel back to item `first_visible_item() + i`. Test this for:
- Several values of `scroll_offset_px` (including non-zero fractional parts)
- A range of `item_height` values (the font metric can vary)
- Clicking the first visible item, a middle item, and the last visible item

**Regression: coordinate-flip bug.**

Given `view_height`, a click near the top of the overlay (large raw y, small
flipped y) must select a row near the top of the list. Before `selector_coord_flip`
this would produce an index far above row 0. Test with the literal coordinate
formula from `buffer_target.rs` to pin the expected behaviour:
```
flipped_y = view_height - raw_y
relative_y = flipped_y - list_origin_y_flipped + scroll_fraction_px
row = floor(relative_y / item_height)
```

**Regression: scroll-rounding bug.**

Apply a sequence of small scroll deltas (each less than `item_height`) and assert
that the accumulated `scroll_offset_px` equals the sum of the deltas — i.e., no
fractional part is discarded. Before `selector_row_scroller` each delta was
rounded to the nearest row, so a sequence of 0.5-row deltas would produce zero
net scroll.

**Boundary conditions to cover explicitly:**
- Clicking exactly on a row boundary (top pixel of a row) selects that row, not
  the one above.
- Clicking when `scroll_fraction_px == 0` still selects correctly (no
  off-by-one).
- Clicking below the last rendered item is a no-op (out-of-bounds guard).
- Clicking above `list_origin_y` is a no-op.

## Success Criteria

- A parameterised test asserts that clicking the pixel centre of any rendered
  row (indexed 0 through `visible_items - 1`) selects exactly
  `first_visible_item() + row_index`, for at least three distinct
  `scroll_offset_px` values with non-zero fractional parts.
- A regression test for the coordinate-flip bug demonstrates that a click with
  raw macOS y near the top of the screen (e.g. `raw_y = view_height - 5.0`)
  maps to the topmost visible item.
- A regression test for the scroll-rounding bug demonstrates that ten
  applications of a `0.4 * item_height` delta produce `scroll_offset_px ==
  4.0 * item_height` (i.e., exactly 4 rows accumulated with no rounding loss).
- All previously passing selector tests continue to pass.
- No new `#[allow(...)]` suppressions are introduced to make tests compile.
