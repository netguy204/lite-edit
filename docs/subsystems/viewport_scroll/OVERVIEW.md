---
status: DOCUMENTED
code_references:
  - ref: crates/editor/src/row_scroller.rs#RowScroller
    implements: "Core scroll arithmetic for uniform-height row lists"
    compliance: COMPLIANT
  - ref: crates/editor/src/viewport.rs#Viewport
    implements: "Buffer-aware scroll mapping with dirty region conversion and wrap support"
    compliance: COMPLIANT
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout
    implements: "Stateless O(1) coordinate mapping between buffer columns and screen positions"
    compliance: COMPLIANT
  - ref: crates/editor/src/dirty_region.rs#DirtyRegion
    implements: "Screen-space dirty region enum with merge semantics"
    compliance: COMPLIANT
  - ref: crates/editor/src/viewport.rs#Viewport::ensure_visible_wrapped
    implements: "Wrap-aware cursor-following scroll with its own clamping"
    compliance: COMPLIANT
  - ref: crates/editor/src/viewport.rs#Viewport::set_scroll_offset_px_wrapped
    implements: "Wrap-aware scroll clamping based on total screen rows"
    compliance: COMPLIANT
  - ref: crates/editor/src/viewport.rs#Viewport::buffer_line_for_screen_row
    implements: "Inverse mapping from screen row to buffer line in wrapped mode"
    compliance: COMPLIANT
  - ref: crates/editor/src/viewport.rs#Viewport::is_at_bottom
    implements: "Bottom-detection for auto-follow behavior (terminal scrollback)"
    compliance: COMPLIANT
  - ref: crates/editor/src/viewport.rs#Viewport::scroll_to_bottom
    implements: "Snap-to-bottom for keypress and mode transition reset (terminal scrollback)"
    compliance: COMPLIANT
created_after: []
---

# viewport_scroll

## Intent

This subsystem provides the coordinate mapping layer between buffer space (logical lines and columns in the text document) and screen space (pixel positions in the rendered viewport). It answers two fundamental questions every frame:

1. **Which content is visible?** — Given a pixel scroll offset, which buffer lines (or screen rows, when wrapping) fall within the viewport?
2. **Where does content appear on screen?** — Given a buffer position (line, column), what pixel coordinate should the renderer draw at?

Without this subsystem, every rendering and input-handling component would need to independently compute scroll bounds, handle fractional pixel offsets, account for soft line wrapping, and track which screen regions need redrawing. The subsystem centralizes these concerns so that consumers (renderer, hit-testing, cursor following) work with clean abstractions.

## Scope

### In Scope

- **Scroll state management**: Tracking `scroll_offset_px` as the single authoritative scroll position, with derived values (`first_visible_row`, `scroll_fraction_px`, `visible_range`) computed on demand.
- **Clamping**: Ensuring scroll position stays within valid bounds for both unwrapped (buffer line count) and wrapped (total screen row count) modes.
- **Buffer ↔ screen coordinate mapping**: Converting between buffer line indices and screen line offsets, including the fractional pixel remainder for smooth sub-row scrolling.
- **Soft line wrapping arithmetic**: O(1) `divmod`-based mapping between buffer columns and wrapped screen positions via `WrapLayout`. No caches, no data structures — pure stateless arithmetic.
- **Dirty region conversion**: Translating buffer-space `DirtyLines` to screen-space `DirtyRegion`, accounting for scroll offset and viewport bounds.
- **Cursor-following scroll**: `ensure_visible` / `ensure_visible_wrapped` to keep the cursor in view, with configurable bottom margin for overlays (e.g., find strip).
- **Reusable scroll primitive**: `RowScroller` as a standalone scroll engine used by both `Viewport` (for text buffers) and `SelectorWidget` (for the file picker / command palette).

### Out of Scope

- **Rendering**: The subsystem computes *where* to draw, not *how*. Glyph vertex buffers, Metal shaders, and atlas management belong to the GPU rendering subsystem.
- **Input handling**: Translating scroll wheel deltas or mouse clicks into scroll offsets happens in `buffer_target.rs` and `editor_state.rs` — this subsystem only provides the `set_scroll_offset_px` API they call.
- **Buffer content**: The subsystem has no knowledge of text content, gap buffers, or editing operations. It receives line counts and character counts as parameters.
- **Terminal scrollback storage**: `TerminalBuffer` in `crates/terminal/` manages its own scrollback storage (hot/cold ring buffers, file-backed cold storage). The subsystem does not own or know about that storage. However, `Viewport` *is* used as the view layer over terminal scrollback — `Viewport::is_at_bottom`, `scroll_to_bottom`, and `set_scroll_offset_px` provide scroll position management while `TerminalBuffer::line_count()` supplies the content bounds. The subsystem owns the scroll arithmetic; the terminal owns the content.

## Invariants

### Hard Invariants

1. **`scroll_offset_px` is the single source of truth.** All other scroll-related values (`first_visible_row`, `scroll_fraction_px`, `visible_range`) are derived from it. There is no separate integer scroll state that can drift.

2. **Scroll offset is always clamped to `[0.0, max_offset_px]`.** The `set_scroll_offset_px` method enforces `max_offset_px = (row_count - visible_rows) * row_height`. When content is shorter than the viewport, max is 0 (no scrolling possible). The wrapped variant computes max from total screen rows instead.

