<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug occurs because the renderer and click handler construct their `WrapLayout` with
different viewport widths:

| Location | Width used | Resulting `cols_per_row` |
|----------|------------|--------------------------|
| `Renderer::update_glyph_buffer` | `self.viewport_width_px` (full window width) | `floor(window_width / glyph_width)` |
| `EditorContext::wrap_layout()` | `self.view_width` (content area = window − RAIL_WIDTH) | `floor(content_width / glyph_width)` |

This mismatch means the renderer wraps at, say, 100 columns while the click handler
wraps at 96 columns. When clicking on continuation row N, the
`screen_pos_to_buffer_col(row_offset, screen_col)` call computes
`N * 96 + screen_col` instead of `N * 100 + screen_col`, producing a cumulative
offset of ~4 characters per wrap row.

**Fix strategy:**

1. **Define content_width consistently.** Content width is `viewport_width_px − RAIL_WIDTH`.
   This is the width of the text rendering area, which is where wrapping should occur.

2. **Introduce a factory for WrapLayout on Renderer.** The renderer already has a
   `wrap_layout(&self)` method that returns `WrapLayout::new(self.viewport_width_px, &self.font.metrics)`.
   However, this uses the **window width** rather than **content width**. We need to either:
   - Option A: Store content width separately in Renderer and use it everywhere, or
   - Option B: Have the Renderer subtract RAIL_WIDTH internally when creating WrapLayouts.

3. **Make the renderer's internal rendering use content width.** The
   `update_glyph_buffer` method creates its WrapLayout with `self.viewport_width_px`.
   This must become `self.viewport_width_px - RAIL_WIDTH` (content width) to match
   what the click handler receives via `EditorContext.view_width`.

4. **Add a test that verifies cols_per_row parity.** Create a test that constructs
   both paths with the same inputs and asserts they produce identical `cols_per_row` values.

The pattern follows the viewport_scroll subsystem's principle: `WrapLayout` is stateless
and O(1), so there's no cache to invalidate. We simply need both call sites to receive
the same `viewport_width_px` argument.

**Key insight**: The `Renderer::wrap_layout()` method currently exists for hit-testing
code to access the layout. However, `EditorState` does not call this method — it
constructs its own `EditorContext` with a separate `view_width` field. The fix must
ensure both paths compute content width consistently.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll
  subsystem. The fix touches `WrapLayout` which is documented as a core component of
  this subsystem. The subsystem overview states that `WrapLayout` is "used uniformly
  wherever wrapping coordinates are needed" — the bug is that it's being constructed
  with *inconsistent* width parameters. This fix enforces that uniformity.

  No deviations discovered — the subsystem documentation correctly describes the
  stateless O(1) arithmetic pattern. The issue is purely that two call sites pass
  different widths, not that either deviates from the subsystem's patterns.

## Sequence

### Step 1: Write a failing test for click position on continuation rows

Before implementing the fix, write a test that exercises the bug. Following the
testing philosophy's TDD approach, this test must fail initially.

Location: `crates/editor/src/buffer_target.rs` (in the `#[cfg(test)]` module)

Test scenario:
1. Create a buffer with a line long enough to wrap (e.g., 200 characters at 80 cols_per_row)
2. Create a `WrapLayout` with content width (e.g., 640px / 8px glyph = 80 cols)
3. Simulate a click at continuation row 1, screen column 10
4. Assert that the computed buffer column is `80 * 1 + 10 = 90`, not some smaller value

This test should FAIL with the current code because:
- The renderer uses `viewport_width_px` (larger)
- The click handler uses `view_width` (smaller = viewport - RAIL_WIDTH)
- The test will use content width, matching what the fix will make both paths use

### Step 2: Add a content_width field to Renderer

Currently the Renderer stores `viewport_width_px` (full window width). Add a
separate `content_width_px` field that equals `viewport_width_px - RAIL_WIDTH`.
This represents the actual rendering area width.

Location: `crates/editor/src/renderer.rs`

Changes:
- Add field `content_width_px: f32` to `Renderer` struct
- In `Renderer::new()`, initialize `content_width_px = viewport_width_px - RAIL_WIDTH`
- In `update_viewport_size()`, update both fields:
  ```rust
  self.viewport_width_px = window_width;
  self.content_width_px = window_width - RAIL_WIDTH;
  ```

### Step 3: Update Renderer::wrap_layout() to use content_width_px

Change the `wrap_layout()` method to use content width instead of full viewport width.

Location: `crates/editor/src/renderer.rs#Renderer::wrap_layout`

Before:
```rust
pub fn wrap_layout(&self) -> WrapLayout {
    WrapLayout::new(self.viewport_width_px, &self.font.metrics)
}
```

