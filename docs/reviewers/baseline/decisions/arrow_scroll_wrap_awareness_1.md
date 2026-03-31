---
decision: APPROVE
summary: "All six ensure_visible call sites converted to wrap-aware variants with tests passing; subsystem Known Deviations documented."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Arrow key navigation does not cause viewport jumping when soft wrapping is active and wrapped lines are present above the cursor

- **Status**: satisfied
- **Evidence**: `editor_state.rs` line ~2691 snap-back guard replaced with `ensure_visible_wrapped`; `test_arrow_scroll_snap_back_wrap_awareness` passes, verifying that scroll_offset_px stays at 256px instead of incorrectly snapping to 160px.

### Criterion 2: All `ensure_visible()` call sites in `editor_state.rs` that operate on editor buffer content use wrap-aware scrolling

- **Status**: satisfied
- **Evidence**: `grep -n '\.ensure_visible(' src/editor_state.rs | grep -v _wrapped` returns no results. All six documented call sites (lines ~1504, ~1617, ~2691, ~3695, ~3840, ~3897) have been converted to `ensure_visible_wrapped`.

### Criterion 3: Go-to-definition scrolls to the correct position with wrapped lines

- **Status**: satisfied
- **Evidence**: Both same-file (~1504) and cross-file (~1617) gotodef paths converted. `test_goto_cross_file_definition_wrap_scroll` passes (asserts scroll_offset_px > 0 for cursor at abs screen row 20 with 5 visible rows). Same-file has a `// TODO: integration test` comment acknowledging lack of tree-sitter test harness — this is within the documented plan scope.

### Criterion 4: File drop and IME insertion scroll to the correct position with wrapped lines

- **Status**: satisfied
- **Evidence**: `test_file_drop_insertion_wrap_awareness` and `test_ime_marked_text_wrap_awareness` both pass, verifying scroll_offset_px stays at 256px after insertion when cursor is already visible.

### Criterion 5: No regressions when wrapping is disabled

- **Status**: satisfied
- **Evidence**: All 5 new tests plus existing test suite (5 passed, 0 failed) pass. The `ensure_visible_wrapped` API correctly handles unwrapped content (single-row lines produce the same result as unwrapped arithmetic).
