---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#test_terminal_cols_matches_wrap_layout_cols_per_row
    implements: "Terminal-renderer column alignment test with default font metrics"
  - ref: crates/editor/src/editor_state.rs#test_terminal_cols_with_realistic_font_metrics
    implements: "Terminal-renderer column alignment test with production font metrics (Intel One Mono 2x)"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- cache_reload_invalidation
- gotodef_cross_file_nav
---

# Chunk Goal

## Minor Goal

The terminal PTY is being told it has more columns (and possibly rows) than the
visible rendering area can actually display. This causes commands like `ls` to
format columnar output for a wider terminal than what's visible, resulting in
excessive soft wrapping that makes output unreadable. Additionally, after
repeated command output with this wrapping, the prompt becomes unreachable via
scrolling — the user scrolls as far down as the terminal allows and still
cannot see their prompt.

The column count is computed in `editor_state.rs` via
`content_width / font_metrics.advance_width`, which is then passed to
`TerminalBuffer::new()` and ultimately to `PtyHandle::spawn()` via
`portable_pty::openpty(size)`. The `terminal_resize_sync` chunk previously
addressed some resize propagation issues, but the initial size calculation
and/or resize propagation still produces a column count wider than what the
pane can render, causing the mismatch.

This chunk should:
1. Diagnose why the calculated column count exceeds the visible rendering area
2. Fix the size calculation so the PTY column/row count matches the actual
   visible character grid
3. Ensure resize events propagate the correct size to the PTY

## Success Criteria

- Running `ls` in a terminal pane shows columns that fit within the visible
  terminal width without soft wrapping
- After running many commands, the prompt remains reachable by scrolling to
  the bottom of the terminal
- The PTY's reported size (`stty size` or `tput cols`/`tput lines` from inside
  the shell) matches the actual number of characters that fit in the visible
  pane area
- Resizing the editor window or pane updates the PTY size to match the new
  visible area accurately