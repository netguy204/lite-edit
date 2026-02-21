# Implementation Plan

## Approach

This chunk adds selection highlighting to the renderer so users can see what text is selected. The implementation follows the existing rendering architecture:

1. **Leverage the existing cursor pattern**: The cursor is already rendered as a solid-color quad using the atlas's `solid_glyph()`. Selection highlights will use the same technique—emit quads that sample from the solid atlas region, but draw them *before* the glyph pass so text remains readable on top.

2. **Two-pass rendering within single draw call**: Rather than creating a separate render pass, we'll emit selection highlight quads first into the vertex buffer, followed by glyph quads. Since both use the same vertex format and sample from the atlas, they can share the same draw call. The fragment shader already supports alpha blending, so we'll use a semi-transparent selection color.

3. **Selection state query**: During `GlyphBuffer::update_from_buffer_with_cursor`, query `buffer.selection_range()` to determine which visible lines intersect the selection. For each such line, emit highlight quads covering the selected columns.

4. **Dirty region integration**: Selection changes (anchor set, cursor moved during drag, selection cleared) already mark affected lines dirty via the existing `DirtyLines` mechanism from `text_selection_model`. No additional dirty tracking is needed.

### Key Design Decisions

- **Selection color**: Use Catppuccin Mocha "surface2" (`#585b70` → RGB `0.345, 0.357, 0.439`) with ~40% alpha. This is visible against the `#1e1e2e` background without overwhelming text.

- **Render order**: Selection quads → Glyph quads → Cursor quad. The existing alpha blending (`source * alpha + dest * (1 - alpha)`) handles the layering correctly.

- **Multi-draw vs single draw**: Keep single draw call. Splitting into multiple passes would complicate the render loop and add overhead. The vertex buffer already mixes cursor and text; adding selection quads is the same pattern.

Following docs/trunk/TESTING_PHILOSOPHY.md, the selection highlight geometry calculation is testable pure logic (in `GlyphBuffer` or a helper struct). The actual Metal rendering is a humble view and won't be unit tested.

## Sequence

### Step 1: Add selection color constant

Define the selection highlight color in `renderer.rs`:
- Add `SELECTION_COLOR: [f32; 4]` constant with Catppuccin surface2 at 40% alpha
- This color will be passed to the fragment shader for selection quads

Location: `crates/editor/src/renderer.rs`

### Step 2: Modify shader to support per-vertex color

Currently the fragment shader uses a single `text_color` uniform for all quads. To render selection highlights with a different color than text:

Option A: Add a per-vertex color attribute (changes vertex layout)
Option B: Use different draw calls with different fragment uniforms
Option C: Encode a "render mode" flag in the vertex data (e.g., UV sentinel value)

**Chosen approach: Option C (UV sentinel)** — Use a special UV region (the solid glyph) combined with checking if we're in the "selection" portion of vertices. Actually, a simpler approach:

**Revised approach**: Emit selection quads first, then draw them with a separate `setFragmentBytes` call before drawing glyph quads. This requires splitting into two draw calls but keeps the vertex format unchanged.

**Final approach**: Since the cursor already shares the glyph pipeline, the cleanest solution is:
1. Keep single vertex format
2. Emit selection quads → glyph quads → cursor quad
3. Draw selection quads with selection color, glyph quads with text color, cursor with text color

This requires 2-3 `drawIndexedPrimitives` calls with different fragment uniforms. Track the index ranges for each category.

Location: `crates/editor/src/renderer.rs`, `crates/editor/src/glyph_buffer.rs`

### Step 3: Extend GlyphBuffer to track quad categories

Modify `GlyphBuffer` to track index ranges for different quad types:
- `selection_index_range: Option<(usize, usize)>` — start/end indices for selection quads
- `glyph_index_range: Option<(usize, usize)>` — start/end indices for glyph quads
- `cursor_index_range: Option<(usize, usize)>` — start/end indices for cursor quad

