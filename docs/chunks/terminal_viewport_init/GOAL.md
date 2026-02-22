---
status: FUTURE
ticket: null
parent_chunk: terminal_tab_initial_render
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/main.rs
code_references: []
narrative: null
investigation: terminal_initial_render_failure
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_pty_wakeup
- terminal_alt_backspace
- terminal_tab_initial_render
---

# Chunk Goal

## Minor Goal

Fix terminal tab initial rendering by addressing the root cause: terminal tab viewports are created with `visible_rows=0`, which causes `scroll_to_bottom` to scroll past all content, producing a blank screen.

The previous attempt (`terminal_tab_initial_render`) added a spin-poll mechanism that waits up to 100ms for shell output. This doesn't fix the issue because even when shell output is captured, `poll_standalone_terminals` calls `scroll_to_bottom` with `visible_rows=0`, computing `max_offset = line_count * line_height` — which scrolls one full viewport past the end of the terminal content. The renderer then tries to display lines starting from an offset beyond the buffer, showing nothing.

A window resize fixes the problem because `update_viewport_dimensions` calls `viewport.update_size()` which sets `visible_rows` correctly and re-clamps the scroll offset.

The fix is to initialize the terminal tab's viewport `visible_rows` at creation time. The content height and line height are already known in `new_terminal_tab`. After the tab is added to the workspace, its viewport should be updated with the correct dimensions.

Additionally, the spin-poll mechanism (`pending_terminal_created`, `spin_poll_terminal_startup`) should be removed since it addresses a symptom rather than the cause. The existing PTY wakeup mechanism (`dispatch_async` → `handle_pty_wakeup`) already handles rendering when shell output arrives asynchronously.

## Success Criteria

- Creating a new terminal tab via Cmd+Shift+T renders the shell prompt immediately without requiring a window resize
- The terminal tab's viewport has correct `visible_rows` immediately after creation
- The spin-poll mechanism (`pending_terminal_created` flag, `spin_poll_terminal_startup` method, and the call site in `EditorController::handle_key`) is removed
- Existing terminal tab functionality (input, scrollback, resize, auto-follow) is unaffected
- No visible flicker or double-render artifacts on tab creation

## Relationship to Parent

The parent chunk `terminal_tab_initial_render` diagnosed the problem as a timing issue (shell output arrives after initial render) and added a spin-poll workaround. Investigation `terminal_initial_render_failure` revealed the actual root cause is that terminal viewports have `visible_rows=0`, which corrupts the scroll position when `scroll_to_bottom` is called.

What remains valid from the parent:
- The diagnosis that the initial render happens before shell output (correct, but not the real problem)
- The tests added in the parent chunk (should be adapted to test the viewport fix instead)

What is being changed:
- Removing the spin-poll mechanism entirely
- Adding viewport dimension initialization in `new_terminal_tab`
