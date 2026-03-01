<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The problem is that the terminal PTY receives a column/row count wider than what the
pane can actually render. After investigation of the related chunks:

- **`terminal_resize_sync` (ACTIVE)**: Propagates resize events to terminal grid when
  viewport dimensions change — this works correctly for resize events
- **`terminal_pane_initial_sizing` (ACTIVE)**: Fixed initial sizing to use pane-specific
  dimensions and added `sync_pane_viewports()` call after terminal creation

Despite these fixes, the column count still exceeds the visible rendering area. The
issue must be in the calculation itself — either the `content_width` used in
`cols = (content_width / advance_width).floor()` is too large, or the rendering area
calculation in the renderer doesn't match the PTY size calculation.

### Diagnosis Strategy

The bug has two manifestations:
1. **Column count too wide**: `ls` output wraps incorrectly (the reported problem)
2. **Prompt unreachable**: After repeated wrapped output, scrolling to bottom doesn't
   reach the prompt

Both symptoms suggest the PTY's column count exceeds the actual rendered character
grid. The root cause could be:

1. **Padding/margin not subtracted**: The renderer may account for horizontal padding
   (e.g., left margin for line numbers) that isn't subtracted from `content_width`
   before computing `cols`

2. **Fractional glyph math**: `floor(content_width / advance_width)` may include a
   partial character that doesn't actually fit

3. **Stale dimensions on first resize**: There may be a timing issue where the PTY
   gets initial dimensions before the pane layout is fully calculated

The fix follows the humble view architecture from TESTING_PHILOSOPHY.md: the model
(TerminalBuffer dimensions) must match what the renderer projects. We'll instrument
the calculation, identify the discrepancy, and fix the sizing math.

## Subsystem Considerations

- **docs/subsystems/spatial_layout** (DOCUMENTED): This chunk USES the spatial layout
  subsystem. The pane rect calculations from `calculate_pane_rects()` provide the
  `content_width` for terminal sizing. If the subsystem's output doesn't account for
  all rendering constraints (like terminal-specific margins), this chunk will need to
  adjust the values downstream.

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport
  subsystem. The `sync_pane_viewports()` function already handles terminal resize
  on layout changes. This chunk may need to adjust the dimension calculation
  within that function.

## Sequence

### Step 1: Instrument the terminal sizing calculation

Add diagnostic logging to understand the actual values being used. In both
`new_terminal_tab()` and `sync_pane_viewports()`, print:

- `content_width` (the pane width used for calculation)
- `advance_width` (font metric for character width)
- Computed `cols` result
- The PTY's actual reported size after creation (`stty size` equivalent)

Run the editor, create a terminal, and capture these values. Compare `cols` to
`tput cols` from inside the shell.

```rust
// Temporary diagnostic (remove before commit)
eprintln!("[DIAG] Terminal sizing: content_width={:.2}, advance_width={:.4}, cols={}",
          content_width, self.font_metrics.advance_width, cols);
```

Location: `crates/editor/src/editor_state.rs#new_terminal_tab` and `sync_pane_viewports`

### Step 2: Compare rendering width calculation

Examine the terminal renderer to understand how it computes the character grid
bounds. Look for:

- How `content_width` is derived in the render path
- Whether any horizontal padding/margins are applied
- The actual `x` coordinates used for character positioning

The renderer is in `crates/editor/src/renderer/`. Find the terminal rendering code
and trace the width calculation back to its source.

Verify: Does the renderer use the same `pane_width` as the sizing calculation?

Location: `crates/editor/src/renderer/` (search for terminal rendering)

### Step 3: Identify the sizing discrepancy

Based on Steps 1-2, identify where the mismatch originates:

**Hypothesis A**: The renderer applies padding that `new_terminal_tab()`/`sync_pane_viewports()` don't subtract

**Hypothesis B**: `calculate_pane_rects()` returns the full pane width but the
renderer uses a smaller content rect

**Hypothesis C**: Font metrics rounding causes an off-by-one in column count