The `update_from_buffer_with_cursor` method will populate these ranges as it emits quads.

Location: `crates/editor/src/glyph_buffer.rs`

### Step 4: Add selection quad generation to GlyphBuffer

In `update_from_buffer_with_cursor`:

1. Query `buffer.selection_range()` to get `(start, end)` positions
2. For each visible line that intersects the selection:
   - Calculate start column (0 if selection starts before this line, otherwise `start.col`)
   - Calculate end column (line length + 1 if selection extends past this line, otherwise `end.col`)
   - For each column in range, emit a solid-color quad at that position
   - Actually, emit one quad per selected region per line (not per character) for efficiency
3. Use `atlas.solid_glyph()` for UV coordinates (same as cursor)
4. Track the index range in `selection_index_range`

Helper method: `create_selection_quad(screen_row, start_col, end_col, glyph: &GlyphInfo) -> Vec<GlyphVertex>`

Location: `crates/editor/src/glyph_buffer.rs`

### Step 5: Update Renderer to draw selection quads with selection color

Modify `render_text` to:

1. If `glyph_buffer.selection_index_range()` is `Some((start, count))`:
   - Set fragment uniform to `SELECTION_COLOR`
   - Draw indexed primitives for selection indices
2. If `glyph_buffer.glyph_index_range()` is `Some((start, count))`:
   - Set fragment uniform to `TEXT_COLOR`
   - Draw indexed primitives for glyph indices
3. If `glyph_buffer.cursor_index_range()` is `Some((start, count))`:
   - Set fragment uniform to `TEXT_COLOR` (or a cursor color)
   - Draw indexed primitives for cursor indices

This splits the current single draw call into up to 3 draw calls, but they share the same pipeline state, vertex buffer, and index buffer.

Location: `crates/editor/src/renderer.rs`

### Step 6: Add unit tests for selection quad geometry

Test the pure geometry calculation:
- `test_selection_single_line` — selection within one line produces correct quad
- `test_selection_multiline` — selection spanning lines produces quads for each line
- `test_selection_partial_line_start` — selection starting mid-line
- `test_selection_partial_line_end` — selection ending mid-line
- `test_selection_empty` — no selection produces no quads
- `test_selection_outside_viewport` — selection outside visible range produces no quads

Location: `crates/editor/src/glyph_buffer.rs` (in `#[cfg(test)]` module)

### Step 7: Visual verification

Manually verify selection rendering works:
1. Build and run the editor
2. Programmatically set a selection (or if mouse_drag_selection is complete, use that)
3. Confirm highlight appears behind selected text
4. Confirm deselecting (cursor movement) removes highlight
5. Confirm text is readable on top of highlight

This step is visual/manual per TESTING_PHILOSOPHY.md (humble view).

### Step 8: Update GOAL.md code_paths and code_references

Add the touched files and key implementations to the chunk's GOAL.md frontmatter.

Location: `docs/chunks/text_selection_rendering/GOAL.md`

## Dependencies

- **text_selection_model (ACTIVE)**: This chunk depends on `TextBuffer::selection_range()` and related selection API. That chunk is marked ACTIVE in the GOAL.md, meaning it's complete and this chunk can proceed.

## Risks and Open Questions

1. **Performance with large selections**: Emitting many selection quads for large selections could increase vertex buffer size. For a full-buffer selection of 10K lines with 80 chars each, that's 10K quads × 4 vertices = 40K vertices for selection alone. This should still be fast (the buffer already handles glyph quads), but worth monitoring. Mitigation: emit one quad per line segment rather than per character.

2. **Draw call overhead**: Splitting from 1 to 3 draw calls adds overhead. Per Metal best practices, this is typically negligible compared to shader execution time, especially with our simple shaders. If profiling shows issues, we could explore vertex attributes for per-vertex color.

3. **Selection color contrast**: The chosen `#585b70` at 40% alpha should be visible but may need tuning. Easy to adjust the constant after visual testing.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->