---
decision: APPROVE
summary: Implementation correctly moves glyph buffer update to content rendering phase for single-pane mode, matching multi-pane behavior, with appropriate styled line cache handling for terminals.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Spawning a terminal tab via Cmd+Shift+T in a single-pane workspace renders the shell prompt within one frame of PTY output arrival

- **Status**: satisfied
- **Evidence**:
  - The early glyph buffer update (previously at `render_with_editor` lines 608-641) has been removed from the start of `render_with_editor()`
  - Glyph buffer update is now performed inside the content rendering block at `mod.rs:723-757`, after Metal drawable is acquired and scissor rect is set
  - This mirrors the multi-pane `render_pane()` behavior (lines 244-277 in `panes.rs`), ensuring terminal content is read at the correct time during the render pass
  - Added test `test_single_pane_terminal_dirty_and_content` verifies that terminal produces dirty regions and has content after polling
  - The fix correctly handles agent terminals, text buffers, and regular terminal buffers in the conditional block

### Criterion 2: The fix does not regress multi-pane terminal rendering

- **Status**: satisfied
- **Evidence**:
  - The multi-pane rendering path (`render_pane()` in `panes.rs`) is unchanged by this chunk
  - The conditional `if pane_rects.len() <= 1` (line 709) correctly routes single-pane vs multi-pane rendering
  - All 31 terminal-related tests in `editor_state::tests` pass, including split-pane tests like `test_terminal_initial_sizing_in_split_pane`, `test_terminal_resize_on_split`, and `test_terminal_initial_sizing_in_vertical_split`
  - All 57 tests in the terminal crate pass

### Criterion 3: The fix does not introduce unnecessary full-viewport invalidations (respect the existing dirty region / invalidation separation architecture)

- **Status**: satisfied
- **Evidence**:
  - The implementation does not modify the invalidation detection logic in `poll_agents()` or `render_if_dirty()`
  - Styled line cache clearing for terminals (`clear_styled_line_cache()` at line 739) is conditional on `is_terminal_tab`, preserving incremental invalidation for text buffers
  - The existing `invalidate_styled_lines(&dirty_lines)` path in `drain_loop.rs:506-507` is unaffected
  - The fix follows the renderer subsystem's layering contract by moving the update to occur at a consistent point in the render pass
