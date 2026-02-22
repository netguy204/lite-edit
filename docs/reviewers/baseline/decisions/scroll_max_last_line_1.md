---
decision: APPROVE
summary: All success criteria satisfied; implementation correctly passes content_height to viewport while preserving full view_height for coordinate operations.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Scrolling to the maximum position fully reveals the last line of the buffer

- **Status**: satisfied
- **Evidence**: `update_viewport_size` and `update_viewport_dimensions` now compute `content_height = window_height - TAB_BAR_HEIGHT` before passing to `viewport.update_size()` (editor_state.rs:248, 260). This ensures `visible_lines` is calculated from the actual content area, making `max_offset_px` correct so the last line can be fully revealed.

### Criterion 2: A regression test verifies that `visible_lines` is computed from the content area height

- **Status**: satisfied
- **Evidence**: Test `test_visible_lines_accounts_for_tab_bar` (editor_state.rs:1672-1689) verifies that with window_height=192 and TAB_BAR_HEIGHT=32, visible_lines equals 10 (computed from content_height=160, not window_height=192). The test also confirms view_height remains the full window height. Test passes.

### Criterion 3: Existing tests continue to pass; the fix does not regress click-to-cursor alignment or resize re-clamp behavior

- **Status**: satisfied
- **Evidence**: All 536 editor tests pass including 25 click-related tests, 90 selector tests, and the resize clamp test. The only failing tests are in unrelated subsystems (performance timing tests in buffer crate, flaky terminal test) with no changes to those codebases.

### Criterion 4: The selector overlay geometry, mouse-coordinate flipping, and tab-bar/rail hit-testing continue to use the full `view_height` / `view_width` values

- **Status**: satisfied
- **Evidence**: Implementation explicitly preserves `self.view_height = window_height` (editor_state.rs:250, 262) with comments explaining this is for coordinate flipping. Grep confirms `view_height` is still used for coordinate transforms (lines 950, 1018, 1062), tab bar hit-testing (line 906), and selector geometry. The regression test also asserts `state.view_height == 192.0`.

## Feedback Items

<!-- No feedback - all criteria satisfied. -->

## Escalation Reason

<!-- No escalation needed. -->
