---
decision: APPROVE
summary: "All success criteria satisfied — pane-aware viewport sync prevents scroll leak, with defense-in-depth guard and comprehensive tests"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Opening a new terminal in one side of a vertical split does not affect the scroll position of a buffer in the other side

- **Status**: satisfied
- **Evidence**: `sync_active_tab_viewport()` (editor_state.rs:870) now uses `get_pane_content_dimensions(pane_id)` to compute content height from actual pane geometry instead of full window height. This prevents `visible_rows` inflation that caused the scroll leak. Test `test_new_terminal_preserves_sibling_buffer_scroll` directly verifies this scenario passes.

### Criterion 2: Buffer scroll positions are preserved across all sibling pane operations (open, close, resize) that don't directly affect the buffer's own dimensions

- **Status**: satisfied
- **Evidence**: The dimension-change guard in `sync_pane_viewports()` (editor_state.rs:989-997) skips `update_size()` for non-terminal tabs when `visible_rows` wouldn't change, preventing unnecessary re-clamping from sibling pane operations. Tests cover: terminal creation in sibling pane (`test_new_terminal_preserves_sibling_buffer_scroll`), sibling pane close (`test_buffer_scroll_preserved_across_sibling_close`).

### Criterion 3: The fix correctly distinguishes between resize events that affect a pane's own size (which may legitimately need scroll adjustment) vs. events from sibling panes (which should not)

- **Status**: satisfied
- **Evidence**: The guard compares `new_visible` (computed from `pane_content_height`) against current `visible_lines()`. When a pane's own dimensions change, `new_visible != visible_lines()` and `update_size` fires with re-clamping. When only a sibling changes, the pane's dimensions are stable and the call is skipped. Terminal tabs are exempt from the guard (always update), preserving PTY resize behavior. Test `test_sync_active_tab_viewport_uses_pane_height` confirms pane-aware height is used, and `test_sync_active_tab_viewport_single_pane_unchanged` confirms the single-pane fallback still works.
