<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds soft (visual) line wrapping to the editor. The core insight from the GOAL.md is that the fixed-width font makes all coordinate mapping pure O(1) integer arithmetic:

```
cols_per_row   = floor(viewport_width_px / glyph_width_px)
screen_rows(line)   = ceil(line.char_count / cols_per_row)     // O(1)
screen_pos(buf_col) = divmod(buf_col, cols_per_row)            // O(1) → (row_offset, col)
buffer_col(row_off, col) = row_off * cols_per_row + col        // O(1)
```

The implementation introduces a `WrapLayout` abstraction that encapsulates these calculations, becoming the single source of truth for all logical-line ↔ visual-line coordinate mapping. This struct is stateless and computes all mappings on the fly from the current viewport width and glyph metrics.

**Key architectural decisions:**

1. **No global wrap index/cache** — The viewport bound ensures we only ever process ~30-80 visible lines. Per the GOAL.md, a document-wide cumulative wrap index would add complexity with no benefit.

2. **All buffer operations stay in logical-line coordinates** — Only the renderer and hit-tester translate between logical lines (buffer) and visual lines (screen rows). This preserves the invariant that keeps operations O(1).

3. **Continuation row indicator via left-edge border** — A 2px solid black border on the leftmost edge of continuation rows, with no layout impact. The border sits inside the existing content area.

4. **Rendering loop restructure** — Instead of iterating `visible_range()` and emitting one screen row per buffer line, the loop iterates buffer lines and emits `screen_rows(line)` screen rows per buffer line, stopping when the accumulated screen row count fills the viewport.

**Testing approach:**

Per TESTING_PHILOSOPHY.md, we focus on semantic assertions tied to success criteria:
- Unit tests for `WrapLayout` coordinate mapping (O(1) verified by construction)
- Unit tests for continuation row detection (which screen rows get the border)
- Integration tests for cursor placement across wrap boundaries
- Integration tests for selection rendering across wrap boundaries
- Hit-testing tests for clicks on continuation rows

The renderer itself (Metal draw calls) is a "humble view" and not unit-tested directly.

## Subsystem Considerations

No documented subsystems exist yet for this project. The work here may seed a future "viewport_rendering" subsystem if similar patterns emerge in other chunks.

## Sequence

### Step 1: Introduce WrapLayout struct

Create a new `WrapLayout` struct in `crates/editor/src/glyph_buffer.rs` (or a new `wrap_layout.rs` module) that encapsulates the wrap arithmetic:

```rust
pub struct WrapLayout {
    /// Number of character columns that fit in the viewport
    cols_per_row: usize,
    /// Glyph width in pixels (from FontMetrics)
    glyph_width: f32,
    /// Line height in pixels
    line_height: f32,
}

impl WrapLayout {
    pub fn new(viewport_width_px: f32, metrics: &FontMetrics) -> Self;

    /// Returns the number of visual screen rows needed to display a line with `char_count` characters
    pub fn screen_rows_for_line(&self, char_count: usize) -> usize;

    /// Converts a buffer column to (row_offset_within_line, screen_col)
    pub fn buffer_col_to_screen_pos(&self, buf_col: usize) -> (usize, usize);

    /// Converts (row_offset_within_line, screen_col) back to buffer column
    pub fn screen_pos_to_buffer_col(&self, row_offset: usize, screen_col: usize) -> usize;

    /// Returns true if this is a continuation row (row_offset > 0)
    pub fn is_continuation_row(&self, row_offset: usize) -> bool;
}
```

Write comprehensive unit tests for all arithmetic:
- `screen_rows_for_line` with various character counts
- `buffer_col_to_screen_pos` at boundaries (0, cols_per_row-1, cols_per_row, etc.)
- `screen_pos_to_buffer_col` round-trips correctly

Location: `crates/editor/src/wrap_layout.rs` (new file)

### Step 2: Extend GlyphLayout with wrap-aware positioning