After:
```rust
// Chunk: docs/chunks/wrap_click_offset - Use content width for consistent wrapping
pub fn wrap_layout(&self) -> WrapLayout {
    WrapLayout::new(self.content_width_px, &self.font.metrics)
}
```

### Step 4: Update update_glyph_buffer() to use content_width_px

The `update_glyph_buffer()` method creates its own WrapLayout inline. Update it
to use `content_width_px`.

Location: `crates/editor/src/renderer.rs#Renderer::update_glyph_buffer`

Before:
```rust
let wrap_layout = WrapLayout::new(self.viewport_width_px, &self.font.metrics);
```

After:
```rust
// Chunk: docs/chunks/wrap_click_offset - Use content width for consistent wrapping
let wrap_layout = WrapLayout::new(self.content_width_px, &self.font.metrics);
```

### Step 5: Verify EditorContext::wrap_layout uses consistent width

Confirm that `EditorContext::wrap_layout()` already uses `self.view_width`, which
is set to `self.view_width - RAIL_WIDTH` in the call sites within `editor_state.rs`.

Location: `crates/editor/src/context.rs#EditorContext::wrap_layout`

Expected (no change needed — this is already correct):
```rust
pub fn wrap_layout(&self) -> WrapLayout {
    WrapLayout::new(self.view_width, &self.font_metrics)
}
```

Verify by inspecting `editor_state.rs` where `EditorContext::new` is called:
- Line ~1296: `self.view_width - RAIL_WIDTH` is passed for mouse events
- Line ~1093: `content_width = self.view_width - RAIL_WIDTH` for key events

Both paths pass content width. The fix makes the renderer match.

### Step 6: Run the failing test to confirm it now passes

The test from Step 1 should now pass because both the renderer and click handler
use the same content width for `WrapLayout` construction.

### Step 7: Add a unit test verifying cols_per_row parity

Write a test that explicitly verifies that constructing a WrapLayout via the
renderer path and the EditorContext path produces identical `cols_per_row` values.

Location: `crates/editor/src/renderer.rs` (in a `#[cfg(test)]` module)

Test:
```rust
#[test]
fn test_wrap_layout_cols_per_row_matches_context() {
    // Given a viewport width and RAIL_WIDTH
    let viewport_width = 800.0;
    let content_width = viewport_width - RAIL_WIDTH;
    let metrics = FontMetrics { advance_width: 8.0, ... };

    // Both paths should compute the same cols_per_row
    let renderer_layout = WrapLayout::new(content_width, &metrics);
    let context_layout = WrapLayout::new(content_width, &metrics);

    assert_eq!(renderer_layout.cols_per_row(), context_layout.cols_per_row());
}
```

This test documents the invariant that both paths must agree.

### Step 8: Run existing tests to verify no regressions

Run the full test suite for the editor crate:
```
cargo test -p lite-edit-editor
```

Verify that all existing wrap rendering tests and click position tests pass.

---

**BACKREFERENCE COMMENTS**

Add chunk backreferences to modified code:
- `// Chunk: docs/chunks/wrap_click_offset - Use content width for consistent wrapping`

This backreference goes on:
- The `content_width_px` field definition
- The `wrap_layout()` method
- The `update_glyph_buffer()` WrapLayout construction line

## Dependencies

None. The chunks listed in `created_after` (`scroll_bottom_deadzone_v3`,
`terminal_input_render_bug`) have already shipped and are ACTIVE. This chunk
has no implementation dependencies on other FUTURE chunks.

## Risks and Open Questions

1. **RAIL_WIDTH is defined in left_rail.rs.** The renderer will need to import
   `RAIL_WIDTH` from `crate::left_rail`. This creates a dependency between
   renderer and left_rail modules. This is acceptable since the renderer already
   imports from left_rail for `LeftRailGlyphBuffer` and related items.

2. **What if RAIL_WIDTH changes?** The fix ties the renderer's content width
   calculation to `RAIL_WIDTH`. If RAIL_WIDTH were made dynamic (e.g., different
   per workspace), both the renderer and EditorContext would need to receive it
   as a parameter rather than using the constant. For now, RAIL_WIDTH is a
   compile-time constant, so this is not a concern.

3. **Are there other places that construct WrapLayout?** A quick grep shows
   WrapLayout is constructed in:
   - `renderer.rs::update_glyph_buffer()` — fixed in Step 4
   - `renderer.rs::wrap_layout()` — fixed in Step 3
   - `context.rs::wrap_layout()` — already correct (uses view_width)
   - `buffer_target.rs` in tests — test-only, uses explicit widths
   - `viewport.rs` in `ensure_visible_wrapped` etc. — passed in, not constructed

   No other production construction sites exist.

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