# Implementation Plan

## Approach

Clip the file picker's item list and selection highlight using Metal's scissor
rect API. When `scroll_fraction_px > 0`, items drawn at `list_origin_y -
scroll_fraction_px` bleed above `list_origin_y` into the query/separator region.
Similarly, the extra row rendered for partial bottom visibility bleeds below
`panel_y + panel_height`. The scissor rect constrains fragment output to the
list region.

**Strategy:**

1. Before drawing the selection highlight and item text in `draw_selector_overlay`,
   set a scissor rect covering only the list region: from `list_origin_y` down to
   `panel_y + panel_height`.

2. Draw the selection highlight and item text (phases 2 and 6 in current code).

3. Reset the scissor rect to the full viewport so subsequent rendering (main
   buffer, tab bar, etc.) is unaffected.

**Metal API used:**

- `MTLRenderCommandEncoder::setScissorRect(MTLScissorRect)` — sets the clipping
  rectangle in pixel coordinates (origin at top-left, Y increases downward,
  matching our coordinate system).

- `MTLScissorRect { x, y, width, height }` — all fields are `NSUInteger` (usize).

**No changes to:**

- `SelectorGlyphBuffer` — it already positions items correctly with fractional
  offsets; only the renderer clips output.
- `OverlayGeometry` or `calculate_overlay_geometry` — existing geometry is
  sufficient; the new scissor rect values derive from `geometry.list_origin_y`,
  `geometry.panel_y`, and `geometry.panel_height`.
- The scroll model (`RowScroller`) — remains unchanged.

**Testing:**

Per the project's Humble View Architecture, scissor rect application is a
renderer-side concern that cannot be meaningfully unit-tested without a GPU.
Visual verification will confirm correctness. Existing geometry tests in
`selector_overlay.rs` and `SelectorGlyphBuffer` tests remain valid.

## Sequence

### Step 1: Import MTLScissorRect in renderer.rs

Add `MTLScissorRect` to the `objc2_metal` imports at the top of `renderer.rs`:

```rust
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLDrawable,
    MTLIndexType, MTLLoadAction, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLScissorRect, MTLStoreAction,
};
```

Location: `crates/editor/src/renderer.rs` (imports section)

### Step 2: Create helper function for scissor rect from geometry

Add a helper function to convert floating-point geometry values to a
`MTLScissorRect`. Metal requires integer pixel coordinates and the rect must
be clamped to the viewport bounds to avoid validation errors.

```rust
/// Creates a scissor rect for clipping the selector item list.
///
/// The rect spans from `list_origin_y` to `panel_y + panel_height`,
/// clipped to the viewport bounds.
fn selector_list_scissor_rect(
    geometry: &OverlayGeometry,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    // Y coordinate: list_origin_y (top of list region)
    let y = (geometry.list_origin_y as usize).min(view_height as usize);

    // Height: from list_origin_y to panel bottom
    let bottom = geometry.panel_y + geometry.panel_height;
    let height = ((bottom - geometry.list_origin_y).max(0.0) as usize)
        .min((view_height as usize).saturating_sub(y));

    MTLScissorRect {
        x: 0,
        y,
        width: view_width as usize,
        height,
    }
}
```

Location: `crates/editor/src/renderer.rs` (helper functions section, near the
Renderer impl)

### Step 3: Create helper function for full viewport scissor rect

Add a second helper function to restore the scissor rect to the full viewport.

```rust
/// Creates a scissor rect covering the entire viewport.
fn full_viewport_scissor_rect(view_width: f32, view_height: f32) -> MTLScissorRect {
    MTLScissorRect {
        x: 0,
        y: 0,
        width: view_width as usize,
        height: view_height as usize,
    }
}
```

Location: `crates/editor/src/renderer.rs` (same section as Step 2)

### Step 4: Modify draw_selector_overlay to apply scissor rect

Update `draw_selector_overlay` to bracket the selection highlight and item text
draw calls with scissor rect changes:

1. After drawing the separator (phase 3) and query text/cursor (phases 4-5), but
   before drawing the selection highlight (currently phase 2 in render order —
   note: we need to reorder phases or apply scissor before phase 2 and maintain
   it through phase 6).

   Actually, looking at the current code structure, the selection highlight is
   drawn as phase 2 (after background), and item text is phase 6 (after cursor).
   Both need to be clipped. The scissor rect should be set before drawing the
   selection highlight and reset after drawing item text.

   **Revised approach:** Set scissor rect before the selection highlight draw,
   keep it through item text draw, then reset it after item text.

