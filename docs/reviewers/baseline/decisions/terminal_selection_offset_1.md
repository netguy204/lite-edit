---
decision: APPROVE
summary: All success criteria satisfied; implementation correctly uses wrap-aware coordinate mapping following established patterns from viewport_scroll subsystem.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Clicking on a terminal line selects text at the correct visual position, even when lines above have soft-wrapped

- **Status**: satisfied
- **Evidence**: The implementation at `editor_state.rs:3138-3171` now uses `viewport.first_visible_screen_row()` plus the clicked row to get `absolute_screen_row`, then calls `Viewport::buffer_line_for_screen_row()` with a properly constructed `WrapLayout` to find the correct buffer line. This replaces the buggy `first_visible_line() + row` mapping that assumed 1:1 correspondence between screen rows and buffer lines. The fix correctly handles wrapped terminal lines where one buffer line may span multiple screen rows.

### Criterion 2: Double-click word selection in the terminal works at the correct position

- **Status**: satisfied
- **Evidence**: The word selection code path at `editor_state.rs:3177-3199` uses the same `pos` variable that is now computed with wrap-aware mapping. Since `doc_line` is correctly calculated, the `Position::new(doc_line, col)` used for word selection will point to the correct buffer line, and the word boundary detection (`styled_line(pos.line)`) will operate on the correct line content.

### Criterion 3: Drag selection in the terminal tracks the mouse position accurately

- **Status**: satisfied
- **Evidence**: The drag handling code at `editor_state.rs:3175` (and the Drag match arm) uses the same `pos` variable computed with wrap-aware mapping. Both anchor and head positions in the selection will use correctly mapped buffer coordinates, so drag selection tracks the visual mouse position accurately.

### Criterion 4: Terminal mouse events forwarded to the PTY (when mouse mode is active) use correct cell coordinates

- **Status**: satisfied
- **Evidence**: The PTY mouse forwarding code at `editor_state.rs:3133-3137` correctly uses the viewport-relative `col` and `row` values (not buffer lines). This is documented in the code comments (line 3130-3132): "PTY mouse encoding uses viewport-relative row (correct as-is), not buffer line." Terminal programs like vim/htop expect screen-relative coordinates, which the original code already provided correctly. The fix only changes the selection code path.

### Criterion 5: The fix follows the same pattern as `pixel_to_buffer_position_wrapped` in `buffer_target.rs`

- **Status**: satisfied
- **Evidence**: Comparing the fix with `buffer_target.rs:885-932`:
  1. Both compute `first_visible_screen_row` from viewport
  2. Both compute `absolute_screen_row = first_visible_screen_row + row`
  3. Both call `Viewport::buffer_line_for_screen_row()` with WrapLayout and a line length function
  4. The terminal version correctly uses `|_line| terminal_cols` since all terminal lines have uniform width, matching the pattern but simplified for terminal's fixed-width lines

  The backreferences to the chunk and subsystem are properly added (lines 3112-3113, 3140), following the pattern established in the codebase.

## Subsystem Compliance

The implementation correctly uses the `viewport_scroll` subsystem's wrap-aware methods:
- `Viewport::first_visible_screen_row()` - for getting scroll position in screen row space
- `Viewport::buffer_line_for_screen_row()` - for inverse mapping from screen row to buffer line
- `WrapLayout::new()` - for constructing the coordinate mapping

This brings the terminal click handler into compliance with the subsystem's soft conventions, as noted in PLAN.md.

## Testing

Two new tests were added in `viewport.rs`:
1. `test_buffer_line_for_screen_row_terminal_fixed_width_lines` - verifies wrap mapping for terminal's uniform-width lines
2. `test_terminal_click_with_scroll_and_wrapping` - regression test that verifies the old buggy code would give wrong results and the new code gives correct results

Both tests pass, and all 2700+ existing tests continue to pass.
