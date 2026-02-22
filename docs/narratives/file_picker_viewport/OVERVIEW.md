---
status: DRAFTING
advances_trunk_goal: null
proposed_chunks:
  - prompt: >
      Fix the coordinate-flip bug in the selector mouse handler. Mouse events
      arrive from macOS with y=0 at the bottom of the screen. The buffer's
      hit-testing correctly flips via `flipped_y = view_height - y` before
      computing line positions. The selector's `handle_mouse_selector` passes
      the raw (un-flipped) y coordinate directly to `SelectorWidget::handle_mouse`,
      while `list_origin_y` from `calculate_overlay_geometry` is a top-relative
      offset. This mismatch causes clicks to land on wrong rows. Fix: flip the y
      coordinate before forwarding to the selector widget, matching the convention
      already established in `buffer_target.rs`.
    chunk_directory: selector_coord_flip
    depends_on: []

  - prompt: >
      Extract a `RowScroller` struct from `Viewport` containing the three shared
      fields (`scroll_offset_px: f32`, `visible_rows: usize`, `row_height: f32`)
      and all thirteen methods that are pure uniform-row scroll arithmetic:
      `first_visible_row()`, `scroll_fraction_px()`, `scroll_offset_px()`,
      `set_scroll_offset_px(px, row_count)`, `update_size(height_px)`,
      `visible_range(row_count)`, `scroll_to(row, row_count)`,
      `ensure_visible(row, row_count)`, `row_to_visible_offset(row)`, and
      `visible_offset_to_row(offset)`. Refactor `Viewport` to contain a
      `RowScroller` and delegate to it; keep `dirty_lines_to_region` and
      `ensure_visible_wrapped` as `Viewport`-only methods since they depend on
      buffer crate types (`DirtyLines`/`DirtyRegion`) and `WrapLayout`
      respectively. All existing `Viewport` tests must pass unchanged after the
      refactor — this chunk changes structure only, not behavior.
    chunk_directory: row_scroller_extract
    depends_on: []

  - prompt: >
      Wire `RowScroller` into `SelectorWidget` to replace its broken integer
      scroll model. Remove the `view_offset: usize` and `visible_items: usize`
      fields and add a `scroll: RowScroller` field instead. Update `handle_scroll`
      to call `scroll.set_scroll_offset_px(current + delta, item_count)`,
      accumulating raw pixel deltas without rounding. Update arrow-key navigation
      in `handle_key` to call `scroll.ensure_visible(selected_index, item_count)`
      after moving the selection. Update `set_items` to re-clamp the scroll
      position via `scroll.set_scroll_offset_px(scroll.scroll_offset_px(),
      new_item_count)`. Expose `scroll.first_visible_row()` and
      `scroll.scroll_fraction_px()` from `SelectorWidget` for use by the
      renderer and hit-tester. Update `SelectorWidget::handle_mouse` to compute
      the clicked item as `scroll.first_visible_row() + floor((relative_y +
      scroll_fraction_px) / item_height)`, accounting for the fractional offset.
    chunk_directory: selector_row_scroller
    depends_on: [0, 1]

  - prompt: >
      Update `SelectorGlyphBuffer::update_from_widget` to consume
      `scroll_fraction_px()` from the `RowScroller` exposed by `SelectorWidget`.
      Currently items are placed at `list_origin_y + visible_row * item_height`
      with no sub-row offset, so scrolling jumps by full rows. After this chunk,
      the starting y for the item list should be `list_origin_y -
      scroll_fraction_px()`, mirroring how the main renderer subtracts
      `viewport.scroll_fraction_px()` to produce smooth glide between positions.
      The `visible_range` passed to the item loop should come from
      `RowScroller::visible_range(item_count)` so the partially-visible top and
      bottom rows are always included.
    chunk_directory: selector_smooth_render
    depends_on: [2]

  - prompt: >
      Harden the hit-test math in `SelectorWidget::handle_mouse` with
      property-based tests. For any combination of `scroll_offset_px`,
      `item_height`, `list_origin_y`, and `item_count`, assert: clicking the
      pixel center of a rendered row (as placed by the updated
      `SelectorGlyphBuffer`) selects exactly that row — never the one above or
      below. Also add a regression test for the original coordinate-flip bug:
      given `view_height`, a raw macOS y coordinate near the top of the list
      (small raw y, large flipped y) should select the topmost item, not an
      out-of-bounds index.
    chunk_directory: selector_hittest_tests
    depends_on: [2, 3]
created_after: ["editor_ux_refinements"]
---

## Advances Trunk Goal

This narrative advances the editor's core interaction quality, the same goal
that drives investments in the main buffer's viewport and wrap-layout
infrastructure. A consistent, correct interaction model across all scrollable
surfaces (buffer *and* overlays) is a required property of the editor.

