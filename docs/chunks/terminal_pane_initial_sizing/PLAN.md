<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause is that `new_terminal_tab()` computes terminal dimensions using the full window
content area (`view_width - RAIL_WIDTH`, `view_height - TAB_BAR_HEIGHT`) rather than the
active pane's actual dimensions. In a multi-pane layout, the active pane is only a fraction
of the window content area.

The fix has two parts:

1. **Get pane dimensions before spawning**: Use `get_pane_content_dimensions()` (added by
   the `vsplit_scroll` chunk) to query the active pane's dimensions rather than using the
   full window dimensions.

2. **Call `sync_pane_viewports()` after terminal creation**: This ensures the terminal's
   PTY is correctly sized even if the initial dimension calculation has any edge cases.
   This is consistent with the existing pattern where `sync_pane_viewports()` is called
   after tab movement operations (`Cmd+Shift+Arrow`).

The fix follows the existing architecture:
- `calculate_pane_rects()` computes the geometry from the pane tree
- `get_pane_content_dimensions()` wraps this for single-pane lookup
- `sync_pane_viewports()` iterates all panes and syncs terminal sizes

**Key pattern**: We already have all the infrastructure from `terminal_resize_sync` and
`vsplit_scroll` chunks. This fix connects those pieces to terminal creation.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem.
  The `sync_pane_viewports()` call at the end of terminal creation follows the subsystem's
  pattern for keeping viewport state consistent with layout geometry.

## Sequence

### Step 1: Write failing test for terminal initial sizing in split pane

Create a test that:
1. Creates an EditorState with a horizontal split (two panes side by side)
2. Creates a terminal tab in the active pane
3. Verifies that the terminal's columns match the pane width, NOT the full window width

The test should fail because `new_terminal_tab()` currently uses full window dimensions.

```rust
#[test]
fn test_terminal_initial_sizing_in_split_pane() {
    use crate::tab_bar::TAB_BAR_HEIGHT;
    use crate::left_rail::RAIL_WIDTH;

    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    // Create a file tab and split horizontally
    state.new_tab();
    if let Some(ws) = state.editor.active_workspace_mut() {
        ws.move_active_tab(Direction::Right);
    }
    state.sync_pane_viewports();

    // Now create a terminal tab in the active pane (right half)
    state.new_terminal_tab();

    // Get terminal size
    let (term_cols, _term_rows) = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal_buffer().unwrap();
        term.size()
    };

    // Calculate expected columns for the RIGHT pane (half of content area)
    let content_width = 800.0 - RAIL_WIDTH;
    let pane_width = content_width * 0.5; // Half due to horizontal split
    let expected_cols = (pane_width as f64 / test_font_metrics().advance_width).floor() as usize;

    // Terminal should be sized for the PANE, not the full window
    assert_eq!(term_cols, expected_cols,
        "Terminal should have {} columns for pane width {}, but has {}",
        expected_cols, pane_width, term_cols);
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 2: Modify `new_terminal_tab()` to use active pane dimensions

Refactor `new_terminal_tab()` to:
1. Get the active pane's ID from the workspace
2. Use `get_pane_content_dimensions()` to compute pane-specific dimensions
3. Fall back to full window dimensions if no pane dimensions are available (single-pane case
   or before view dimensions are set)

The key change is replacing:
```rust
// Current code (wrong in multi-pane)
let content_height = self.view_height - TAB_BAR_HEIGHT;
let content_width = self.view_width - RAIL_WIDTH;
```

With:
```rust
// Chunk: docs/chunks/terminal_pane_initial_sizing - Use pane dimensions for terminal sizing
// Get active pane ID to compute pane-specific dimensions
let pane_dimensions = self.editor.active_workspace()
    .and_then(|ws| Some(ws.active_pane_id))
    .and_then(|pane_id| self.get_pane_content_dimensions(pane_id));

let (content_height, content_width) = match pane_dimensions {
    Some((height, width)) => (height, width),
    None => {
        // Fall back to full window dimensions (single-pane or dimensions not set)
        (self.view_height - TAB_BAR_HEIGHT, self.view_width - RAIL_WIDTH)
    }
};
```

Location: `crates/editor/src/editor_state.rs#EditorState::new_terminal_tab`

### Step 3: Call `sync_pane_viewports()` after terminal creation

Add a call to `sync_pane_viewports()` at the end of `new_terminal_tab()`, after the tab
is added to the workspace. This ensures the terminal's PTY and viewport are correctly
synchronized with the pane geometry.

