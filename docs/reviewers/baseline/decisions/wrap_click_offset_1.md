---
decision: APPROVE
summary: "All success criteria satisfied: renderer and click handler now use consistent content_width_px for WrapLayout, with new tests verifying alignment"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Clicking anywhere on a soft-wrapped line (first row, second row, third row, etc.) positions the cursor at the character under the mouse pointer with no cumulative offset.

- **Status**: satisfied
- **Evidence**: The fix introduces `content_width_px` field in `Renderer` (line 215) and uses it in both `wrap_layout()` (line 315) and `update_glyph_buffer()` (line 408). This ensures the renderer wraps at the same column count as the click handler, eliminating cumulative offset errors on continuation rows.

### Criterion 2: The `WrapLayout` used by the renderer and the `WrapLayout` used by `pixel_to_buffer_position_wrapped` have identical `cols_per_row` values.

- **Status**: satisfied
- **Evidence**:
  - Renderer's `wrap_layout()` at line 315 uses `WrapLayout::new(self.content_width_px, &self.font.metrics)`
  - EditorContext's `wrap_layout()` at context.rs:73 uses `WrapLayout::new(self.view_width, &self.font_metrics)`
  - EditorState passes `self.view_width - RAIL_WIDTH` as view_width to EditorContext (editor_state.rs:1093, 1296, 1373)
  - Renderer initializes `content_width_px = (viewport_width_px - RAIL_WIDTH).max(0.0)` (line 262) and updates it on resize (line 334)
  - Both paths now compute the same `cols_per_row` value

### Criterion 3: Existing wrap rendering tests and click position tests continue to pass.

- **Status**: satisfied
- **Evidence**: `cargo test --workspace` passes (275 tests pass, only 2 pre-existing performance tests fail which are unrelated to this chunk). Specifically, all wrap_test.rs, viewport_test.rs, and mouse click tests in buffer_target.rs pass.

### Criterion 4: A new test verifies that clicking on the Nth continuation row of a wrapped line produces the correct buffer column (not offset by N × delta).

- **Status**: satisfied
- **Evidence**: Two new tests added in buffer_target.rs:
  1. `test_click_continuation_row_buffer_column` (line 4129): Tests clicking on continuation rows 1 and 2 of a 200-char line at 80 cols_per_row, verifying buffer_col = row_offset * 80 + screen_col
  2. `test_wrap_layout_cols_per_row_consistency` (line 4204): Documents the invariant that both renderer and click handler must use content_width, explicitly demonstrating the old bug and verifying the fix produces equal cols_per_row values
