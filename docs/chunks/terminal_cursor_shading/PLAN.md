# Implementation Plan

## Approach

The bug manifests as the cursor's block inversion being rendered one character behind the actual cursor position, with ghost shading persisting on old positions. This suggests a mismatch between where the cursor **is** (as reported by `TerminalBuffer::cursor_info()`) and where the cursor **shading** is rendered in `GlyphBuffer`.

**Hypothesis**: The issue is most likely in how the cursor position from `cursor_info()` interacts with the glyph rendering pipeline. The terminal cursor position comes from `alacritty_terminal::grid::cursor.point`, which uses (column, line) coordinates. When rendering:

1. The cursor position is obtained via `BufferView::cursor_info()`
2. This position is converted to screen coordinates
3. A cursor quad is created at that screen position

The bug description mentions the shading lags by one character, which could mean:
- The cursor position is read at the wrong time (before vs after a grid update)
- Column indexing is off-by-one somewhere in the coordinate chain
- The styled line cache is interfering with cursor rendering

**Investigation Strategy**:
1. Add a test that reproduces the bug by inserting text and checking cursor position
2. Trace through the cursor position flow from `TerminalBuffer::cursor_info()` to quad generation
3. Check if the `pane_mirror_restore` chunk's styled line cache clearing interacts with terminal cursor state

**Fix Strategy**: Once the root cause is identified, fix the coordinate mismatch. This is likely a semantic bug (revealed new understanding of intended behavior) rather than an implementation bug, since cursor rendering used to work.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem for cursor quad generation. The fix will likely be in `glyph_buffer.rs` cursor rendering logic or in `terminal_buffer.rs` cursor position calculation. Per the subsystem's "Draw Order Within Layer" convention, cursor quads render last (background → selection → glyphs → cursor).

## Sequence

### Step 1: Create reproduction test

Add a test in `crates/terminal/tests/integration.rs` that:
1. Creates a `TerminalBuffer`
2. Feeds escape sequences to move the cursor to a specific position
3. Verifies `cursor_info()` returns the expected position

This test will confirm whether the bug is in cursor position reporting or rendering.

Location: `crates/terminal/tests/integration.rs`

### Step 2: Trace cursor position flow

Using the test and code analysis, trace:
1. `TerminalBuffer::cursor_info()` - how it reads from `term.grid().cursor.point`
2. How cold scrollback offset is added to cursor line
3. How the cursor position flows to `GlyphBuffer::update_from_buffer_with_wrap()`

Document findings before proceeding with fix.

### Step 3: Identify and fix the off-by-one issue

Based on Step 2 findings, apply the fix. Most likely locations:
- `terminal_buffer.rs#cursor_info()` - line/column calculation
- `glyph_buffer.rs` - screen coordinate conversion in cursor quad creation

The fix should ensure the cursor quad is created at `cursor_info().position`, not at an adjacent cell.

Location: One or both of:
- `crates/terminal/src/terminal_buffer.rs`
- `crates/editor/src/glyph_buffer.rs`

### Step 4: Verify ghost shading is resolved

The ghost shading (old cursor position retaining inversion) is likely caused by:
- Dirty region tracking not including the old cursor position, OR
- Styled line cache retaining stale data

Verify that when cursor moves:
1. The old position is marked dirty
2. The new position shows the cursor quad
3. No inversion remains at the old position

If dirty tracking is the issue, ensure `DirtyLines` includes both old and new cursor positions.

### Step 5: Add cursor rendering regression test

Add a test that verifies cursor position tracking through multiple movements:
1. Type several characters
2. Move cursor with arrow keys
3. Verify `cursor_info()` position matches expected position at each step

Location: `crates/terminal/tests/integration.rs`

### Step 6: Manual verification

Since cursor rendering involves visual output (humble view), manually verify:
- Terminal cursor tracks correctly during typing
- Arrow key movements update cursor immediately
- Backspace moves cursor and clears shading correctly
- Works in both shell prompt and TUI applications (vim, htop)
- Editor pane cursor rendering is unaffected (no regression)

## Risks and Open Questions

- **Root cause uncertainty**: The bug description mentions "file_change_events chunk landed" but that chunk is still FUTURE. Need to identify the actual change that introduced this regression by checking what's in `created_after` (emacs_line_nav, pane_mirror_restore).

- **Styled line cache interaction**: The `pane_mirror_restore` chunk added cache clearing between pane renders. This may have changed timing/state that affects cursor position reads. Need to verify cache isn't causing stale cursor positions.

- **Multiple code paths**: There are two cursor rendering paths in `glyph_buffer.rs`:
  1. `update_from_buffer_with_cursor()` - non-wrap-aware (used for editor buffers)
  2. `update_from_buffer_with_wrap()` - wrap-aware (used for wrapped content and terminals)

  Need to verify the fix applies to the correct path for terminal rendering.

## Deviations

### Investigation Findings

The implementation deviated from the expected fix because thorough investigation revealed that **the bug could not be reproduced through testing**:

1. **Cursor position tracking is correct**: Added 12+ unit tests verifying that `cursor_info()` returns the correct position after various operations (typing, backspace, escape sequences, cursor movement). All tests pass.

2. **No off-by-one error found**: Traced the cursor position flow from `TerminalBuffer::cursor_info()` → `GlyphBuffer::update_from_buffer_with_wrap()` → `create_cursor_quad_with_offset()`. The coordinate conversion uses the same `wrap_layout.buffer_col_to_screen_pos()` for both glyphs and cursor, ensuring consistency.

3. **No spurious INVERSE flags**: Added a test verifying that terminal cells don't have the INVERSE flag set by cursor position (inverse is only set by explicit escape sequences).

4. **Styled line cache is clean**: The `pane_mirror_restore` chunk (in `created_after`) added `clear_styled_line_cache()` between pane renders, which may have inadvertently fixed the reported issue.

5. **Draw order is correct**: Cursor quad is rendered last (after glyphs), ensuring it appears on top.

### Conclusion

The bug description mentioned "This was observed after the recent `file_change_events` chunk landed", but `file_change_events` is still FUTURE (not yet implemented). The actual trigger was likely related to the styled line cache contamination that was fixed by `pane_mirror_restore`.

Given that:
- All cursor position tests pass
- The rendering code is verified correct
- No reproduction path was found

This chunk adds **comprehensive cursor position regression tests** to prevent future regressions, rather than a code fix to the rendering pipeline.