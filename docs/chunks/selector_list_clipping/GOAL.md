---
status: HISTORICAL
ticket: null
parent_chunk: selector_smooth_render
code_paths:
- crates/editor/src/renderer.rs
code_references:
  - ref: crates/editor/src/renderer.rs#selector_list_scissor_rect
    implements: "Scissor rect calculation for clipping selector list region"
  - ref: crates/editor/src/renderer.rs#full_viewport_scissor_rect
    implements: "Scissor rect reset to full viewport after list rendering"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on:
- selector_smooth_render
- selector_row_scroller
created_after:
- agent_lifecycle
- content_tab_bar
- terminal_input_encoding
- find_in_file
- cursor_wrap_scroll_alignment
- row_scroller_extract
- selector_row_scroller
- selector_smooth_render
---

# Chunk Goal

## Minor Goal

Clip file picker list item rendering to the panel's list region using a Metal
scissor rect, so that smoothly-scrolled items never escape the panel boundary.

`selector_smooth_render` introduced fractional pixel offsets: items are drawn
starting at `list_y = list_origin_y - scroll_fraction_px`. When
`scroll_fraction_px > 0`, the top item begins above `list_origin_y` and bleeds
into the query/separator area. The extra row rendered for partial bottom
visibility similarly bleeds below the panel's bottom edge. No Metal scissor rect
is currently applied, so both overflows are visible.

The fix is to bracket the selection highlight and item text draw calls in
`draw_selector_overlay` (in `renderer.rs`) with a scissor rect that constrains
rendering to the list region — from `list_origin_y` down to `panel_y +
panel_height`. The scissor is reset to the full viewport after the item list and
selection highlight are drawn.

No changes are needed to `SelectorGlyphBuffer`, `OverlayGeometry`,
`calculate_overlay_geometry`, or the scroll model — this is a pure renderer-side
fix.

## Success Criteria

- When the file picker is scrolled to a fractional position, no item text or
  selection highlight pixels appear above `list_origin_y` (i.e., over the
  separator or query row).
- No item text or selection highlight pixels appear below `panel_y +
  panel_height` (i.e., below the panel's bottom edge).
- The background rect, separator, and query text draws are unaffected — no
  scissor is applied to those phases.
- The scissor rect is reset to the full viewport after the clipped draws so
  subsequent rendering (main buffer, tab bar, etc.) is unaffected.
- The fix uses Metal's `setScissorRect` on the render command encoder, matching
  the coordinate system used by the geometry calculations (screen pixels).
- All existing renderer and selector overlay tests pass.

## Relationship to Parent

The `selector_smooth_render` chunk intentionally drew items at
`list_origin_y - scroll_fraction_px` and included an extra bottom row so smooth
scrolling would work correctly. It did not add clipping because the scissor rect
lives in the renderer, not in `SelectorGlyphBuffer`. This chunk adds the missing
renderer-side clip that was implicitly required to make that design complete.

The scroll model, geometry struct, and `SelectorGlyphBuffer` update logic from
`selector_smooth_render` remain correct and unchanged.