<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause of the pane scroll coupling is that `Renderer` owns a single shared
`Viewport` instance (line 259 of `renderer.rs`) that is synchronized only with the
focused tab's scroll state. In multi-pane mode, the `render_pane()` function uses
this shared viewport for all panes, causing:

1. **Coupled scrolling**: All panes render from the focused tab's scroll offset
2. **Height clamping**: `visible_lines` is computed for full window, not per-pane
3. **Jitter**: `ensure_visible()` on focused tab moves the shared viewport
4. **Stale wrap width**: `content_width_px` updates happen at wrong time or persist

**Strategy**: Before rendering each pane, configure the renderer's viewport state
from that pane's active tab. This is a "borrow and configure" pattern rather than
passing viewports through the call stack (which would require extensive signature
changes).

The key insight is that each `Tab` already owns its own `Viewport` (line 203 of
`workspace.rs`). The fix is to:
1. Copy the tab's viewport scroll state into the renderer's viewport before drawing
2. Update `visible_lines` based on the pane's actual height
3. Update `content_width_px` before calling `update_glyph_buffer_with_cursor_visible()`

This approach:
- Minimizes code churn (no new types or ownership changes)
- Follows the existing pattern where tabs own scroll state
- Aligns with the viewport_scroll subsystem's invariant that viewport dimensions
  must be updated before rendering

**Testing Strategy**: Per TESTING_PHILOSOPHY.md's "Humble View Architecture", the
rendering path is a humble object that projects state onto the screen. We will add
unit tests for the testable components:
- Viewport dimension updates based on pane height
- WrapLayout coordinate calculations at different widths
- The logic that selects which tab's viewport to use per pane