Update `GlyphLayout` in `crates/editor/src/glyph_buffer.rs` to accept a `WrapLayout` reference and compute positions for wrapped lines:

```rust
impl GlyphLayout {
    /// Position for a character at buffer_line, buffer_col with wrapping
    pub fn position_for_wrapped(
        &self,
        first_screen_row: usize,  // cumulative screen row where this buffer line starts
        buffer_col: usize,
        wrap_layout: &WrapLayout,
        y_offset: f32,
    ) -> (f32, f32);
}
```

The key change: previously `position_for` took `(row, col)` where `row` was screen row. Now the wrapped variant computes the screen row internally via `buffer_col_to_screen_pos`.

Tests:
- Position at col 0 of a line → x=0
- Position at col = cols_per_row → x=0, y += line_height (wrapped to next screen row)
- Position at col = cols_per_row + 5 → x=5*glyph_width, y += line_height

### Step 3: Update GlyphBuffer::update_from_buffer_with_cursor for wrapping

Refactor the glyph buffer update loop to handle wrapped lines:

1. Accept `WrapLayout` as a parameter (computed from current viewport width)
2. Track cumulative screen row count as we iterate buffer lines
3. For each buffer line:
   - Compute `screen_rows_for_line(line.len())`
   - For each character, compute its screen position using the new wrap-aware method
   - Stop when cumulative screen rows exceed viewport capacity

The loop structure changes from:
```rust
for buffer_line in visible_range {
    let screen_row = buffer_line - first_visible_line;
    // emit one row of quads
}
```

To:
```rust
let mut screen_row = 0;
for buffer_line in first_visible_line.. {
    if screen_row >= max_screen_rows { break; }
    let line_content = buffer.line_content(buffer_line);
    let rows_for_this_line = wrap_layout.screen_rows_for_line(line_content.len());
    // emit quads for each character, computing screen position via wrap_layout
    screen_row += rows_for_this_line;
}
```

Location: `crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor`

### Step 4: Add continuation row border rendering

Extend the glyph buffer to emit solid quads for continuation row borders:

1. Add a new `QuadRange` for border quads (similar to selection/glyph/cursor ranges)
2. When emitting quads for a buffer line, for each screen row where `row_offset > 0`, emit a border quad:
   - x: 0
   - y: screen_row * line_height - y_offset
   - width: 2px (or 1px at 1x scale, 2px at 2x)
   - height: line_height
   - Uses solid glyph from atlas

3. The renderer draws border quads in a new phase with a fixed black color

Location: `crates/editor/src/glyph_buffer.rs` (new `border_range: QuadRange` field)

### Step 5: Update cursor rendering for wrapped positions

The cursor is already rendered via `create_cursor_quad_with_offset`. Update it to:

1. Accept `WrapLayout` to compute the cursor's screen position
2. The cursor's buffer column determines which screen row it lands on
3. Cursor x position = `(buffer_col % cols_per_row) * glyph_width`
4. Cursor y position = screen row computed from buffer line's cumulative offset + row_offset

Test: cursor at column > cols_per_row renders on a continuation row

Location: `crates/editor/src/glyph_buffer.rs#create_cursor_quad_with_offset`

### Step 6: Update selection rendering for wrapped lines

Selection quads currently span `[start_col, end_col)` within a single screen row. With wrapping:

1. A selection within a single buffer line may span multiple screen rows
2. For each screen row within the selection, emit a separate selection quad
3. Selection on continuation rows should not include the border area (start at x=0)

The selection logic changes from "one quad per buffer line in selection" to "one quad per screen row segment in selection".

Location: `crates/editor/src/glyph_buffer.rs` (selection quad generation loop)

### Step 7: Update hit-testing for wrapped lines

Refactor `pixel_to_buffer_position` in `crates/editor/src/buffer_target.rs`:

1. The current logic assumes screen_line == buffer_line - scroll_offset. This breaks with wrapping.
2. New algorithm (from GOAL.md):
   - `click_screen_row = floor((y_px + scroll_fraction_px) / line_height_px)`
   - Walk forward from `first_visible_line`, subtracting `screen_rows(line)` from a counter until the counter would go negative
   - The current line owns the click; `row_offset_within_line = remaining_counter`
   - `buffer_col = row_offset * cols_per_row + floor(x_px / glyph_width_px)`
   - Clamp to line length

3. Accept `WrapLayout` as a parameter

Tests:
- Click on first screen row of a long line → buffer_col computed correctly
- Click on second screen row of a long line → buffer_col = cols_per_row + x_col
- Click on the first screen row of the buffer line AFTER a wrapped line → resolves to that line at col 0 + x offset

Location: `crates/editor/src/buffer_target.rs#pixel_to_buffer_position`

### Step 8: Update Viewport to track screen row counts

The viewport currently tracks scroll position in pixel space and computes `first_visible_line` from it. With wrapping:

1. `visible_range()` must account for wrapped lines consuming multiple screen rows
2. `ensure_visible()` must compute the pixel offset of the cursor's screen row, not just the cursor's buffer line
3. `buffer_line_to_screen_line()` is no longer a simple subtraction — it requires summing `screen_rows` for all lines before the target

However, per GOAL.md guidance: **Do NOT introduce a global wrap index.** These operations remain O(visible_lines) which is bounded constant (~30-80).

Changes:
- Add `WrapLayout` as a parameter to relevant Viewport methods
- `visible_range()` may need to iterate to count how many buffer lines fit

Location: `crates/editor/src/viewport.rs`

### Step 9: Update Renderer to pass WrapLayout

The renderer creates the glyph buffer and holds the viewport. It must:

1. Compute `WrapLayout` from current viewport width and font metrics
2. Pass it to `GlyphBuffer::update_from_buffer_with_cursor`
3. Add a draw pass for border quads (black color)

Location: `crates/editor/src/renderer.rs`

### Step 10: Integration tests

Write integration tests in `crates/editor/tests/` that verify:

1. **Cursor at wrap boundary**: Insert a line longer than viewport width, position cursor at column > cols_per_row, verify it renders on a continuation row
2. **Selection across wrap boundary**: Select text spanning a wrap point, verify selection quads are emitted for both screen rows
3. **Click on continuation row**: Simulate click at (x, y) where y is on a continuation row, verify buffer position resolves correctly
4. **No horizontal scroll**: With wrapping enabled, verify no horizontal scroll offset exists or is reachable
5. **Continuation row visual indicator**: Verify border quads are emitted for continuation rows only

Location: `crates/editor/tests/wrap_test.rs` (new file)

### Step 11: Remove horizontal scroll (if present)

Verify that no horizontal scroll offset exists in the current codebase. If any horizontal scroll logic exists, remove it — wrapping makes it unnecessary.

Location: Search codebase for "horizontal", "scroll_x", etc.

## Dependencies

All implementation dependencies are already satisfied:
- `viewport_fractional_scroll` (created_after) — provides pixel-space scroll position
- `viewport_rendering` (created_after) — provides the viewport abstraction
- `text_buffer` (created_after) — provides buffer content and cursor position

No external libraries needed.

## Risks and Open Questions

1. **Performance with extremely long lines**: A buffer line with 10,000 characters would produce ~100 screen rows (at 100 cols/row). The loop still iterates character-by-character which is O(line_length). This should be acceptable but worth monitoring. If needed, we could chunk the iteration.

2. **Tab character handling**: Tabs expand to variable widths in some editors. The current monospace assumption treats tab as a single character width. This may need revisiting if tab rendering becomes a concern.

3. **Unicode grapheme clusters**: A single "character" may be multiple Unicode code points (e.g., emoji with skin tone modifiers). The current implementation uses `.chars().count()` which counts code points, not graphemes. For now, this matches the buffer's internal model, but could cause visual misalignment for complex Unicode.

4. **Resize handling**: When the viewport width changes, `WrapLayout` changes, which changes where lines wrap. The entire viewport should be marked dirty on resize. Verify this happens.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