3. **`WrapLayout` is stateless and O(1).** All coordinate mappings are pure `divmod` arithmetic on `cols_per_row`. There is no per-line cache or data structure to invalidate. `WrapLayout` is cheap to reconstruct whenever viewport width or font metrics change.

4. **`DirtyRegion::merge` is associative and commutative with `None` as identity.** Multiple dirty events per frame can be accumulated in any order and produce the correct minimal covering region. `FullViewport` absorbs everything.

5. **`visible_range` includes a +1 row for partial visibility.** When scrolled to a fractional position, the bottom row is partially clipped. The range always includes this extra row so renderers draw enough content.

6. **`ensure_visible` snaps to whole-row boundaries.** After cursor-following scroll, `scroll_fraction_px` is 0. This prevents fractional creep from accumulated cursor movements.

7. **Resize re-clamps scroll offset.** `update_size` recomputes `visible_rows` and calls `set_scroll_offset_px` with the current offset, ensuring the scroll position is valid for the new viewport dimensions. Without this, first_visible_row can exceed the valid maximum, causing click/cursor misalignment.

### Soft Conventions

1. **Prefer `set_scroll_offset_px_wrapped` over `set_scroll_offset_px` when wrapping is enabled.** The unwrapped variant clamps based on buffer line count, which underestimates the scrollable range when lines wrap to multiple screen rows. The wrapped variant computes total screen rows for correct bounds. (The `ensure_visible_wrapped` method uses `set_scroll_offset_unclamped` internally because it does its own bounds computation.)

2. **Consumers should use `Viewport` for buffer editing and `RowScroller` directly for non-buffer scrollable lists.** `Viewport` adds buffer-specific methods (`dirty_lines_to_region`, `ensure_visible_wrapped`) that don't apply to generic lists. `SelectorWidget` correctly uses `RowScroller` directly via `viewport.row_scroller()`.

## Implementation Locations

### RowScroller (`crates/editor/src/row_scroller.rs`)

The foundational scroll primitive. It is a pure data structure with no platform dependencies: just `scroll_offset_px: f32`, `visible_rows: usize`, and `row_height: f32`. All methods are deterministic functions of these three fields.

Key design choice: scroll position is tracked in **floating-point pixels**, not integer rows. This enables smooth trackpad scrolling where sub-row deltas accumulate naturally. The integer row index is always derived via `floor(offset / height)`.

`RowScroller` was extracted from `Viewport` (chunk: `row_scroller_extract`) when `SelectorWidget` needed the same scroll arithmetic without the buffer-specific methods. This is the canonical pattern for adding new scrollable UI elements.

### Viewport (`crates/editor/src/viewport.rs`)

A thin wrapper around `RowScroller` that adds buffer-editing concerns:

- **`dirty_lines_to_region`**: Converts buffer `DirtyLines` to screen `DirtyRegion` by intersecting dirty ranges with the visible viewport. This is what allows incremental redraw — only touched screen lines are rebuilt.
- **`ensure_visible_wrapped`**: The most complex method. It computes the cursor's absolute screen row by iterating over buffer lines, then adjusts scroll offset to keep that row visible. It uses `set_scroll_offset_unclamped` because it computes its own max from the wrapped total.
- **`buffer_line_for_screen_row`**: The inverse mapping for wrapped mode — given a screen row, find which buffer line contains it and the row offset within that line. This is a static method (no `&self`) since it's used during rendering before a `Viewport` position is established.
- **`is_at_bottom`**: Returns whether the viewport is scrolled to the bottom (within 1px tolerance). Used by terminal tabs for auto-follow behavior — when at bottom, new PTY output advances the viewport automatically.
- **`scroll_to_bottom`**: Snaps the viewport to the maximum scroll offset. Used for keypress snap-to-bottom in terminal tabs and for mode transition resets (alternate → primary screen).

### WrapLayout (`crates/editor/src/wrap_layout.rs`)

A `Copy` struct with three fields: `cols_per_row`, `glyph_width`, `line_height`. Every method is `#[inline]` and branch-free (except the empty-line check in `screen_rows_for_line`).

The critical insight documented in this module: with a monospace font, all wrapping coordinates reduce to integer division and modulo. There is no need for a line-width cache because `cols_per_row` is constant across the viewport. This makes `WrapLayout` trivially correct and impossible to have stale.

### DirtyRegion (`crates/editor/src/dirty_region.rs`)

An enum with three variants: `None`, `Lines { from, to }`, `FullViewport`. The `merge` method implements a simple lattice: `None < Lines < FullViewport`. Two `Lines` ranges merge to their bounding range (which may overestimate slightly for disjoint ranges, but the renderer handles this cheaply).

`DirtyRegion` is the screen-space counterpart to the buffer crate's `DirtyLines`. The `Viewport::dirty_lines_to_region` method bridges them.

## Known Deviations

No known deviations. All four components follow the subsystem's patterns consistently. The separation between `RowScroller` (generic) and `Viewport` (buffer-specific) is clean, and `WrapLayout` is used uniformly wherever wrapping coordinates are needed.
