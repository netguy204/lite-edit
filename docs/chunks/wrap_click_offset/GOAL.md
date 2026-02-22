---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/renderer.rs
code_references:
  - ref: crates/editor/src/renderer.rs#Renderer::content_width_px
    implements: "Content width field ensuring consistent WrapLayout construction"
  - ref: crates/editor/src/renderer.rs#Renderer::wrap_layout
    implements: "WrapLayout factory using content_width_px for hit-testing"
  - ref: crates/editor/src/renderer.rs#Renderer::update_glyph_buffer
    implements: "Glyph buffer update using content_width_px for rendering"
  - ref: crates/editor/src/renderer.rs#Renderer::update_viewport_size
    implements: "Resize handler updating both viewport_width_px and content_width_px"
  - ref: crates/editor/src/buffer_target.rs#test_click_continuation_row_buffer_column
    implements: "Test verifying click position on wrapped continuation rows"
  - ref: crates/editor/src/buffer_target.rs#test_wrap_layout_cols_per_row_consistency
    implements: "Test documenting cols_per_row parity invariant"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- scroll_bottom_deadzone_v3
- terminal_input_render_bug
---

# Chunk Goal

## Minor Goal

Fix mouse click cursor positioning on soft-wrapped lines. Currently, clicking on the first (unwrapped) row of a long line positions the cursor correctly, but clicking on continuation rows introduces a cumulative offset of ~3 characters per wrap row (first wrap is ~3 chars behind, second wrap is ~6 chars behind, etc.).

**Root cause:** The renderer creates its `WrapLayout` with the full window width (`viewport_width_px`), but the click handling path creates its `WrapLayout` with a narrower width (`view_width - RAIL_WIDTH`) via `EditorContext::wrap_layout()`. This mismatch means the renderer and click handler disagree on `cols_per_row`, causing the `screen_pos_to_buffer_col()` conversion to compute the wrong buffer column on continuation rows.

**Fix:** Introduce a single factory function (or method) that both the renderer and click handler call to construct their `WrapLayout`, guaranteeing they use the same recipe (same content-area width = `viewport_width_px - RAIL_WIDTH`, same font metrics). Currently the renderer calls `WrapLayout::new(self.viewport_width_px, ...)` with full window width while the click handler calls `WrapLayout::new(self.view_width, ...)` with content width — two call sites with different inputs. A shared factory eliminates this class of divergence.

## Success Criteria

- Clicking anywhere on a soft-wrapped line (first row, second row, third row, etc.) positions the cursor at the character under the mouse pointer with no cumulative offset.
- The `WrapLayout` used by the renderer and the `WrapLayout` used by `pixel_to_buffer_position_wrapped` have identical `cols_per_row` values.
- Existing wrap rendering tests and click position tests continue to pass.
- A new test verifies that clicking on the Nth continuation row of a wrapped line produces the correct buffer column (not offset by N × delta).



