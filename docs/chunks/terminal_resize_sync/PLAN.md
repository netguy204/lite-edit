<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause of the cursor misalignment is that `sync_pane_viewports()` updates
each tab's `Viewport.visible_lines` when the window resizes or a pane splits, but
for terminal tabs it does **not** call `TerminalBuffer::resize()`. This means
the alacritty grid retains its original row/column count while the viewport
expects a different number of visible lines. Programs query cursor position via
DSR/CPR and receive coordinates relative to the stale grid geometry.

The fix is straightforward: in `sync_pane_viewports()`, when iterating over
terminal tabs, compute the new grid dimensions from `(pane_content_height, pane_width)`
and `font_metrics`, then call `TerminalBuffer::resize(cols, rows)`. This updates
both the alacritty grid (via `Term::resize`) and the PTY (via `TIOCGWINSZ`).

We follow the existing pattern from `new_terminal_tab()` for computing
`rows = (content_height / line_height).floor()` and
`cols = (content_width / advance_width).floor()`.

To avoid excessive PTY writes during rapid resize events (e.g., dragging a window
edge), we'll add a simple guard: only call `TerminalBuffer::resize()` when the
computed `(cols, rows)` differs from the terminal's current `size()`.

**Key architectural pattern**: This chunk USES the `viewport_scroll` subsystem
(DOCUMENTED status) for viewport updates and follows the humble view principle
from TESTING_PHILOSOPHY.md — the model (TerminalBuffer + Viewport) is updated
on resize, and the renderer just projects state.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the subsystem.
  `Viewport::update_size()` is already called in `sync_pane_viewports()` for all
  tabs. This chunk adds a parallel resize call for terminal buffers to keep the
  alacritty grid in sync with the viewport's visible_lines. The subsystem's
  invariant "resize re-clamps scroll offset" is preserved because we continue
  calling `tab.viewport.update_size()`.

## Sequence

### Step 1: Write failing test for terminal resize on viewport sync

Create a test that:
1. Creates an EditorState with a terminal tab
2. Verifies the terminal's initial size
3. Simulates a window resize by calling `update_viewport_dimensions()` with new dimensions
4. Asserts that the terminal's `size()` matches the expected new dimensions

This test will fail because `sync_pane_viewports()` does not currently call
`TerminalBuffer::resize()`.

```rust
#[test]
fn test_sync_pane_viewports_resizes_terminal() {
    use crate::tab_bar::TAB_BAR_HEIGHT;
    use crate::left_rail::RAIL_WIDTH;

    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    // Create a terminal tab
    state.new_terminal_tab();

    // Get initial terminal size
    let initial_size = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal().unwrap();
        term.size()
    };

    // Resize window (double the height)
    state.update_viewport_dimensions(800.0, 1200.0 + TAB_BAR_HEIGHT);

    // Terminal should have more rows now
    let new_size = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal().unwrap();
        term.size()
    };

    // With double the content height, we should have roughly double the rows
    assert!(new_size.1 > initial_size.1,
        "Terminal rows should increase after resize: was {:?}, now {:?}",
        initial_size, new_size);
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 2: Add `as_terminal()` accessor to Tab

The test needs read-only access to the terminal buffer for assertions. Add a
method to `Tab` that returns `Option<&TerminalBuffer>`:

```rust
/// Returns a reference to the terminal buffer, if this is a terminal tab.
pub fn as_terminal(&self) -> Option<&TerminalBuffer> {
    match &self.buffer {
        TabBuffer::Terminal(term) => Some(term),
        TabBuffer::File(_) | TabBuffer::AgentTerminal => None,
    }
}
```

This mirrors the existing `terminal_and_viewport_mut()` pattern but is read-only
for test assertions.

Location: `crates/editor/src/workspace.rs` (impl Tab)

### Step 3: Add `as_terminal_mut()` accessor to Tab

For the resize implementation, we need mutable access to the terminal buffer
independent of the viewport:

```rust
/// Returns a mutable reference to the terminal buffer, if this is a terminal tab.
pub fn as_terminal_mut(&mut self) -> Option<&mut TerminalBuffer> {
    match &mut self.buffer {
        TabBuffer::Terminal(term) => Some(term),
        TabBuffer::File(_) | TabBuffer::AgentTerminal => None,
    }
}
```

Location: `crates/editor/src/workspace.rs` (impl Tab)

### Step 4: Modify `sync_pane_viewports()` to resize terminal tabs

In `EditorState::sync_pane_viewports()`, after computing the pane content
dimensions but before the `tab.viewport.update_size()` call, add terminal
resize logic:

```rust
// Chunk: docs/chunks/terminal_resize_sync - Propagate resize to terminal grid
// For terminal tabs, also resize the terminal buffer to keep the
// alacritty grid synchronized with the viewport dimensions.
if let Some(terminal) = tab.as_terminal_mut() {
    // Compute new terminal dimensions from pane content area
    let rows = (pane_content_height as f64 / self.font_metrics.line_height).floor() as usize;
    let cols = (pane_width as f64 / self.font_metrics.advance_width).floor() as usize;

    // Only resize if dimensions actually changed (avoid PTY thrashing)
    let (current_cols, current_rows) = terminal.size();
    if cols != current_cols || rows != current_rows {
        if cols > 0 && rows > 0 {
            terminal.resize(cols, rows);
        }
    }
}
```

**Key points:**
- We compute `pane_width` from `pane_rect.width` (need to capture this from the loop)
- The `font_metrics` must be accessible; store a copy at the start of the method
- We guard against zero dimensions and only resize when dimensions change

The full modified loop structure:

```rust
// Update each pane's tabs with the correct viewport dimensions
let line_height = self.font_metrics.line_height;
let advance_width = self.font_metrics.advance_width;