2. Before the "Draw Selection Highlight" section, add:

```rust
// Chunk: docs/chunks/selector_list_clipping - Clip item list to panel bounds
// Apply scissor rect to clip selection highlight and items to the list region.
// This prevents fractionally-scrolled items from bleeding into query/separator.
let list_scissor = selector_list_scissor_rect(&geometry, view_width, view_height);
encoder.setScissorRect(list_scissor);
```

3. After the "Draw Item Text" section, add:

```rust
// Chunk: docs/chunks/selector_list_clipping - Reset scissor for subsequent rendering
// Restore full viewport scissor so other render passes (if any) are not clipped.
let full_scissor = full_viewport_scissor_rect(view_width, view_height);
encoder.setScissorRect(full_scissor);
```

**Note on draw order:** The current draw order is:
1. Background (should NOT be clipped — covers full panel)
2. Selection highlight (SHOULD be clipped)
3. Separator (should NOT be clipped — above list region)
4. Query text (should NOT be clipped)
5. Query cursor (should NOT be clipped)
6. Item text (SHOULD be clipped)

This means we need to reorder the draws or apply scissor selectively. The
simplest approach is:

**Option A:** Move selection highlight draw to be immediately before item text
draw, then bracket both with the scissor rect.

**Option B:** Apply/reset scissor twice: once around selection highlight, once
around item text.

**Chosen approach:** Option A is cleaner. Move the selection highlight draw
(phase 2) to after the query cursor draw, immediately before item text. Then
apply scissor before both and reset after.

Location: `crates/editor/src/renderer.rs#draw_selector_overlay`

### Step 5: Reorder draw phases in draw_selector_overlay

Move the selection highlight draw to after the query cursor draw. The new order
will be:

1. Background (unclipped)
2. Separator (unclipped)
3. Query text (unclipped)
4. Query cursor (unclipped)
5. **[Apply scissor rect]**
6. Selection highlight (clipped)
7. Item text (clipped)
8. **[Reset scissor rect]**

This minimizes scissor state changes (1 set, 1 reset) and groups all clipped
draws together.

**Implementation:**

- Move the "Draw Selection Highlight" block from its current position (after
  background) to after "Draw Query Cursor".
- Insert scissor rect set before selection highlight.
- Insert scissor rect reset after item text.

Location: `crates/editor/src/renderer.rs#draw_selector_overlay`

### Step 6: Visual verification

Build and run the editor. Open the file picker, scroll to a fractional position,
and verify:

- No item text or selection highlight appears above `list_origin_y` (over the
  separator or query row).
- No item text or selection highlight appears below the panel's bottom edge.
- The background, separator, and query text are unaffected.
- Other UI elements (main buffer, tab bar, left rail) render correctly.

This is a manual verification step per the project's Humble View Architecture.

### Step 7: Run existing tests

Run all existing tests to ensure no regressions:

```bash
cargo test -p editor
```

All tests should pass — this change is renderer-only and does not affect the
geometry calculations or selector behavior tested in existing unit tests.

## Dependencies

- **selector_smooth_render** (ACTIVE): Provides the fractional scroll offset
  that causes items to bleed past panel boundaries. This chunk fixes the
  resulting visual artifact.

- **selector_row_scroller** (ACTIVE): Provides `scroll_fraction_px()` and
  `visible_item_range()` on `SelectorWidget`.

Both dependencies are satisfied (status: ACTIVE).

## Risks and Open Questions

1. **Scissor rect coordinate system:** Metal uses top-left origin with Y
   increasing downward, matching our screen coordinate system. Verified by
   reading Metal documentation and existing codebase patterns.

2. **Integer rounding:** `MTLScissorRect` fields are `NSUInteger`. Truncation
   from `f32` may cause ±1 pixel error at boundaries. This is acceptable —
   single-pixel errors are not visually noticeable and do not affect
   correctness.

3. **Scissor rect validation:** Metal requires the scissor rect to be within
   the render target bounds. The helper function clamps values to viewport
   dimensions to prevent validation errors.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
