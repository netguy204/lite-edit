---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/wrap_layout.rs
- crates/editor/src/glyph_buffer.rs
- crates/editor/src/buffer_target.rs
- crates/editor/src/viewport.rs
- crates/editor/src/renderer.rs
- crates/editor/src/main.rs
- crates/editor/tests/wrap_test.rs
code_references:
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout
    implements: "Stateless layout calculator for O(1) coordinate mapping between buffer columns and screen positions"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::screen_rows_for_line
    implements: "O(1) calculation of screen rows needed for a buffer line with given character count"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::buffer_col_to_screen_pos
    implements: "O(1) conversion from buffer column to (row_offset, screen_col) for wrapped lines"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::screen_pos_to_buffer_col
    implements: "O(1) inverse conversion from screen position to buffer column"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::is_continuation_row
    implements: "Determines if a screen row needs the continuation indicator (row_offset > 0)"
  - ref: crates/editor/src/wrap_layout.rs#WrapLayout::position_for_wrapped
    implements: "Calculate pixel position for a character with wrapping applied"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_wrap
    implements: "Wrap-aware rendering loop that emits selection, border, glyph, and cursor quads for wrapped lines"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::create_border_quad
    implements: "Creates 2px left-edge border quad for continuation row indicators"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::border_range
    implements: "Returns index range for continuation row border quads"
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped
    implements: "Wrap-aware hit-testing: converts pixel coordinates to buffer position by walking screen rows"
  - ref: crates/editor/src/viewport.rs#Viewport::ensure_visible_wrapped
    implements: "Wrap-aware cursor visibility ensuring the specific screen row containing cursor is visible"
  - ref: crates/editor/src/renderer.rs#Renderer::wrap_layout
    implements: "Factory method creating WrapLayout from current viewport width and font metrics"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_double_click_select
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Lines longer than the viewport width currently overflow horizontally and are not
fully visible without horizontal scrolling. This chunk adds soft (visual) line
wrapping so that every character is always reachable within the viewport without
horizontal navigation.

Wrapping is purely a rendering concern — the underlying buffer stores lines as
they were entered and the cursor model continues to think in buffer columns. The
renderer splits a long buffer line into multiple screen rows on the fly, fitting
as many characters per screen row as the viewport width allows.

To make the boundary between a buffer line and its continuation rows immediately
legible, wrapped continuation rows receive a distinct left-edge treatment: a
solid black border drawn flush against the leftmost pixel of the row, covering
the full row height. No whitespace is introduced — the border sits inside the
existing content area and the first glyph of the continuation row starts
immediately to its right. The effect reads as a subtle indent marker without
consuming any horizontal space or altering the character-column mapping.

## Success Criteria

- Every character on every buffer line is visible without horizontal scrolling.
  A line whose glyph count exceeds `floor(viewport_width / glyph_width)` is
  split into multiple screen rows.
- The split is character-column exact: the first `N` characters occupy screen row
  0, the next `N` occupy screen row 1, and so on, where `N` is the number of
  fixed-width glyphs that fit in the viewport width at the current font size.
- Continuation rows (screen rows 2..k produced by a single buffer line) each
  render a solid black left-edge border one or two pixels wide, running the full
  height of the row, with no gap between the border and the first glyph.
- The first screen row of every buffer line (including lines that do not wrap)
  has no left-edge border, preserving the visual distinction between "this is a
  new line" and "this is a continuation of the previous line."
- Cursor rendering is correct: the cursor appears at the screen row and column
  that correspond to the cursor's buffer column after splitting.
- Selection rendering is correct: highlighted spans cross wrap boundaries
  naturally, colouring the appropriate portion of each screen row.
- The viewport's visible-line count and scroll arithmetic account for the
  expanded screen-row count so that `ensure_visible` and fractional scroll both
  operate correctly after wrapping is introduced.
- Mouse click hit-testing is correct throughout. A click on a continuation
  screen row resolves to the buffer line that owns that row and the buffer column
  derived from the click's X position plus the column offset of that screen row's
  first character. A click on any screen row that follows a wrapped buffer line
  (i.e. the first screen row of the next buffer line) resolves to that next buffer
  line at column 0 plus the X-derived offset — not to the tail of the preceding
  wrapped line.
