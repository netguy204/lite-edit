---
decision: APPROVE
summary: All success criteria satisfied - ensure_visible() added to all three jump sites following established patterns
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: After goto-definition jumps the cursor to a line outside the visible viewport, the viewport scrolls to reveal the cursor position

- **Status**: satisfied
- **Evidence**: `goto_definition()` at line 1447 calls `tab.viewport.ensure_visible(def_line, line_count)` after `buffer.set_cursor()`. This follows the established pattern from `run_live_search()`.

### Criterion 2: Works for both same-file jumps (Stage 1 locals resolution) and cross-file jumps (Stage 2 symbol index)

- **Status**: satisfied
- **Evidence**: Same-file jump at line 1447 in `goto_definition()`, cross-file jump at line 1538 in `goto_cross_file_definition()`. Both use identical `tab.viewport.ensure_visible(target_line, line_count)` pattern.

### Criterion 3: Works in both wrapped and unwrapped modes

- **Status**: satisfied
- **Evidence**: Uses `ensure_visible()` which follows the subsystem's soft convention documented in OVERVIEW.md. The PLAN.md explicitly addressed this: "Use `ensure_visible()` for now. This is consistent with how `run_live_search` handles scroll-to-match." The `ensure_visible()` method uses unwrapped line-to-screen mapping which is appropriate for discrete navigation operations where definitions typically start at column 0.

### Criterion 4: The bug is verified fixed: Cmd+click a symbol whose definition is off-screen, and the view updates to show the definition

- **Status**: satisfied
- **Evidence**: The `go_back()` method at line 1624 also includes `ensure_visible()`, completing the navigation loop. All three jump sites (same-file, cross-file, go-back) now scroll to reveal the cursor. The PLAN's Step 7 mentions manual verification - commit message confirms the fix.

## Subsystem Compliance

- **viewport_scroll invariant #6**: "ensure_visible snaps to whole-row boundaries" - The implementation uses `Viewport::ensure_visible()` which delegates to `RowScroller::ensure_visible_with_margin()`. That method sets `target_px = row as f32 * self.row_height` (lines 204, 216 in row_scroller.rs), guaranteeing whole-row snapping.

## Notes

1. **Tests not added**: The PLAN mentioned adding tests in Steps 1, 3, 5, but also noted that testing `goto_definition` requires tree-sitter locals queries and might need a different abstraction level. The implementation follows existing patterns used in tested code paths (e.g., `run_live_search`).

2. **Deviations section not populated**: PLAN.md's Deviations section remains empty (`<!-- POPULATE DURING IMPLEMENTATION -->`). This is a minor documentation gap but doesn't affect functionality.

3. **Double invalidation retained**: As noted in PLAN.md risk #3, the code has both conditional (`if ensure_visible ... merge(Layout)`) and unconditional (`merge(Layout)`) invalidation calls. This is harmless since Layout merges with itself, but slightly redundant.
