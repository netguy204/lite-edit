<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The core insight is that tab characters need special treatment everywhere the codebase converts between **character indices** and **visual columns**. Currently, all coordinate mapping assumes 1 character = 1 visual column (except for wide characters via `unicode_width`). Tabs break this assumption because a single `'\t'` character occupies 1-N columns depending on its position relative to tab stops.

The implementation strategy introduces a `TAB_WIDTH` constant and threads **tab-stop-aware column arithmetic** through every layer that performs character↔visual mapping:

1. **Visual width calculation**: Add helper functions to compute the visual width of a character at a given visual column position: regular chars = 1 column (or 2 for wide), tab = columns to next tab stop.

2. **Rendering (glyph emission)**: Modify `GlyphBuffer::update_from_buffer_with_wrap` to skip emitting glyphs for `'\t'` characters but advance the visual column counter to the next tab stop. This is analogous to how spaces are already skipped (no glyph) but advance the column.

3. **Cursor positioning**: The cursor's pixel X coordinate must use the **visual column**, not the character index. This affects both cursor glyph rendering and selection quad positioning.

4. **Mouse hit-testing**: `pixel_to_buffer_position_wrapped` must reverse the visual column back to a character index by walking the line's characters and accumulating visual widths until the target visual column is reached.

5. **Wrap layout**: `WrapLayout::screen_rows_for_line` currently uses character count. For tab-aware wrapping, it needs the line's **visual width** (sum of visual widths of all characters). This requires passing line content, not just character count.

6. **Selection rendering**: Selection start/end columns are character indices. The selection quad rendering must convert these to visual columns.

The key architectural pattern: **visual column is a derived property computed on-the-fly from character content + position**. This avoids caching visual widths (which would require invalidation on edit). The overhead is acceptable because:
- Tab characters are relatively rare in most files
- Visual width calculation is O(n) in line length but only performed for visible lines
- The existing wide-character handling already does per-character width checks

**Testing strategy** (per docs/trunk/TESTING_PHILOSOPHY.md):
- Unit tests for tab-stop arithmetic functions (pure, testable)
- Unit tests for visual-width-to-char-index conversion (pure, testable)
- Integration tests for WrapLayout with tabs (coordinate round-trips)
- No tests for actual rendering (humble view) — verify visually that tabs render as whitespace

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll subsystem. Specifically:
  - `WrapLayout` needs modification to compute visual widths instead of character counts for wrapped line row calculation.
  - `Viewport::buffer_line_for_screen_row` is consumed by hit-testing and rendering; it delegates to `WrapLayout::screen_rows_for_line`, which will need tab-aware visual width.
  - The subsystem invariant "WrapLayout is stateless and O(1)" will be relaxed slightly: tab-aware wrapping requires O(n) line content traversal to compute visual width. This is acceptable because the traversal is only performed for visible lines and is already done for wide character width computation.

- **docs/subsystems/spatial_layout** (DOCUMENTED): This chunk USES the spatial_layout subsystem's hit-testing coordinate chain:
  - `pixel_to_buffer_position_wrapped` converts pane-local pixels to buffer position. It currently uses `WrapLayout::screen_pos_to_buffer_col` which assumes 1 char = 1 column. This must be replaced with a tab-aware reverse mapping.
  - The subsystem invariant "pixel_to_buffer_position uses floor, not round" is preserved — we floor the visual column from pixel X, then map visual column to character index.

## Sequence

### Step 1: Add TAB_WIDTH constant and tab-stop arithmetic helpers

Create a new module `crates/editor/src/tab_width.rs` with:

1. A `TAB_WIDTH` constant set to 4 (compile-time, no configuration).

2. Pure helper functions for tab-stop arithmetic:
   ```rust
   /// Returns the visual width of a character at the given visual column.
   /// Tab characters span from `visual_col` to the next tab stop.
   /// Wide characters (CJK, emoji) return 2. Other characters return 1.
   pub fn char_visual_width(c: char, visual_col: usize) -> usize

   /// Returns the next tab stop column after `visual_col`.
   /// Tab stops are at columns 0, TAB_WIDTH, 2*TAB_WIDTH, ...
   pub fn next_tab_stop(visual_col: usize) -> usize
   ```

3. Unit tests for:
   - `next_tab_stop(0)` → 4, `next_tab_stop(3)` → 4, `next_tab_stop(4)` → 8
   - `char_visual_width('\t', 0)` → 4, `char_visual_width('\t', 2)` → 2, `char_visual_width('\t', 4)` → 4
   - `char_visual_width('a', 0)` → 1 for any position
   - Wide char handling (use `unicode_width::UnicodeWidthChar`)

Location: `crates/editor/src/tab_width.rs` (new file), `crates/editor/src/lib.rs` (add `mod tab_width;`)