- Wrapping must not change the time complexity of any buffer or viewport
  operation. Operations that were O(1) before (cursor movement, single-line
  lookup, hit-test for a given screen position) must remain O(1). The
  screen-row count for a single buffer line is derivable in O(1) from its
  character count and the viewport column width, so no full-buffer scan is
  needed to map a buffer position to a screen row or vice versa, and no
  such scan should be introduced.
- No horizontal scroll offset exists or is reachable; the editor is
  viewport-width constrained at all times once this chunk is implemented.

## Implementation Notes

### Logical lines vs visual lines

The established vocabulary for this problem distinguishes *logical lines* (buffer
lines, what the buffer model sees) from *visual lines* (screen rows, what the
renderer draws). The invariant that preserves time complexity is: **all buffer
operations stay in logical-line coordinates**. Only the renderer and hit-tester
ever translate between the two. Nothing in the edit path should touch visual-line
arithmetic.

### Why this editor has it easy: monospace arithmetic

The general version of this problem (tackled at length in Raph Levien's xi-editor
"rope science" series) is hard because variable-width fonts require pixel-level
layout measurement to determine where lines break. xi solves this with a b-tree
storing cached break positions and incremental invalidation — O(log n) operations
throughout.

This editor uses a fixed-width font. That reduces every coordinate mapping to
integer arithmetic:

```
cols_per_row   = floor(viewport_width_px / glyph_width_px)
screen_rows(line)   = ceil(line.char_count / cols_per_row)     // O(1)
screen_pos(buf_col) = divmod(buf_col, cols_per_row)            // O(1) → (row_offset, col)
buffer_col(row_off, col) = row_off * cols_per_row + col        // O(1)
```

No cache, no data structure, no invalidation. Introduce a small `WrapLayout`
struct (or module-level helpers) holding `cols_per_row` that exposes these three
functions. It becomes the single source of truth for all coordinate mapping in
rendering, cursor placement, selection, and hit-testing.

### The rendering loop

The existing loop iterates `viewport.visible_range()` (a range of buffer line
indices) and emits one screen row per buffer line. Change it to: start at
`first_visible_line`, iterate buffer lines, for each emit
`screen_rows(line.char_count)` screen rows, and stop when the accumulated screen
row count fills the viewport. No global index is needed — only the lines
currently being rendered are ever examined.

### Hit-testing (screen → buffer)

Given a click at `(x_px, y_px)`:

1. `click_screen_row = floor((y_px + scroll_fraction_px) / line_height_px)`
2. Walk forward from `first_visible_line`, subtracting `screen_rows(line)` from
   a counter until the counter would go negative. The current line owns the click.
3. `row_offset_within_line = remaining_counter`
4. `buffer_col = buffer_col(row_offset_within_line, floor(x_px / glyph_width_px))`

This is O(visible_lines), which is a fixed constant (~30–80 rows), not O(document
length). No global data structure is required or appropriate here.

### Scroll arithmetic

`scroll_offset_px` is already pixel-based (from the `viewport_fractional_scroll`
chunk). Pixel space is neutral with respect to wrapping — the viewport scrolls
through a continuous run of screen rows, each `line_height_px` tall, regardless
of whether adjacent screen rows belong to the same logical line or different ones.
The only change needed is that `ensure_visible` must compute the pixel offset of
the *first screen row* of the target buffer line, which requires summing
`screen_rows(line)` for all buffer lines before it — but only for lines from
`first_visible_line` to the target, a bounded scan.

### Do not reach for a global wrap index

Any approach that pre-computes or caches a document-wide array of cumulative
screen-row offsets is over-engineered for this case and requires complex
invalidation on every edit. The viewport bound makes it unnecessary: you only
ever need to resolve screen rows within the visible window.

## Rejected Ideas

### Left-padding / indentation on continuation rows

Adding actual whitespace or an indented left margin would shift the column origin
of continuation rows relative to their buffer columns, complicating the
character-to-screen mapping and wasting horizontal space. The black left-border
treatment achieves the same perceptual goal (obvious wrap indicator) with zero
layout impact.

### Incremental wrap cache (xi-editor style)

xi-editor maintains a b-tree of cached line-break positions with incremental
invalidation to handle variable-width fonts at O(log n). This editor's fixed-width
font makes per-line wrap counts pure O(1) arithmetic with no state to maintain,
so an incremental cache would add complexity with no benefit.

---