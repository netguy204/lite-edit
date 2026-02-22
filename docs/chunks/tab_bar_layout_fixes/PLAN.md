# Implementation Plan

## Approach

This chunk fixes two coordinate accounting bugs introduced by the `content_tab_bar` chunk:

1. **Buffer text renders over tab bar text** — The glyph rendering in `update_from_buffer_with_cursor` uses `quad_vertices_with_offset` which only accepts y_offset, while selection/cursor quads use `position_for_with_xy_offset` which applies both x_offset and y_offset. This inconsistency means glyphs may not be properly offset when the left rail and tab bar are visible.

2. **Click targeting is off by ~one line height** — The mouse coordinate transformation in `handle_mouse_buffer()` may have an inconsistency between how coordinates are passed and how they're transformed in `pixel_to_buffer_position`.

**Coordinate System Background**:

The codebase uses multiple coordinate systems:
- **NSView (macOS)**: Bottom-left origin (y=0 at bottom, y=view_height at top)
- **Metal rendering**: Top-left origin (y=0 at top)
- **Buffer/Screen coordinates**: Top-left origin, row 0 is top of viewport

When the tab bar is active:
- Tab bar occupies the top `TAB_BAR_HEIGHT` (32px) of the window
- Content area occupies NSView y ∈ [0, view_height - TAB_BAR_HEIGHT)
- Content must be rendered starting at Metal y = TAB_BAR_HEIGHT (shifted down)
- Mouse clicks in the content area must map correctly to buffer positions

**Fix Strategy**:

1. **For Bug #1**: Ensure `update_from_buffer_with_cursor` applies both `self.x_offset` and the effective y offset to glyph quads, consistent with how selection/cursor quads are positioned.

2. **For Bug #2**: Verify the mouse coordinate transformation accounts for both offsets correctly. The existing tests suggest the math may already be correct — need to verify through additional tests.

**Testing Philosophy Alignment**:

Following the Humble View Architecture from `TESTING_PHILOSOPHY.md`:
- Test coordinate transformation math (pure functions)
- Test mouse-to-buffer position mapping
- Visual rendering verified manually; glyph positioning math is testable

## Sequence

### Step 1: Write failing tests for glyph positioning with content offsets

Write tests that verify glyph quad positioning when x_offset and y_offset are set:

1. With x_offset=56 (RAIL_WIDTH) and y_offset=32 (TAB_BAR_HEIGHT), verify glyph at row=0, col=0 is positioned at (56, 32)
2. Verify selection quads and glyph quads have consistent positioning

These tests will fail initially, demonstrating the bug.

Location: `crates/editor/src/glyph_buffer.rs` (test module)

### Step 2: Fix glyph positioning in update_from_buffer_with_cursor

Change glyph quad creation from:
```rust
let quad = self.layout.quad_vertices_with_offset(screen_row, col, glyph, effective_y_offset, fg);
```
to:
```rust
let quad = self.layout.quad_vertices_with_xy_offset(screen_row, col, glyph, self.x_offset, effective_y_offset, fg);
```

This ensures glyphs are offset by both RAIL_WIDTH (x) and TAB_BAR_HEIGHT (y), consistent with how selection and cursor quads are rendered.

Location: `crates/editor/src/glyph_buffer.rs`, line ~708-709

### Step 3: Verify Step 2 tests pass

Run the tests from Step 1 to confirm the fix works:
```bash
cargo test -p lite-edit-editor glyph_buffer
```

### Step 4: Write test for Y coordinate click targeting

Write a test that verifies clicking in the content area (below the tab bar) correctly targets buffer lines:

1. Set up a view with view_height=320 and TAB_BAR_HEIGHT=32
2. Click at y coordinate targeting line 0 in the content area (NSView y ≈ 280)
3. Verify cursor lands on line 0, not line 1 or line -1

The existing test `test_mouse_click_accounts_for_rail_offset` handles X offset. Add a similar test for Y offset.

Location: `crates/editor/src/editor_state.rs` (test module)

### Step 5: Fix click targeting if tests fail

If the test from Step 4 fails, trace through:
1. `handle_mouse_buffer` coordinate adjustment (currently passes raw y, adjusts via content_height)
2. `EditorContext` creation with `content_height = view_height - TAB_BAR_HEIGHT`
3. `pixel_to_buffer_position` y-flip calculation: `flipped_y = content_height - y`

The expected behavior:
- Clicking at NSView y = (view_height - TAB_BAR_HEIGHT) - ε should target line 0 (top of content)
- Clicking at NSView y = 0 should target the bottom line of the visible content

Location: `crates/editor/src/editor_state.rs`

### Step 6: Run full test suite

Verify no regressions:
```bash
cargo test -p lite-edit-editor
```

### Step 7: Manual visual verification

After fixing both bugs, manually test:
1. Buffer text does not render within the tab bar strip at any scroll position
2. Clicking on line N in the buffer moves cursor to line N (not N±1)
3. Tab bar click-to-switch still works correctly
4. Scrolling doesn't cause content to overlap with tab bar
5. Left rail click handling is unaffected

Add backreference comments to modified code:
```rust
// Chunk: docs/chunks/tab_bar_layout_fixes - Fixed glyph positioning for tab bar/rail offsets
```

## Dependencies

- **content_tab_bar** (ACTIVE): This chunk is a direct child fixing bugs from the parent chunk. The tab bar rendering and coordinate system are already in place.

## Risks and Open Questions

1. **Coordinate system complexity**: Multiple coordinate flips (NSView ↔ Metal ↔ buffer) create opportunities for off-by-one errors. Each transformation must be traced carefully.

2. **Test coverage for Y offset**: The existing tests focus on X offset (left rail). The new tests added in this chunk should provide equivalent coverage for Y offset.

3. **Wrap vs non-wrap code paths**: `update_from_buffer_with_wrap` and `update_from_buffer_with_cursor` have different implementations. After fixing the non-wrapped path, verify the wrapped path is still correct.

4. **Fractional scroll interaction**: The y_offset passed to rendering methods includes both the tab bar offset and scroll pixel fraction. Ensure these compose correctly at boundary conditions.

## Deviations

1. **Bug #2 (click targeting) was already fixed**: The plan anticipated that click
   targeting would be off by one line height due to incorrect TAB_BAR_HEIGHT handling.
   However, upon investigation and testing, the coordinate transformation in
   `handle_mouse_buffer()` was already correctly implemented:
   - X coordinate is adjusted by subtracting RAIL_WIDTH
   - Y coordinate handling uses `content_height = view_height - TAB_BAR_HEIGHT`
     for the y-flip calculation in `pixel_to_buffer_position`

   A test was added to verify this behavior (`test_mouse_click_accounts_for_tab_bar_offset`)
   and it passed without any code changes. The original bug report may have been
   based on an earlier state of the code or a misdiagnosis.

2. **Only Bug #1 required a code fix**: The glyph rendering bug was real and required
   changing `update_from_buffer_with_cursor` to use `quad_vertices_with_xy_offset`
   instead of `quad_vertices_with_offset`, ensuring glyphs are properly offset by
   both RAIL_WIDTH (x) and TAB_BAR_HEIGHT (y).