for pane_rect in &pane_rects {
    // Get the pane by ID
    let pane = match workspace.pane_root.get_pane_mut(pane_rect.pane_id) {
        Some(p) => p,
        None => continue,
    };

    // Calculate the pane's content dimensions (pane height/width minus tab bar)
    let pane_content_height = pane_rect.height - TAB_BAR_HEIGHT;
    let pane_width = pane_rect.width;

    // Update each tab's viewport in this pane
    for tab in &mut pane.tabs {
        // Chunk: docs/chunks/terminal_resize_sync - Resize terminal grid on layout change
        // For terminal tabs, resize the alacritty grid to match the new pane dimensions.
        // This ensures hosted programs (Claude Code, vim, htop) see the correct terminal
        // size via TIOCGWINSZ and position their cursors accurately.
        if let Some(terminal) = tab.as_terminal_mut() {
            let rows = (pane_content_height as f64 / line_height).floor() as usize;
            let cols = (pane_width as f64 / advance_width).floor() as usize;

            let (current_cols, current_rows) = terminal.size();
            if cols != current_cols || rows != current_rows {
                if cols > 0 && rows > 0 {
                    terminal.resize(cols, rows);
                }
            }
        }

        // Get the line count for this tab's content (existing code)
        // ...
    }
}
```

Location: `crates/editor/src/editor_state.rs#EditorState::sync_pane_viewports`

### Step 5: Run the test from Step 1

Verify the test now passes. The terminal size should increase when the window
is resized.

### Step 6: Write test for terminal resize on pane split

Create a test that verifies terminal resize works correctly when a pane splits
(reducing the content area):

```rust
#[test]
fn test_terminal_resize_on_split() {
    use crate::tab_bar::TAB_BAR_HEIGHT;

    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    // Create a terminal tab
    state.new_terminal_tab();

    // Get initial terminal size
    let initial_rows = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal().unwrap();
        term.size().1 // rows
    };

    // Split vertically (top/bottom panes)
    // This should halve the height available to each pane
    // ... invoke split command ...

    // Terminal should have fewer rows now (roughly half)
    let new_rows = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal().unwrap();
        term.size().1
    };

    assert!(new_rows < initial_rows,
        "Terminal rows should decrease after split: was {}, now {}",
        initial_rows, new_rows);
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 7: Write test for no-op resize when dimensions unchanged

Verify that `sync_pane_viewports()` doesn't call `TerminalBuffer::resize()` when
the terminal dimensions haven't changed (important for avoiding PTY thrashing):

```rust
#[test]
fn test_terminal_resize_skipped_when_unchanged() {
    use crate::tab_bar::TAB_BAR_HEIGHT;

    let mut state = EditorState::empty(test_font_metrics());
    state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

    // Create a terminal tab
    state.new_terminal_tab();

    // Get initial terminal size
    let initial_size = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal().unwrap();
        term.size()
    };

    // Call sync_pane_viewports again with the same dimensions
    state.sync_pane_viewports();

    // Size should be unchanged
    let new_size = {
        let ws = state.editor.active_workspace().unwrap();
        let tab = ws.active_pane().unwrap().active_tab().unwrap();
        let term = tab.as_terminal().unwrap();
        term.size()
    };

    assert_eq!(initial_size, new_size, "Terminal size should not change");
}
```

This test verifies the guard condition `if cols != current_cols || rows != current_rows`
is working correctly.

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 8: Run full test suite

Run `cargo test -p lite-edit` to verify:
- All new tests pass
- No regressions in existing terminal tests
- No regressions in viewport/scroll tests
- No regressions in pane split tests

### Step 9: Manual verification with vttest

After the code changes, manually run `vttest` in a terminal tab to verify:
1. **Cursor positioning (vttest 1)**: The E/+ border test should fill the entire screen
2. **Autowrap (vttest 1)**: Letters on the right margin should appear at consistent positions
3. **Origin mode (vttest 2)**: Text positioned at "bottom of screen" should appear at the actual last visible row

### Step 10: Manual verification with Claude Code

Run Claude Code in a terminal tab and verify:
1. After a window resize, the block cursor renders on the correct row (the input prompt line)
2. The cursor does not drift below the prompt after resize

### Step 11: Update GOAL.md code_paths

Add the files touched to the chunk's GOAL.md frontmatter:
- `crates/editor/src/editor_state.rs`
- `crates/editor/src/workspace.rs`

## Dependencies

- **tty_cursor_reporting** (ACTIVE): This chunk assumes DSR/CPR round-trip works correctly.
  The fix here ensures the CPR response contains coordinates for the correct grid geometry.
- **split_scroll_viewport** (ACTIVE): This chunk's `sync_pane_viewports()` function is
  the integration point for the terminal resize logic.

## Risks and Open Questions

- **Rapid resize performance**: The guard condition (only resize when dimensions change)
  should prevent PTY thrashing, but if users drag window edges very rapidly, we may
  still send many resize signals. This is acceptable because the PTY kernel buffers
  TIOCGWINSZ signals, and the terminal program (shell, vim, etc.) handles SIGWINCH
  debouncing internally.

- **Alt screen mode**: When in alternate screen mode (e.g., vim, htop), resize should
  still work. `TerminalBuffer::resize()` calls `self.term.resize(size)` which handles
  both primary and alternate screen grids. No special handling needed.

- **Cold scrollback during resize**: If a resize occurs while there's cold scrollback,
  the line count may change. The viewport update already handles this via
  `update_size(pane_content_height, line_count)`. No additional handling needed.

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