Document the specific discrepancy and which component is "correct" (the renderer,
since it determines what's actually visible).

### Step 4: Fix the column calculation

Based on the identified discrepancy, modify the terminal sizing calculation to
match the renderer's actual character grid.

**If Hypothesis A (renderer padding):**
```rust
// Chunk: docs/chunks/terminal_size_accuracy - Account for terminal padding
// The terminal renderer applies TERMINAL_PADDING on left/right edges.
// Subtract this from content_width before computing columns.
let terminal_content_width = pane_width - (2.0 * TERMINAL_PADDING);
let cols = (terminal_content_width as f64 / advance_width).floor() as usize;
```

**If Hypothesis B (pane rect vs content rect):**
The fix would be in the rect calculation or extracting a different dimension.

**If Hypothesis C (rounding issue):**
```rust
// Use ceil for advance_width to ensure characters always fit
let cols = (content_width as f64 / advance_width).floor() as usize - 1;
```

Apply the fix to both locations:
- `crates/editor/src/editor_state.rs#new_terminal_tab`
- `crates/editor/src/editor_state.rs#sync_pane_viewports`

### Step 5: Write failing test for correct column count

Create a test that verifies the terminal column count matches the actual renderable
character grid. The test should:

1. Create a terminal with known window dimensions and font metrics
2. Verify that `tput cols` (simulated via terminal size) matches the expected
   renderable columns

```rust
#[test]
fn test_terminal_columns_match_renderable_width() {
    use crate::tab_bar::TAB_BAR_HEIGHT;
    use crate::left_rail::RAIL_WIDTH;

    let mut state = EditorState::empty(test_font_metrics());
    // Use a window width that would expose off-by-one errors
    // 800px - RAIL_WIDTH = content_width
    // content_width / advance_width should equal terminal cols
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    state.new_terminal_tab();

    let (term_cols, _term_rows) = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal_buffer().unwrap();
        term.size()
    };

    // Calculate expected columns based on the same formula the renderer uses
    // (this is the key: we're asserting model matches view)
    let content_width = 800.0 - RAIL_WIDTH;
    // Apply any padding the renderer applies
    let renderable_width = content_width; // Adjust this if padding is found
    let expected_cols = (renderable_width as f64 / test_font_metrics().advance_width).floor() as usize;

    assert_eq!(term_cols, expected_cols,
        "Terminal cols ({}) should match renderable cols ({}) for width {}",
        term_cols, expected_cols, content_width);
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 6: Test scrollback accessibility after wrapping

Write a test that verifies the prompt is reachable after output that wraps:

```rust
#[test]
fn test_terminal_prompt_reachable_after_wrapped_output() {
    // This test is more of an integration/manual test since it requires
    // simulating PTY output and verifying scroll behavior
    // For now, document as a manual verification step
}
```

The key assertion: after scroll_to_bottom(), the terminal's cursor row should be
within the visible viewport.

Location: Manual verification or integration test

### Step 7: Remove diagnostic logging

Remove the `eprintln!` diagnostic statements added in Step 1.

### Step 8: Run full test suite

Run `cargo test -p lite-edit` to verify:
- New tests pass
- No regressions in existing terminal tests
- No regressions in pane layout tests

### Step 9: Manual verification

1. Open the editor with a terminal
2. Run `tput cols` and verify it matches visible character columns
3. Run `ls -la` in a directory with long filenames — verify columns wrap at the
   visible boundary, not beyond
4. Generate many lines of output (e.g., `seq 1 500`)
5. Verify the prompt is reachable by scrolling to bottom

### Step 10: Update GOAL.md code_paths

Add files touched to the chunk's GOAL.md frontmatter:
- `crates/editor/src/editor_state.rs`
- Any renderer files if modified

## Dependencies

- **terminal_resize_sync** (ACTIVE): Established the `sync_pane_viewports()` pattern
  for propagating dimensions to terminal grid
- **terminal_pane_initial_sizing** (ACTIVE): Added pane-aware initial sizing and
  `sync_pane_viewports()` call after terminal creation

## Risks and Open Questions

- **What if the renderer doesn't have explicit padding?** The discrepancy may be
  more subtle (floating-point accumulation, rounding direction). Step 3's diagnosis
  must be thorough.

- **Multiple terminal tabs**: The fix must work for all terminals, not just the
  active one. `sync_pane_viewports()` already iterates all panes, but verify.

- **Edge case: very narrow panes**: When a pane is split many times, the column
  count may approach minimum values. Ensure guards against cols < 1 are in place.

- **Font metrics accuracy**: `advance_width` is a single value but some fonts
  have variable-width glyphs. For monospace fonts this should be consistent, but
  verify the test font metrics match production behavior.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->