<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause is a rendering path divergence: in single-pane mode, the glyph buffer is updated *before* the Metal drawable is acquired (lines 608-641 in `render_with_editor`), while in multi-pane mode, each pane's glyph buffer is updated inside `render_pane()` (which happens *after* the drawable is acquired).

When a terminal tab is spawned in single-pane mode:
1. The early glyph buffer update runs with the terminal's *current* content (likely empty or minimal)
2. PTY wakeup signals arrive via the event channel, triggering `poll_agents()` which returns `DirtyRegion::FullViewport`
3. `render_if_dirty()` calls `render_with_editor()` again
4. The early glyph buffer update runs again, but uses `tab.buffer()` which returns a *stale* reference to the terminal content

The multi-pane path works correctly because `render_pane()` updates the glyph buffer *during* the render pass, always using the current tab state.

**Fix Strategy**: Mirror the multi-pane rendering behavior in the single-pane path. Move the glyph buffer update for single-pane mode to occur *after* the Metal drawable is acquired and *within* the content rendering block, just like `render_pane()` does. This ensures the terminal content is read at the correct time during the render pass.

This follows the renderer subsystem's layering contract (docs/subsystems/renderer): rendering phases should access content at a consistent point in the frame.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem's glyph buffer update pattern. The multi-pane code path in `render_pane()` demonstrates the correct pattern: configure viewport, then update glyph buffer, then render text — all within the render pass. We are aligning single-pane mode to follow this same pattern.

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem's `configure_viewport_for_pane()` method. No changes to viewport scroll logic are needed.

## Sequence

### Step 1: Analyze the current rendering flow

Before making changes, trace the exact code path:

1. `render_with_editor()` (mod.rs:589) is called
2. Lines 608-641: Early glyph buffer update for single-pane mode
   - Gets the active tab
   - Calls `configure_viewport_for_pane()` with tab's viewport
   - Calls `update_glyph_buffer()` with tab's buffer
3. Lines 642+: Acquires Metal drawable, creates encoder
4. Lines 745-763: Single-pane rendering block
   - Draws tab bar
   - Sets scissor rect
   - Calls `render_text()` using the *already updated* glyph buffer

The problem: glyph buffer is updated in step 2 using terminal content that may have changed by step 4.

### Step 2: Remove early glyph buffer update for single-pane mode

**Location**: `crates/editor/src/renderer/mod.rs`, lines 608-641

Remove the early `if ws.pane_root.pane_count() <= 1` block that calls `configure_viewport_for_pane()` and `update_glyph_buffer()`. This code was added by `pane_mirror_restore` to prevent cache contamination in multi-pane mode, but the single-pane path should not use early buffer updates at all.

After this change, the early block only needs to remain for multi-pane mode (which skips this block anyway since `pane_count() > 1`).

### Step 3: Add glyph buffer update inside single-pane content rendering

**Location**: `crates/editor/src/renderer/mod.rs`, single-pane rendering block (lines 745-778)

Inside the `if pane_rects.len() <= 1` block, after drawing the tab bar and setting the scissor rect, add glyph buffer update logic similar to what `render_pane()` does:

```rust
// Single-pane case: render as before (global tab bar, no dividers)
self.draw_tab_bar(&encoder, view, editor);

// Clip buffer content to area below tab bar
let content_scissor = buffer_content_scissor_rect(TAB_BAR_HEIGHT, view_width, view_height);
encoder.setScissorRect(content_scissor);

// Check for welcome screen or normal buffer rendering
if editor.should_show_welcome_screen() {
    let scroll = editor.welcome_scroll_offset_px();
    self.draw_welcome_screen(&encoder, view, scroll);
} else {
    // Chunk: docs/chunks/terminal_single_pane_refresh - Update glyph buffer during render pass
    // For single-pane mode, update glyph buffer here (during the render pass) rather than
    // at the start of render_with_editor. This ensures terminal content is read at the
    // correct time, matching the multi-pane render_pane() behavior.
    if let Some(ws) = editor.active_workspace() {
        if let Some(tab) = ws.active_tab() {
            let content_height = view_height - TAB_BAR_HEIGHT;
            let content_width = view_width - RAIL_WIDTH;
            self.configure_viewport_for_pane(&tab.viewport, content_height, content_width);

            if tab.is_agent_tab() {
                if let Some(terminal) = ws.agent_terminal() {
                    self.update_glyph_buffer(terminal);
                }
            } else if let Some(text_buffer) = tab.as_text_buffer() {
                let highlighted_view = HighlightedBufferView::new(
                    text_buffer,
                    tab.highlighter(),
                );
                self.update_glyph_buffer(&highlighted_view);
            } else {
                // Terminal or other buffer type
                self.update_glyph_buffer(tab.buffer());
            }
        }
    }

    // Render editor text content
    if self.glyph_buffer.index_count() > 0 {
        self.render_text(&encoder, view);
    }
}
```

### Step 4: Ensure styled line cache is cleared appropriately

The styled line cache may contain stale data from a previous render. In `render_pane()`, `clear_styled_line_cache()` is called between pane renders. For single-pane mode, this isn't needed *between* panes (there's only one), but we should verify the cache invalidation logic in `render_if_dirty()` handles terminal content updates correctly.

**Location**: `crates/editor/src/drain_loop.rs`, `render_if_dirty()` method

Review the existing logic that calls `renderer.invalidate_styled_lines(&dirty_lines)`. For terminal tabs, verify that `dirty_lines` is populated correctly when PTY output arrives. If terminal tabs don't track dirty lines the same way text buffers do, we may need to call `clear_styled_line_cache()` for terminal tabs.

### Step 5: Write tests

**Location**: `crates/editor/src/editor_state.rs` (test module)

Per the testing philosophy, we can't directly test GPU rendering, but we can test that:
1. Terminal tabs produce dirty regions when PTY output arrives (existing test)
2. The terminal buffer has content after polling (existing test)

Add a test that specifically validates the single-pane rendering scenario:
- Create a single-pane workspace
- Spawn a terminal tab (Cmd+Shift+T simulation)
- Poll for PTY events
- Verify dirty region is FullViewport
- Verify terminal has non-empty content after polling

This test validates the preconditions for successful rendering; the actual rendering behavior must be verified visually.

### Step 6: Manual visual verification

1. Launch the editor
2. Ensure a single-pane workspace (no splits)
3. Press Cmd+Shift+T to spawn a terminal tab
4. Verify the shell prompt renders immediately (within one frame of PTY output)
5. Type commands, verify output appears correctly
6. Split into multiple panes (Cmd+D)
7. Create another terminal tab
8. Verify both terminals render correctly
9. Close one pane, verify the remaining terminal still updates correctly

## Dependencies

None. This chunk builds on existing infrastructure from:
- `terminal_tab_initial_render` (ACTIVE) - Fixed blank screens from visible_rows=0
- `terminal_viewport_init` (ACTIVE) - Fixed scroll_to_bottom computing wrong offsets
- `pane_mirror_restore` (ACTIVE) - Added early glyph buffer update logic (which this chunk modifies)

## Risks and Open Questions

1. **Styled line cache contamination**: Moving the glyph buffer update inside the render pass changes when the cache is populated. Need to verify cache invalidation still works correctly for terminal tabs.

2. **Performance regression**: The early glyph buffer update was potentially done to avoid work during the Metal drawable wait. However, since the multi-pane path already does this without issues, this should be acceptable.

3. **Welcome screen edge case**: The welcome screen rendering path uses different logic. Need to verify this still works correctly when there's an empty file buffer in single-pane mode.

4. **Agent terminal special case**: The `is_agent_tab()` check and `ws.agent_terminal()` lookup is a workaround for agent terminal tabs. Need to preserve this behavior.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->