```rust
// Chunk: docs/chunks/terminal_pane_initial_sizing - Sync viewports after terminal creation
// Ensure the terminal is sized correctly for its pane, syncing the PTY and viewport.
// This is especially important in split layouts where the pane is smaller than the window.
self.sync_pane_viewports();
```

This call should be placed after the existing `sync_active_tab_viewport()` call but
before the `ensure_active_tab_visible()` call.

Location: `crates/editor/src/editor_state.rs#EditorState::new_terminal_tab`

### Step 4: Run the test from Step 1

Verify that the test now passes. The terminal should have columns matching the pane
width, not the full window width.

### Step 5: Write test for vertical split case

Add a test for vertical splits where pane height (not width) is reduced:

```rust
#[test]
fn test_terminal_initial_sizing_in_vertical_split() {
    use crate::tab_bar::TAB_BAR_HEIGHT;

    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    // Create a file tab and split vertically
    state.new_tab();
    if let Some(ws) = state.editor.active_workspace_mut() {
        ws.move_active_tab(Direction::Down);
    }
    state.sync_pane_viewports();

    // Create a terminal tab in the active pane (bottom half)
    state.new_terminal_tab();

    // Get terminal size
    let (_term_cols, term_rows) = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal_buffer().unwrap();
        term.size()
    };

    // Calculate expected rows for the BOTTOM pane (half of content area)
    let pane_height = 600.0 * 0.5; // Half due to vertical split
    let pane_content_height = pane_height - TAB_BAR_HEIGHT;
    let expected_rows = (pane_content_height as f64 / test_font_metrics().line_height).floor() as usize;

    // Terminal should be sized for the PANE, not the full window
    assert_eq!(term_rows, expected_rows,
        "Terminal should have {} rows for pane content height {}, but has {}",
        expected_rows, pane_content_height, term_rows);
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 6: Write test for single-pane case (regression test)

Ensure the fix doesn't break the single-pane case:

```rust
#[test]
fn test_terminal_initial_sizing_in_single_pane() {
    use crate::tab_bar::TAB_BAR_HEIGHT;
    use crate::left_rail::RAIL_WIDTH;

    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    // Create a terminal tab in the default single pane
    state.new_terminal_tab();

    // Get terminal size
    let (term_cols, term_rows) = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal_buffer().unwrap();
        term.size()
    };

    // Calculate expected dimensions for full content area
    let content_width = 800.0 - RAIL_WIDTH;
    let content_height = 600.0; // Full height (TAB_BAR_HEIGHT deducted from pane content)
    let pane_content_height = content_height - TAB_BAR_HEIGHT;

    let expected_cols = (content_width as f64 / test_font_metrics().advance_width).floor() as usize;
    let expected_rows = (pane_content_height as f64 / test_font_metrics().line_height).floor() as usize;

    assert_eq!(term_cols, expected_cols, "Terminal columns mismatch in single pane");
    assert_eq!(term_rows, expected_rows, "Terminal rows mismatch in single pane");
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 7: Run full test suite

Run `cargo test -p lite-edit` to verify:
- All new tests pass
- No regressions in existing terminal tests
- No regressions in viewport/scroll tests
- No regressions in pane split tests

### Step 8: Manual verification

Test the fix manually:
1. Open the editor and create a horizontal split (`Cmd+Shift+Right` on a file tab)
2. Open a new terminal in the right pane (`Cmd+Shift+T`)
3. The terminal should render correctly with proper wrapping from the start
4. `ls -la` output should wrap at the pane boundary, not the window boundary
5. The terminal should scroll to bottom correctly

### Step 9: Update GOAL.md code_paths

Add the files touched to the chunk's GOAL.md frontmatter:
- `crates/editor/src/editor_state.rs`

## Dependencies

- **terminal_resize_sync** (ACTIVE): This chunk builds on the `sync_pane_viewports()` infrastructure
  that correctly sizes terminals during resize events.
- **vsplit_scroll** (ACTIVE): This chunk uses `get_pane_content_dimensions()` introduced by
  vsplit_scroll for computing pane-specific dimensions.

## Risks and Open Questions

- **Timing during startup**: The `get_pane_content_dimensions()` function early-returns if
  view dimensions aren't set. The fallback to full window dimensions handles this case, but
  we should verify that `sync_pane_viewports()` at the end handles any edge cases.

- **Single-pane performance**: Adding a `sync_pane_viewports()` call adds a slight overhead
  to terminal creation. However, this function already guards against unnecessary terminal
  resizes (only resizes if dimensions changed), so the overhead is minimal for single-pane
  layouts where the computed dimensions match.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->
