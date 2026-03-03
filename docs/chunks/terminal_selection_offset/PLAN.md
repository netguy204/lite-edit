<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The terminal click handler at `editor_state.rs:3055-3137` uses the wrong coordinate mapping for converting screen positions to buffer lines. It uses `first_visible_line() + row` which assumes 1:1 screen-row-to-buffer-line correspondence. When terminal output contains lines wider than the viewport (e.g., multi-column `ls` output), those lines soft-wrap to multiple screen rows, causing an offset between the visual click position and the calculated buffer line.

**The fix**: Replace the linear `first_visible_line() + row` mapping with the same wrap-aware approach used by:
1. The file editor click handler (`buffer_target.rs:pixel_to_buffer_position_wrapped`)
2. The renderer (`glyph_buffer.rs:update_from_buffer_with_wrap`)

Both of these use `Viewport::first_visible_screen_row()` and `Viewport::buffer_line_for_screen_row()` from the `viewport_scroll` subsystem.

**Key insight**: Terminal lines always have the same length (`terminal.line_len()` returns the terminal width for all lines), which makes the wrap arithmetic simpler than for text buffers. A `WrapLayout` can be constructed from the pane width and font metrics, then `buffer_line_for_screen_row()` will correctly map the clicked screen row to the buffer line.

**Testing approach**: Per `docs/trunk/TESTING_PHILOSOPHY.md`, the viewport coordinate mapping logic (`Viewport::buffer_line_for_screen_row`) is already well-tested. The fix brings the terminal click handler into compliance with these tested patterns. A targeted unit test will verify that the terminal-specific `screen_row_to_doc_position` helper function correctly handles wrapped lines.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk **USES** the viewport_scroll subsystem's `Viewport::buffer_line_for_screen_row()` and `WrapLayout` to perform wrap-aware coordinate mapping. The existing terminal click code is a deviation from this subsystem's patterns—it implements its own linear mapping instead of using the subsystem's wrap-aware methods.

  This fix brings the terminal click handler into compliance with the subsystem's soft conventions (from OVERVIEW.md): "Prefer `set_scroll_offset_px_wrapped` over `set_scroll_offset_px` when wrapping is enabled." The same principle applies to click coordinate mapping: prefer wrap-aware methods when content may wrap.

## Sequence

### Step 1: Create a helper function for wrap-aware terminal position calculation

Create a helper function `screen_row_to_doc_position()` that takes:
- `row`: The viewport-relative screen row (from pixel calculation)
- `col`: The column (from pixel calculation)
- `viewport`: Reference to the terminal's Viewport
- `terminal_cols`: The terminal width (from `terminal.line_len(0)` or `terminal.size().0`)
- `terminal_line_count`: The total line count
- `font_metrics`: For constructing WrapLayout
- `pane_width`: The pane content width (for wrap layout)

Returns a `Position` with the correct buffer line and column.

This mirrors the approach in `pixel_to_buffer_position_wrapped` but simplified for terminals:
1. Compute `first_visible_screen_row` from viewport
2. Compute `absolute_screen_row = first_visible_screen_row + row`
3. Call `Viewport::buffer_line_for_screen_row()` with a WrapLayout
4. Return `Position::new(buffer_line, col)` (column stays the same since terminal doesn't have tab expansion)

Location: `crates/editor/src/editor_state.rs` (inline or as a private function near the terminal mouse handler)

### Step 2: Update the selection position calculation

Replace the buggy linear mapping at lines 3082-3084:
```rust
// WRONG: let doc_line = viewport.first_visible_line() + row;
// let pos = Position::new(doc_line, col);
```

With a call to the new helper function or inline the wrap-aware logic:
```rust
let pos = screen_row_to_doc_position(row, col, viewport, terminal, &self.font_metrics, pane_width);
```

The `pane_width` can be extracted from the `hit` result (similar to how it's done for file tabs at line 3012-3024).

### Step 3: Update the PTY mouse forwarding coordinates

When forwarding mouse events to the PTY (lines 3075-3079), the `col` and `row` values sent to the PTY should also be wrap-aware. Programs running in the terminal (like vim, htop) expect coordinates relative to the terminal's own coordinate system.

**Important consideration**: For PTY forwarding, the program expects the _screen row within the viewport_, not the buffer line. The current `row` calculation is actually correct for PTY forwarding—it's the viewport-relative row. Only the selection code needs wrap-aware buffer line mapping.

After review: The PTY mouse encoding uses `col` and `row` which are screen-relative coordinates within the visible terminal area. These should NOT be converted to buffer lines—they're correct as-is. The fix only applies to the selection code path (lines 3080-3136).

### Step 4: Write a unit test for the wrap-aware position calculation

Add a test that verifies:
1. With no wrapping (all lines fit in viewport width), `row` maps directly to `first_visible_line + row`
2. With wrapping (some lines exceed viewport width), `row` correctly maps to the buffer line accounting for wrapped screen rows above
3. Edge cases: clicking on continuation rows of a wrapped line, clicking when scrolled partway down

Location: `crates/editor/src/editor_state.rs` in the `#[cfg(test)]` module, or `crates/editor/src/viewport.rs` if testing the underlying mapping function.

### Step 5: Manual verification

Before marking complete:
1. Open a terminal pane
2. Run `ls -la` in a directory with many files to produce wrapped output
3. Click on various lines including continuation rows of wrapped lines
4. Verify selection appears at the clicked position
5. Verify double-click word selection works correctly
6. Verify drag selection tracks accurately
7. Test with programs in mouse mode (e.g., vim) to ensure PTY forwarding still works

---

**BACKREFERENCE COMMENTS**

When implementing the fix, add a chunk backreference:
```rust
// Chunk: docs/chunks/terminal_selection_offset - Wrap-aware terminal click coordinates
```

Also reference the subsystem:
```rust
// Subsystem: docs/subsystems/viewport_scroll - Wrap-aware buffer line lookup
```

## Risks and Open Questions

1. **Terminal line length assumptions**: Terminal lines are fixed-width (equal to terminal column count), unlike text buffer lines which vary. This should simplify the wrap calculation, but verify that `terminal.line_len()` consistently returns the terminal width.

2. **Alternate screen mode**: When in alternate screen mode (e.g., vim, htop), there may be different wrapping behavior. Need to verify the fix works correctly in both primary and alternate screen modes.

3. **Scrollback with cold storage**: The terminal uses hot/cold scrollback storage. Ensure `line_count()` accounts for both when computing wrap offsets.

4. **PTY coordinate correctness**: Double-check that PTY mouse forwarding doesn't need changes. The current coordinates are viewport-relative which should be correct for terminal programs.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->