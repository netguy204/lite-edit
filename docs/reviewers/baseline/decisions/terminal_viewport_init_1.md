---
decision: APPROVE
summary: All success criteria satisfied; terminal viewport initialization implemented correctly with comprehensive testing and spin-poll mechanism fully removed.
operator_review: null
---

## Criteria Assessment

### Criterion 1: Creating a new terminal tab via Cmd+Shift+T renders the shell prompt immediately without requiring a window resize

- **Status**: satisfied
- **Evidence**: The `new_terminal_tab()` function in `editor_state.rs:2158-2167` now calls `tab.viewport.update_size(content_height, line_count)` immediately after the tab is added to the workspace. This ensures `visible_rows` is correctly computed before any `scroll_to_bottom` call, which prevents the scroll offset from exceeding the content bounds.

### Criterion 2: The terminal tab's viewport has correct `visible_rows` immediately after creation

- **Status**: satisfied
- **Evidence**: The implementation at `editor_state.rs:2162-2166` retrieves the newly added tab's viewport and calls `update_size()` with the known `content_height` (computed at the start of the function) and `line_count` from the buffer. The test `test_terminal_viewport_has_visible_rows_immediately()` at lines 5813-5850 explicitly validates that `visible_lines() > 0` immediately after `new_terminal_tab()` is called, and that the value matches the expected calculation.

### Criterion 3: The spin-poll mechanism (`pending_terminal_created` flag, `spin_poll_terminal_startup` method, and the call site in `EditorController::handle_key`) is removed

- **Status**: satisfied
- **Evidence**:
  - The `pending_terminal_created` field was removed from `EditorState` struct
  - The `spin_poll_terminal_startup()` method (~35 lines) was deleted
  - The call site in `main.rs:253-262` that called `spin_poll_terminal_startup()` after key events was removed
  - Grep search for `pending_terminal_created|spin_poll_terminal_startup` in `crates/` returns no matches (only documentation references remain)

### Criterion 4: Existing terminal tab functionality (input, scrollback, resize, auto-follow) is unaffected

- **Status**: satisfied
- **Evidence**: All 644 tests pass, including extensive terminal-related tests like `test_poll_agents_dirty_after_terminal_creation`, `test_terminal_scroll_updates_viewport`, `test_terminal_viewport_is_at_bottom_initial`, and many others. The change is additive (adds viewport initialization) and subtractive (removes workaround), without modifying the core terminal input, scrollback, or resize logic.

### Criterion 5: No visible flicker or double-render artifacts on tab creation

- **Status**: satisfied
- **Evidence**: The implementation marks `DirtyRegion::FullViewport` only once at the end of `new_terminal_tab()`. The viewport is initialized synchronously before any render can occur, so there's no intermediate state with incorrect viewport dimensions. The removal of the spin-poll mechanism (which introduced 100ms of blocking delays) actually improves the creation flow by eliminating potential jank.