## Driving Ambition

The file picker is broken in two distinct ways that both stem from the same root
cause: it rolls its own scroll and hit-test math instead of sharing the viewport
infrastructure the main buffer already relies on.

**Bug 1 — wrong row selected on click.** Mouse events arrive from macOS with
`y = 0` at the *bottom* of the screen. Every hit-test in `buffer_target.rs`
correctly flips the coordinate first (`flipped_y = view_height - y`) before
dividing by `line_height`. But `handle_mouse_selector` forwards the raw,
un-flipped y to `SelectorWidget::handle_mouse`, while `list_origin_y` from
`calculate_overlay_geometry` is a *top-relative* offset. The sign mismatch means
a click at the top of the list lands on a row far above the intended target.

**Bug 2 — choppy, non-smooth scrolling.** The main `Viewport` accumulates a
`scroll_offset_px: f32` with no rounding, so trackpad micro-deltas accumulate
faithfully and content glides sub-pixel-smoothly between positions. The
selector's `handle_scroll` rounds `(delta_y / item_height).round()` on every
event, discarding the fractional remainder. Each scroll event snaps to a
whole-row boundary, producing the stepped, non-inertial feel that the main
buffer does not have.

**Why these belong together.** Both bugs come from the selector owning its own
ad-hoc scroll state (`view_offset: usize`) rather than sharing the fractional
pixel model already proven in `Viewport`. The fix is to extract the shared core
of `Viewport` into a `RowScroller` struct and have the selector use it directly.
This is not just a bugfix — it's eliminating the bifurcated scroll abstraction
that made both bugs possible in the first place.

**What `Viewport` and the selector's scroll model actually share.** A precise
method-by-method comparison shows that thirteen of `Viewport`'s methods are pure
uniform-row scroll arithmetic with identical formulas on both sides. Only two
methods are buffer-specific: `dirty_lines_to_region` (depends on the buffer
crate's `DirtyLines`/`DirtyRegion` types) and `ensure_visible_wrapped` (depends
on `WrapLayout` for wrapped text). Everything else — fractional pixel
accumulation, clamping, `first_visible_row()`, `scroll_fraction_px()`,
`visible_range()`, `ensure_visible()`, row↔offset mapping — is identical math
that can live in a single `RowScroller` struct serving both sites.

## Chunks

1. **Fix y-coordinate flip in selector mouse handler** — Surgical bug fix in
   `handle_mouse_selector`: flip the raw y before forwarding to
   `SelectorWidget::handle_mouse`. Independent of the scroll refactor; can land
   immediately to eliminate the "clicks land far above the target" symptom.

2. **Extract `RowScroller` from `Viewport`** — Pure structural refactor.
   Factor the three shared fields and thirteen shared methods out of `Viewport`
   into a new `RowScroller` struct. `Viewport` becomes a thin wrapper that
   delegates to `RowScroller` and adds only `dirty_lines_to_region` and
   `ensure_visible_wrapped`. No behavior changes; all existing tests pass
   unchanged. Also independent of chunk 1 — the two can be implemented in
   parallel.

3. **Wire `RowScroller` into `SelectorWidget`** — Replace `SelectorWidget`'s
   `view_offset: usize` / `visible_items: usize` fields with a `RowScroller`.
   Update `handle_scroll` to accumulate raw pixel deltas, arrow-key navigation
   to call `ensure_visible`, `set_items` to re-clamp via the scroller, and
   `handle_mouse` hit-testing to account for `scroll_fraction_px()`. Depends on
   both 1 (coordinate model is now clean) and 2 (RowScroller exists).

4. **Wire fractional scroll into selector rendering** — Update
   `SelectorGlyphBuffer` to subtract `scroll_fraction_px()` from the item list
   y-origin, and use `RowScroller::visible_range()` for the item loop.
   Gives the same smooth-glide appearance as the main buffer. Depends on 3.

5. **Harden hit-test math with property-based tests** — Assert that clicking
   the pixel center of any rendered row at any scroll position selects exactly
   that row; add a regression test for the original coordinate-flip bug. Depends
   on 3 and 4 (both the model and the rendering must be correct first).

## Completion Criteria

When all chunks are complete:

- Clicking any row in the file picker selects that row — not a row above or
  below it — at all scroll positions.
- Trackpad scrolling in the file picker is as smooth as in the main buffer: no
  visible row-snapping or stepping.
- `RowScroller` is the single implementation of uniform-row fractional scroll
  arithmetic, used by both `Viewport` and the selector, with no duplication.
- All existing `Viewport` and selector tests pass; new tests cover the
  coordinate flip and fractional hit-test scenarios explicitly.
