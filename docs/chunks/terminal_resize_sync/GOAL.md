---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_pane_viewports
    implements: "Propagates viewport resize to terminal grid by calling TerminalBuffer::resize() when pane dimensions change"
  - ref: crates/editor/src/workspace.rs#TabBuffer::as_terminal_buffer
    implements: "Read-only accessor for terminal buffer (used in test assertions)"
  - ref: crates/editor/src/workspace.rs#TabBuffer::as_terminal_buffer_mut
    implements: "Mutable accessor for terminal buffer (used in resize logic)"
  - ref: crates/editor/src/workspace.rs#Tab::as_terminal_buffer
    implements: "Tab-level delegation to TabBuffer::as_terminal_buffer()"
  - ref: crates/editor/src/workspace.rs#Tab::as_terminal_buffer_mut
    implements: "Tab-level delegation to TabBuffer::as_terminal_buffer_mut()"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- dirty_tab_close_confirm
- split_scroll_viewport
- tty_cursor_reporting
---

# Chunk Goal

## Minor Goal

Propagate viewport dimension changes to the terminal PTY and alacritty grid so
that hosted programs (Claude Code, vim, htop, etc.) always see the correct
terminal size and position their cursors accurately.

Currently, `sync_pane_viewports()` and `update_viewport_dimensions()` update
each tab's `Viewport.visible_lines` when the window resizes, a pane splits or
unsplits, or any other layout event changes the available content area. However,
neither path calls `TerminalBuffer::resize()`, so the terminal grid retains its
original row/column count and the PTY never receives a `TIOCGWINSZ` update.

This creates a mismatch: the viewport expects N visible rows, but the terminal
grid has M screen lines (where M was correct at creation time but is now stale).
The cursor's document-line offset is computed as
`cold_line_count + history_size + cursor_point.line`, and `scroll_to_bottom`
positions the viewport at `line_count - visible_lines`. When `visible_lines != screen_lines`,
the cursor renders at the wrong screen row — off by exactly
`visible_lines - screen_lines` rows.

This is the root cause of the persistent cursor misalignment in Claude Code
that the `tty_cursor_reporting` chunk did not resolve (CPR round-trip works,
but the reported coordinates are for a stale grid geometry).

### Evidence from vttest

Running `vttest` in a terminal tab confirms the grid/viewport mismatch across
multiple test categories:

**Cursor positioning (vttest 1):** The E/+ border test draws a frame that should
fill the entire screen. Instead the border occupies roughly the upper-left
quadrant — the grid is ~80x24 while the viewport is much larger.

**Autowrap (vttest 1):** The autowrap test fills left and right margins with
sequential letters. Letters on the right margin appear at two different column
positions — the grid's last column vs the viewport's actual right edge — and
letter sequences have gaps where the grid wraps at a narrower width than the
viewport.

**Wrap-around mode (vttest 2):** Three lines of `*`'s that should fill the full
screen width stop well short of the right edge, proving the grid column count
is smaller than the viewport width.

**Origin mode (vttest 2):** Text positioned at "the bottom of the screen"
appears in the vertical middle of the viewport. This is the clearest proof:
the grid thinks row ~24 is the bottom, but the viewport has ~45+ visible rows.

All failures are consistent with a single root cause: the terminal grid
dimensions are stale and smaller than the viewport area.

## Success Criteria

- When the window is resized, terminal tabs call `TerminalBuffer::resize(new_cols, new_rows)` with dimensions derived from the new pane content area and font metrics.
- When a pane splits or unsplits (changing the content area for terminal tabs), terminal resize is propagated in the same manner.
- The PTY receives the updated `TIOCGWINSZ` so that programs see the correct `stty size` / `$COLUMNS` / `$LINES` after a resize.
- The alacritty grid dimensions match the viewport's visible rows/cols at all times (no stale geometry).
- **Bug fix verification (Claude Code)**: After a window resize with Claude Code running in a terminal tab, the block cursor renders on the correct row (the input prompt line, not offset below it).
- **Bug fix verification (vttest)**: vttest cursor positioning test 1 draws the E/+ border filling the full screen edges. vttest origin mode test places "bottom of screen" text on the actual last visible row.
- Resize is debounced or idempotent — rapid resize events (e.g., dragging a window edge) do not cause excessive PTY writes or grid thrashing.
- Existing tests continue to pass; new test verifies that `sync_pane_viewports` triggers terminal resize when dimensions change.