---

### Step 2: Add line visual width calculation helpers

Extend `crates/editor/src/tab_width.rs` with functions that operate on line content:

```rust
/// Returns the total visual width of a string, accounting for tabs and wide chars.
pub fn line_visual_width(line: &str) -> usize

/// Converts a buffer column (character index) to a visual column.
/// Returns the visual column where the character at `char_col` begins.
pub fn char_col_to_visual_col(line: &str, char_col: usize) -> usize

/// Converts a visual column to a buffer column (character index).
/// If the visual column is in the middle of a tab, returns the tab's char index.
/// If the visual column is past the line end, returns the line's char count.
pub fn visual_col_to_char_col(line: &str, visual_col: usize) -> usize
```

Unit tests for:
- `line_visual_width("a\tb")` with TAB_WIDTH=4 → 1 + 3 + 1 = 5 (tab at col 1 expands to col 4)
- `line_visual_width("\t")` → 4
- `char_col_to_visual_col("a\tb", 2)` → 4 (char 'b' is at visual column 4)
- `visual_col_to_char_col("a\tb", 4)` → 2 (visual col 4 is char 'b')
- `visual_col_to_char_col("a\tb", 2)` → 1 (visual col 2 is inside the tab, returns tab's char index)
- Round-trip: `visual_col_to_char_col(line, char_col_to_visual_col(line, col)) == col` for non-tab positions

Location: `crates/editor/src/tab_width.rs`

---

### Step 3: Update WrapLayout to accept visual width

Modify `WrapLayout::screen_rows_for_line` to accept **visual width** instead of character count:

1. Rename parameter from `char_count` to `visual_width` (the semantics change — callers will pass visual width).

2. Add a new method `screen_rows_for_line_with_content(line: &str)` that computes visual width internally and calls `screen_rows_for_line`.

3. Update all existing callers of `screen_rows_for_line` to pass visual width. Callers that have access to line content should use `line_visual_width(line)`. Callers that only have character count (e.g., terminal buffers) continue to pass char count (tabs are rare in terminal output).

Affected callers (search for `screen_rows_for_line`):
- `glyph_buffer.rs`: Multiple call sites in `update_from_buffer_with_wrap`
- `viewport.rs`: `buffer_line_for_screen_row`
- `buffer_target.rs`: `pixel_to_buffer_position_wrapped`

Location: `crates/editor/src/wrap_layout.rs`, callers listed above

---

### Step 4: Update glyph rendering to skip tabs and advance visual column

Modify `GlyphBuffer::update_from_buffer_with_wrap` (Phase 3: Glyph Quads) to handle tab characters:

1. Track `visual_col` separately from `col` (character index).

2. For tab characters:
   - Do not emit a glyph quad (like spaces, tabs are just whitespace)
   - Advance `visual_col` to the next tab stop
   - Advance `col` by 1 (the tab character itself)

3. For other characters:
   - Use `visual_col` for positioning (`quad_vertices_with_xy_offset` uses `col` for X position — change to use `visual_col`)
   - Advance `visual_col` by `char_width` (from `unicode_width`)
   - Advance `col` by 1

The key change: the X position for glyph quads comes from `visual_col`, not `col`.

Location: `crates/editor/src/glyph_buffer.rs` (around line 820-860 in the glyph emission loop)

---

### Step 5: Update background and selection quad rendering for tabs

Modify the background and selection quad rendering phases in `GlyphBuffer::update_from_buffer_with_wrap` to use visual columns:

**Background quads (Phase 1)**:
- Currently computes `span_width` from character display widths
- Must convert span character range to visual column range using `char_col_to_visual_col`
- The quad's `screen_start_col` and `screen_end_col` must be visual columns

**Selection quads (Phase 2)**:
- Selection `start_col` and `end_col` are character indices
- Convert to visual columns before emitting quads
- For wrapped lines, the row/column math uses visual columns

Location: `crates/editor/src/glyph_buffer.rs` (Phases 1 and 2)

---

### Step 6: Update cursor rendering for tabs

The cursor quad rendering (Phase 5) must position the cursor at the correct visual column:

1. The cursor position comes from `view.cursor_position()` which returns `(line, col)` where `col` is a character index.

2. To render the cursor:
   - Get the line content at cursor line
   - Convert `cursor.col` to visual column using `char_col_to_visual_col`
   - Use the visual column for the cursor quad's X position

3. For cursor shapes (bar, block, underline):
   - Block cursor width should be the visual width of the character under cursor (1 for regular, `tab_visual_width` for tab, 2 for wide char)
   - Bar cursor is 2px wide at the visual column
   - Underline cursor spans the visual width of the character

Location: `crates/editor/src/glyph_buffer.rs` (Phase 5: Cursor Quad)

---

### Step 7: Update mouse hit-testing for tabs

Modify `pixel_to_buffer_position_wrapped` in `buffer_target.rs`:

1. After computing the screen column from pixel X (`screen_col = x / glyph_width`), this is a **visual column** within the screen row.

2. Convert the visual column to a buffer column:
   - Get the buffer line content
   - Use `visual_col_to_char_col(line_content, visual_col)` to get the character index

Currently the code uses `wrap_layout.screen_pos_to_buffer_col(row_offset, screen_col)` which treats `screen_col` as a character index. This must be changed to:
   - Compute the buffer column from `row_offset * cols_per_row + screen_col` (the visual column within the buffer line)
   - Convert that visual column to a character index using `visual_col_to_char_col`

This requires access to line content in the hit-testing function. The `line_len_fn` closure must be extended to also provide line content, or a separate closure added.

Location: `crates/editor/src/buffer_target.rs` (function `pixel_to_buffer_position_wrapped`)

---

### Step 8: Update underline rendering for tabs

Modify underline quad rendering (Phase 4) to use visual columns:

- Span start/end are character indices
- Convert to visual columns for quad positioning
- Underline width should match the visual width (tabs render as wider underlines)

Location: `crates/editor/src/glyph_buffer.rs` (Phase 4: Underline Quads)

---

### Step 9: Integration test with tab-containing content

Add integration tests that verify the complete pipeline:

1. Create a test `TextBuffer` with tab characters
2. Verify `line_visual_width` matches expected values
3. Verify `WrapLayout::screen_rows_for_line` with tabs produces correct row counts
4. Verify `char_col_to_visual_col` and `visual_col_to_char_col` round-trip correctly

Manual visual verification (not automated):
- Open a file with tab indentation (e.g., a Go file, a Makefile)
- Verify tabs render as whitespace to tab stops, not as glyphs
- Verify cursor positioning on lines with tabs
- Verify mouse clicking on lines with tabs places cursor correctly
- Verify selection highlighting spans correct visual region

Location: `crates/editor/tests/tab_rendering_test.rs` (new file)

---

**BACKREFERENCE COMMENTS**

When implementing, add the following backreference to modified code:

```rust
// Chunk: docs/chunks/tab_rendering - Tab character rendering and tab-aware coordinate mapping
```

Place at:
- Module level in `tab_width.rs`
- Method level for modified functions in `glyph_buffer.rs`, `wrap_layout.rs`, `buffer_target.rs`

## Dependencies

- **unicode-width crate**: Already a dependency (used for wide character handling). No new dependencies needed.
- **No chunk dependencies**: This chunk is independent. The `treesitter_indent` chunk (mentioned in the investigation) depends on this chunk, not vice versa.

## Risks and Open Questions

1. **Performance impact of O(n) visual width calculation**: The current `WrapLayout` is O(1) because it assumes 1 char = 1 column. With tabs, computing visual width is O(n) in line length. This happens for every visible line, every frame. Mitigation: This is the same cost as the existing wide-character handling (`unicode_width` check per char), which is already O(n). If performance becomes an issue, we could cache visual widths per line with invalidation on edit, but this adds complexity. Start without caching and measure.

2. **Line content access in hit-testing**: `pixel_to_buffer_position_wrapped` currently only has access to line length via a closure. Tab-aware conversion requires line content. Options:
   - Extend the closure to return `(len, &str)` — but `&str` lifetime may be tricky
   - Pass a second closure `line_content_fn: Fn(usize) -> &str`
   - Fetch line content inside the function via the `BufferView` trait — but the function doesn't have a `BufferView` reference currently

   **Decision**: Extend the hit-testing function signature to accept line content access. The caller (`handle_mouse` in buffer_target.rs) has access to `ctx.buffer` which can provide line content.

3. **Wrap boundary edge cases**: When a tab character spans across a wrap boundary (visual column crosses `cols_per_row`), the tab should render only up to the wrap point, not extend into the next row. Verify this works correctly with the visual column arithmetic.

4. **Terminal tabs**: Terminal buffers (`TerminalBuffer`) also render via `GlyphBuffer`. Terminals typically handle tabs internally (the PTY expands tabs to spaces), so terminal content rarely contains literal `'\t'`. However, raw mode or certain applications might. Verify terminal rendering still works — the same tab-aware code path should handle it correctly.

5. **Syntax highlighting span boundaries**: Syntax highlighting spans have `(start_byte, end_byte)` ranges. If a span starts or ends in the middle of a tab's visual width, the background/underline rendering must handle this correctly. The current span-based rendering uses character indices which should map correctly to visual columns via the conversion functions.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->