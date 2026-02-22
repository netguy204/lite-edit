---
decision: APPROVE
summary: Both layout bugs fixed with proper x_offset/y_offset handling in glyph rendering; click targeting verified via tests; no regressions.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Tab bar labels are fully visible and not obscured by buffer text at any scroll position

- **Status**: satisfied
- **Evidence**: The fix in `glyph_buffer.rs:711` changes glyph rendering from `quad_vertices_with_offset` (y-only) to `quad_vertices_with_xy_offset` (x and y), ensuring buffer text is properly offset by both `RAIL_WIDTH` (x) and `TAB_BAR_HEIGHT` (y). The effective y_offset calculation `y_offset - self.y_offset` positions glyphs below the tab bar. Tests `test_quad_vertices_with_xy_offset` and `test_position_for_with_xy_offset` verify this positioning math.

### Criterion 2: Buffer text does not render within the tab bar strip (Y < TAB_BAR_HEIGHT from top of content area)

- **Status**: satisfied
- **Evidence**: Same fix as Criterion 1. The `quad_vertices_with_xy_offset` call with the effective y_offset of -32 (derived from `y_offset - self.y_offset` where `self.y_offset = TAB_BAR_HEIGHT = 32`) positions glyphs at y=32 and below, not within the tab bar strip (y < 32). Test `test_quad_vertices_with_xy_offset` verifies that quad[0].position starts at [56.0, 32.0].

### Criterion 3: Clicking on a line in the buffer moves the cursor to that line, not the line above it

- **Status**: satisfied
- **Evidence**: Test `test_mouse_click_accounts_for_tab_bar_offset` explicitly verifies this by clicking at the center of line 0 and line 2 and asserting the cursor lands on exactly those lines. The test passes without any code changes to click handling, as noted in PLAN.md "Deviations" section - the coordinate transformation was already correct.

### Criterion 4: The click offset error is consistent at zero — not just approximately zero — across the full range of Y positions in the buffer

- **Status**: satisfied
- **Evidence**: The test `test_mouse_click_accounts_for_tab_bar_offset` uses exact integer line targeting (lines 0 and 2) with precise coordinate calculations derived from `content_height` and `line_height`. The assertions use `assert_eq!` with exact values, not approximate comparisons. The coordinate math in `pixel_to_buffer_position` uses `content_height = view_height - TAB_BAR_HEIGHT` for the y-flip, which is exact.

### Criterion 5: Existing tab bar click-to-switch behavior (clicking a tab switches to it) continues to work correctly

- **Status**: satisfied
- **Evidence**: All tab-related tests pass: `test_switch_tab_changes_active_tab`, `test_cmd_shift_right_bracket_next_tab`, `test_cmd_shift_left_bracket_prev_tab`, `test_close_button_contains`, and 20+ other tab tests. The chunk's changes are isolated to glyph rendering (`glyph_buffer.rs`) and do not modify tab bar click handling code.

### Criterion 6: No regression in left rail click handling

- **Status**: satisfied
- **Evidence**: Test `test_mouse_click_accounts_for_rail_offset` passes, verifying that X coordinate click handling (which accounts for RAIL_WIDTH) continues to work correctly. The chunk's glyph fix ensures consistency between glyph rendering and click handling by using the same x_offset for both.