The visual behavior (correct rendering with independent scroll) will be verified
manually since it requires GPU output.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS the
  subsystem's patterns. The subsystem states that `scroll_offset_px` is the single
  source of truth and that viewport dimensions must be updated before rendering.
  The fix ensures each pane rendering pass correctly configures the viewport.

  Key invariants to maintain:
  - Invariant 1: `scroll_offset_px` is authoritative (we'll copy from tab's viewport)
  - Invariant 7: Resize re-clamps scroll offset (we'll call `update_size()` per pane)
  - Soft convention 1: Use wrapped scroll methods when wrapping is enabled

## Sequence

### Step 1: Add helper method to configure renderer viewport from tab

Add a method to `Renderer` that configures its viewport for a specific pane/tab.

Location: `crates/editor/src/renderer.rs`

The method will:
1. Copy the tab's viewport scroll offset (unclamped, since tab owns the state)
2. Update `visible_lines` based on pane content height (pane_rect.height - TAB_BAR_HEIGHT)
3. Update `content_width_px` to the pane's width
4. Update `viewport_width_px` if needed for other calculations

```rust
// Chunk: docs/chunks/pane_scroll_isolation - Per-pane viewport configuration
/// Configures the renderer's viewport for rendering a specific pane.
///
/// This copies the scroll state from the tab's viewport and updates dimensions
/// to match the pane's actual size. Must be called before `update_glyph_buffer_*`
/// for each pane in multi-pane mode.
fn configure_viewport_for_pane(
    &mut self,
    tab_viewport: &Viewport,
    pane_content_height: f32,
    pane_width: f32,
) {
    // Copy scroll offset from tab (tab is authoritative)
    self.viewport.set_scroll_offset_px_unclamped(tab_viewport.scroll_offset_px());

    // Update visible lines for this pane's height
    // Note: We don't have row_count here, so use update_size_no_clamp or similar
    // The tab's viewport already has correct clamping for its content
    let line_height = self.viewport.line_height();
    let visible_lines = (pane_content_height / line_height).floor() as usize;
    self.viewport.set_visible_lines(visible_lines);

    // Update wrap width for this pane
    self.content_width_px = pane_width;
    self.viewport_width_px = pane_width + RAIL_WIDTH; // If needed
}
```

### Step 2: Add `set_visible_lines` method to Viewport/RowScroller

The current API only has `update_size(height_px, row_count)` which re-clamps.
For per-pane configuration, we need to set `visible_lines` directly since the
tab's viewport already holds the correctly-clamped scroll position.

Location: `crates/editor/src/row_scroller.rs`, `crates/editor/src/viewport.rs`

```rust
// In RowScroller:
/// Sets the visible row count directly, without re-clamping scroll offset.
///
/// Use this when copying viewport state from another source that has already
/// been properly clamped. The tab's viewport owns the scroll state and has
/// correct bounds for its content.
pub fn set_visible_rows(&mut self, rows: usize) {
    self.visible_rows = rows;
}

// In Viewport:
/// Sets the visible line count directly, without re-clamping scroll offset.
pub fn set_visible_lines(&mut self, lines: usize) {
    self.scroller.set_visible_rows(lines);
}
```

### Step 3: Update `render_pane` to configure viewport before drawing

Modify `render_pane()` to call `configure_viewport_for_pane()` before
`update_glyph_buffer_with_cursor_visible()`.

Location: `crates/editor/src/renderer.rs` (around line 1591-1618)

**Before** (current code):
```rust
// Set content offsets for this pane
self.set_content_x_offset(pane_rect.x);
self.set_content_y_offset(pane_rect.y + TAB_BAR_HEIGHT);

// Update glyph buffer with pane-local content width
let pane_content_width = pane_rect.width;
self.content_width_px = pane_content_width;

// ... cursor visibility ...

// Update glyph buffer from tab's buffer
if let Some(text_buffer) = tab.as_text_buffer() {
    // Uses self.viewport which has WRONG scroll offset
```

**After**:
```rust
// Set content offsets for this pane
self.set_content_x_offset(pane_rect.x);
self.set_content_y_offset(pane_rect.y + TAB_BAR_HEIGHT);

// Chunk: docs/chunks/pane_scroll_isolation - Configure viewport for this pane
// Copy tab's scroll state and update dimensions for pane size
let pane_content_height = pane_rect.height - TAB_BAR_HEIGHT;
self.configure_viewport_for_pane(&tab.viewport, pane_content_height, pane_rect.width);

// ... cursor visibility ...

// Update glyph buffer from tab's buffer (now using correctly configured viewport)
```

### Step 4: Remove stale viewport sync from drain_loop

The current `render_if_dirty()` syncs the renderer's viewport with the focused
tab once at the start. With per-pane configuration, this sync is redundant for
multi-pane mode and can cause the first pane to render incorrectly before
`render_pane()` configures it.

Location: `crates/editor/src/drain_loop.rs` (lines 255-259)

**Option A** (minimal change): Keep the sync for single-pane mode, skip for multi-pane.
This requires knowing pane count before rendering, which is awkward.

**Option B** (cleaner): Remove the sync entirely. Single-pane rendering should also
go through a similar path that copies the active tab's viewport. The single-pane
path in `render_with_editor()` can do this explicitly.

We'll choose **Option B** to ensure consistency. Update `render_with_editor()`'s
single-pane path to configure the viewport from the active tab before rendering.

### Step 5: Update single-pane rendering path for consistency

The single-pane rendering path (lines 1174-1191 in `renderer.rs`) should also
configure the viewport from the active tab, even though there's only one pane.

Location: `crates/editor/src/renderer.rs`

```rust
if pane_rects.len() <= 1 {
    // Single-pane case
    self.draw_tab_bar(&encoder, view, editor);

    // Chunk: docs/chunks/pane_scroll_isolation - Configure viewport for single pane
    if let Some(ws) = editor.active_workspace() {
        if let Some(tab) = ws.active_tab() {
            let content_height = view_height - TAB_BAR_HEIGHT;
            let content_width = view_width - RAIL_WIDTH;
            self.configure_viewport_for_pane(&tab.viewport, content_height, content_width);
        }
    }

    // ... rest of single-pane rendering
}
```

### Step 6: Handle agent terminal viewport

Agent terminals have a placeholder tab (`TabBuffer::AgentTerminal`) but use a
shared terminal buffer from the workspace. The agent terminal's scroll state
is managed differently - check if `workspace.agent_terminal()` has its own
viewport or if it shares the tab's viewport.

Location: `crates/editor/src/renderer.rs` in `render_pane()`

Review the agent terminal handling (lines 1606-1609) and ensure:
- The tab's viewport is still used for scroll offset (consistent API)
- Visible lines are computed from pane height
- The terminal buffer provides correct line counts for clamping

### Step 7: Add unit tests for viewport dimension configuration

Per TESTING_PHILOSOPHY.md, add tests for the testable logic:

Location: `crates/editor/src/viewport.rs` (in `#[cfg(test)]` module)

```rust
#[test]
fn test_set_visible_lines_preserves_scroll() {
    let mut vp = Viewport::new(20.0);
    vp.set_scroll_offset_px_unclamped(100.0); // Scroll to line 5
    vp.set_visible_lines(10);
    assert_eq!(vp.visible_lines(), 10);
    assert_eq!(vp.scroll_offset_px(), 100.0); // Preserved, not clamped
}

#[test]
fn test_configure_for_different_pane_heights() {
    let mut vp = Viewport::new(20.0);

    // Pane 1: 200px tall = 10 visible lines
    vp.update_size(200.0, 100);
    assert_eq!(vp.visible_lines(), 10);

    // Pane 2: 100px tall = 5 visible lines (different pane)
    vp.set_visible_lines(5);
    assert_eq!(vp.visible_lines(), 5);
    // Scroll offset unchanged
}
```

### Step 8: Verify wrap layout uses correct width

Ensure `WrapLayout` is created with the pane's width, not the global window width.
The current code creates `WrapLayout` inside `update_glyph_buffer_with_cursor_visible()`
using `self.content_width_px`. After Step 3, this should already be correct since
we update `content_width_px` before calling the method.

Location: `crates/editor/src/renderer.rs` (line 493)

Verify the order:
1. `configure_viewport_for_pane()` sets `self.content_width_px`
2. `update_glyph_buffer_with_cursor_visible()` creates `WrapLayout::new(self.content_width_px, ...)`

### Step 9: Run existing tests and manual verification

1. Run `cargo test` to ensure no regressions
2. Manual testing:
   - Open two panes (vertical split) with different files
   - Scroll one pane - verify other doesn't move
   - Type in one pane - verify other doesn't jump
   - Resize window - verify both panes scroll correctly
   - Test with soft-wrapped long lines in different pane widths
   - Collapse back to single pane - verify wrap width is correct

## Dependencies

No external dependencies. All required types and infrastructure exist:
- `Tab.viewport: Viewport` for per-tab scroll state
- `RowScroller` for scroll arithmetic
- `WrapLayout` for wrap coordinate mapping

## Risks and Open Questions

1. **Terminal scrollback interaction**: Agent terminal tabs use a shared
   `TerminalBuffer` from the workspace. Need to verify the tab's viewport is
   still the correct source of scroll state for terminal panes.

2. **Performance**: Copying viewport state per pane per frame adds minimal
   overhead (a few f32 assignments). If profiling shows issues, could cache
   whether viewport changed, but unlikely to be measurable.

3. **Scroll clamping during resize**: When a pane is resized, the tab's
   viewport may need re-clamping to the new bounds. This happens naturally
   when the user scrolls, but immediate resize might show out-of-bounds
   scroll position for one frame. Consider whether to clamp on pane resize.

4. **Multi-buffer tabs**: Currently each tab has one buffer. If future work
   adds split views within a tab, this approach would need extension. Out of
   scope for this chunk.

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
-->

_None yet - to be populated during implementation._