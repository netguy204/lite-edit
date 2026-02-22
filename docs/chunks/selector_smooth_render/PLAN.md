<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk modifies `SelectorGlyphBuffer::update_from_widget` in
`crates/editor/src/selector_overlay.rs` to consume the fractional scroll state
from `SelectorWidget`, enabling smooth sub-row scrolling in the file picker
list. The changes mirror the pattern already established in the main buffer
renderer.

**Strategy:**

1. **Read `scroll_fraction_px()` from the widget** — The `selector_row_scroller`
   chunk already exposes this method on `SelectorWidget`.

2. **Offset list origin by fractional scroll** — Compute `list_y = list_origin_y
   - frac` so the top item is partially clipped when `scroll_fraction_px > 0`.

3. **Use `visible_item_range()` for the item loop** — This delegates range
   calculation to `RowScroller`, which already includes the +1 extra row for
   partial bottom visibility.

4. **Compute each item's Y from the draw index** — Use `draw_idx` (loop index)
   rather than the absolute item index, since we're iterating over a slice that
   already starts at `first_visible_item()`.

5. **Apply the same offset to the selection highlight** — The highlight quad Y
   must be computed using `list_y` and the visible offset of `selected_index`
   relative to `first_visible_item()`.

**What changes:**
- `SelectorGlyphBuffer::update_from_widget()`: Item text rendering and selection
  highlight positioning.

**What does NOT change:**
- `calculate_overlay_geometry()` — No changes needed.
- `OverlayGeometry` struct — No changes needed.
- Panel background and separator rendering — These use fixed geometry.

**Testing approach:**

Following docs/trunk/TESTING_PHILOSOPHY.md, the existing unit tests in
`selector_overlay.rs` verify geometry calculations. Since the rendering changes
affect only the Y positioning of quads (which requires a GPU to observe), we
rely on:

1. Visual verification that trackpad scrolling produces smooth motion.
2. The existing `selector.rs` tests that verify `scroll_fraction_px()` and
   `visible_item_range()` work correctly.
3. The `selector_hittest_tests` chunk (next in the narrative) will add
   property-based tests to ensure clicks land on the correct rows.

## Subsystem Considerations

No subsystems are relevant to this chunk. The changes are localized to the
selector overlay rendering code and don't touch any cross-cutting patterns.

## Sequence

### Step 1: Update item list rendering to use fractional scroll offset

Modify the "Phase 6: Item Text" section of `update_from_widget()` to:

1. Read `scroll_fraction_px()` from the widget at the start of the method.
2. Compute `list_y = geometry.list_origin_y - frac` as the base Y for item
   rendering.
3. Replace the current item iteration:
   ```rust
   for (i, item) in items
       .iter()
       .skip(widget.first_visible_item())
       .take(geometry.visible_items)
       .enumerate()
   ```
   with iteration over `widget.visible_item_range()`:
   ```rust
   let range = widget.visible_item_range();
   for (draw_idx, item) in widget.items()[range.clone()].iter().enumerate()
   ```
4. Compute each item's Y using `list_y + draw_idx as f32 * geometry.item_height`
   instead of `list_origin_y + i as f32 * geometry.item_height`.

Location: `crates/editor/src/selector_overlay.rs`, lines 468-506

### Step 2: Update selection highlight to use fractional scroll offset

Modify the "Phase 2: Selection Highlight" section to:

1. Use the same `list_y` computed in Step 1 (will need to move `frac`
   computation earlier in the method).
2. Compute the visible row as `selected - first_visible`, where `first_visible =
   widget.first_visible_item()`.
3. Compute the highlight Y using `list_y + visible_row as f32 *
   geometry.item_height`.
4. Update the visibility check to use `visible_item_range()` bounds rather than
   the old `view_offset + visible_items` formula.

Location: `crates/editor/src/selector_overlay.rs`, lines 373-396

### Step 3: Clean up capacity estimation

Update the capacity estimation near the top of `update_from_widget()` to use
`visible_item_range().len()` instead of `geometry.visible_items`, since the
range may include one extra item for partial bottom visibility.

Location: `crates/editor/src/selector_overlay.rs`, lines 328-336

### Step 4: Add chunk backreference comment

Add a backreference comment at the start of the modified sections:
```rust
// Chunk: docs/chunks/selector_smooth_render - Fractional scroll offset for smooth list scrolling
```

### Step 5: Run existing tests to verify no regressions

Run `cargo test -p editor selector_overlay` to ensure all existing geometry
tests pass. The tests don't exercise fractional scroll (that requires GPU
rendering), but they verify the geometry calculations are unchanged.

## Dependencies

- **selector_row_scroller** (ACTIVE) — This chunk depends on the `RowScroller`
  integration in `SelectorWidget`, which provides `first_visible_item()`,
  `scroll_fraction_px()`, and `visible_item_range()` methods.

## Risks and Open Questions

1. **Off-by-one in visible range** — The `visible_item_range()` method already
   adds +1 for partial bottom visibility, but we should verify that this doesn't
   cause us to attempt drawing items beyond the list bounds. The range is clamped
   by `RowScroller::visible_range(item_count)`, so this should be safe.

2. **Selection highlight clipping** — When the selected item is partially
   scrolled off the top, the highlight quad will extend above `list_origin_y`.
   This is intentional (mirrors main buffer behavior), but we should verify the
   Metal render pipeline clips correctly to the overlay panel bounds.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->