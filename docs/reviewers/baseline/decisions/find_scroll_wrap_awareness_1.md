---
decision: APPROVE
summary: All four success criteria satisfied; ensure_visible_wrapped_with_margin is correctly implemented with full test coverage and advance_to_next_match is covered via delegation to run_live_search.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: When soft wrapping is active and a find match is on or after wrapped lines, the viewport scrolls so the match highlight is visible within the viewport

- **Status**: satisfied
- **Evidence**: `viewport.rs` — `ensure_visible_wrapped_with_margin` computes absolute screen rows by summing `wrap_layout.screen_rows_for_line()` for all preceding lines plus the sub-row offset from `buffer_col_to_screen_pos`. `editor_state.rs:2031` — `run_live_search()` now calls this method. Integration test `test_find_scroll_wrap_awareness` (editor_state.rs:8912) sets up 4 wrapping lines (2 screen rows each) before the match and asserts `scroll_offset_px > 0`.

### Criterion 2: The match is not obscured by the find strip (margin still respected)

- **Status**: satisfied
- **Evidence**: `editor_state.rs:2038` — `margin=1` is passed to `ensure_visible_wrapped_with_margin`, matching the previous `ensure_visible_with_margin` margin. `viewport.rs` — `effective_visible = visible_lines.saturating_sub(bottom_margin_rows).max(1)` reduces the effective window. Unit test `test_ensure_visible_wrapped_with_margin_margin_shrinks_effective_window` (viewport.rs) confirms margin=1 causes scrolling 1 row earlier than margin=0.

### Criterion 3: Scrolling to matches still works correctly when wrapping is disabled

- **Status**: satisfied
- **Evidence**: `ensure_visible_wrapped` now delegates to `ensure_visible_wrapped_with_margin` with `margin=0` (`viewport.rs:304`). Unit test `test_ensure_visible_wrapped_with_margin_margin0_same_as_no_margin` confirms that margin=0 produces identical results to the former `ensure_visible_wrapped` — meaning existing wrapped and non-wrapped behaviour is preserved. All 601 tests pass.

### Criterion 4: Both `run_live_search()` (live incremental search) and `advance_to_next_match()` (Return key) use wrap-aware scrolling

- **Status**: satisfied
- **Evidence**: `editor_state.rs:2080` — `advance_to_next_match()` delegates directly to `self.run_live_search()`. Since `run_live_search()` now uses `ensure_visible_wrapped_with_margin`, both paths are covered. No separate update to `advance_to_next_match` was required, and none was missed.

## Feedback Items

<!-- For FEEDBACK decisions only. Delete section if APPROVE. -->

## Escalation Reason

<!-- For ESCALATE decisions only. Delete section if APPROVE/FEEDBACK. -->
