<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk implements the content tab bar — the horizontal tab strip at the top of the content area showing the active workspace's open tabs. This is the second level of the two-level tab hierarchy established by the `workspace_model` chunk.

**Architecture Strategy:**

Following the "Humble View Architecture" from `TESTING_PHILOSOPHY.md`:
- **Model**: The `Tab` and `Workspace` data structures already exist (from `workspace_model`). We'll add tab navigation methods and unread tracking.
- **Update**: Keyboard handlers in `EditorState` for tab cycling (Cmd+Shift+]/[) and tab close (Cmd+W).
- **View**: A new `TabBarGlyphBuffer` for rendering the tab strip, following the pattern established by `LeftRailGlyphBuffer` and `SelectorGlyphBuffer`.

**Rendering Pattern:**
- Create `tab_bar.rs` with layout calculation (`calculate_tab_bar_geometry`) and `TabBarGlyphBuffer` for GPU buffer management
- Tab bar renders horizontally at top of content area (below window title, right of left rail)
- Each tab shows: label, optional dirty/unread indicator, close button hit area
- Active tab is visually highlighted
- Horizontal scrolling via `view_offset` when tabs overflow

**Input Handling:**
- Extend `EditorState::handle_key()` with Cmd+Shift+]/[ for tab cycling and Cmd+W for close
- Extend `EditorState::handle_mouse()` with tab bar click detection and close button handling
- Tab bar clicks above content area, right of left rail

**Integration Points:**
- Renderer calls `draw_tab_bar()` between left rail and content
- Content area Y offset shifts down by `TAB_BAR_HEIGHT` when tab bar is visible
- Tab unread state set when terminal tabs receive output while not active (placeholder for terminal_emulator chunk)

## Sequence

### Step 1: Tab bar layout constants and geometry types

Create `crates/editor/src/tab_bar.rs` with:

- Layout constants: `TAB_BAR_HEIGHT`, `TAB_MIN_WIDTH`, `TAB_MAX_WIDTH`, `TAB_PADDING`, `CLOSE_BUTTON_SIZE`, etc.
- Color constants following Catppuccin Mocha theme (consistent with `left_rail.rs`)
- `TabRect` struct with `x`, `y`, `width`, `height`, `close_button_rect`, and `contains()` method
- `TabBarGeometry` struct holding `tab_rects`, `view_offset`, `total_width`
- `calculate_tab_bar_geometry()` pure function that takes view width, RAIL_WIDTH offset, and workspace tabs to compute layout

This step produces no tests yet (pure struct/constant definitions).

Location: `crates/editor/src/tab_bar.rs`

### Step 2: Tests for tab bar geometry calculation

Write unit tests for `calculate_tab_bar_geometry()`:

- Test with 0 tabs returns empty geometry
- Test with 1 tab returns single TabRect
- Test with 5 tabs returns 5 TabRects in order
- Test that tab rects don't overlap and are laid out horizontally
- Test `TabRect::contains()` for hit testing
- Test that tab labels are truncated to MAX_WIDTH
- Test view_offset scrolls when tabs overflow available width

These tests follow TDD: write failing tests first, then implement `calculate_tab_bar_geometry()` to make them pass.

Location: `crates/editor/src/tab_bar.rs` (in `#[cfg(test)] mod tests`)

### Step 3: TabBarGlyphBuffer for GPU rendering

Implement `TabBarGlyphBuffer` following the `LeftRailGlyphBuffer` pattern:

- Store `vertex_buffer`, `index_buffer`, `index_count`, `layout` (GlyphLayout)
- Track QuadRanges: `background_range`, `tab_background_range`, `active_tab_range`, `dirty_indicator_range`, `close_button_range`, `label_range`
- Implement `update()` method that builds vertex data:
  1. Tab bar background strip
  2. Inactive tab backgrounds
  3. Active tab highlight
  4. Dirty/unread indicators (dots)
  5. Close button icons (×)
  6. Tab labels (truncated)
- Use `atlas.solid_glyph()` for rectangles, `atlas.get_glyph()` for text

Location: `crates/editor/src/tab_bar.rs`

### Step 4: Renderer integration for tab bar

Extend `Renderer` to render the tab bar:

- Add `tab_bar_buffer: Option<TabBarGlyphBuffer>` field
- Add `set_content_y_offset()` method to shift content area down by TAB_BAR_HEIGHT
- In `draw_tab_bar()`:
  - Calculate tab bar geometry from workspace tabs
  - Update `TabBarGlyphBuffer`
  - Draw quads in order: background → tab backgrounds → active highlight → indicators → close buttons → labels
- Call `draw_tab_bar()` in `render_with_editor()` after `draw_left_rail()` and before content

Update `GlyphBuffer` to support Y offset similar to X offset (already has `set_x_offset`, add `set_y_offset`).

Location: `crates/editor/src/renderer.rs`, `crates/editor/src/glyph_buffer.rs`

### Step 5: Tab bar click handling in EditorState

Extend `EditorState::handle_mouse()`:

- After checking left rail clicks, check if click is in tab bar region (y < TAB_BAR_HEIGHT + header offset, x >= RAIL_WIDTH)
- Calculate tab bar geometry to determine which tab was clicked
- If close button was clicked:
  - Check if tab is dirty, if so skip close (confirmation is future work)
  - Otherwise close the tab via `workspace.close_tab(idx)`
- If tab body was clicked:
  - Switch to that tab via `workspace.switch_tab(idx)`
- Mark `DirtyRegion::FullViewport` on tab changes

Add mouse coordinate transformation for content area clicks (subtract TAB_BAR_HEIGHT from y).

Location: `crates/editor/src/editor_state.rs`

### Step 6: Keyboard shortcuts for tab navigation

Extend `EditorState::handle_key()`:

- **Cmd+Shift+]**: Cycle to next tab (`workspace.cycle_tab_forward()`)
- **Cmd+Shift+[**: Cycle to previous tab (`workspace.cycle_tab_backward()`)
- **Cmd+W**: Close active tab (without Shift, differs from Cmd+Shift+W which closes workspace)
  - Check if tab is dirty; if so, this chunk does not implement confirmation (out of scope)
  - Close tab via `workspace.close_tab(active_tab)`
  - If last tab in workspace, create a new empty tab (workspace must have at least one tab)
- **Cmd+T**: Create new empty tab in current workspace

Add `Workspace::cycle_tab_forward()` and `Workspace::cycle_tab_backward()` methods.

Location: `crates/editor/src/editor_state.rs`, `crates/editor/src/workspace.rs`

### Step 7: Tests for tab cycling and close behavior

Write tests for the new keyboard shortcuts:

- Test Cmd+Shift+] cycles forward through tabs (wraps around)
- Test Cmd+Shift+[ cycles backward through tabs (wraps around)
- Test Cmd+W closes active tab
- Test Cmd+W on dirty tab does not close (placeholder for confirmation)
- Test Cmd+W on last tab creates new empty tab
- Test Cmd+T creates new tab and switches to it
- Test workspace.cycle_tab_forward/backward methods

Location: `crates/editor/src/editor_state.rs` and `crates/editor/src/workspace.rs` (test modules)

### Step 8: Unread badge support for terminal tabs

Add unread tracking infrastructure to `Tab`:

- `Tab.unread: bool` already exists
- Add `Tab::mark_unread()` method that sets `unread = true`
- Add `Tab::clear_unread()` method that sets `unread = false`
- When switching tabs, call `clear_unread()` on the newly active tab
- Render unread indicator (colored dot) in tab bar for tabs with `unread == true`

Note: Setting `unread = true` when terminal output arrives is deferred to the `terminal_emulator` chunk. This step only sets up the infrastructure.

Location: `crates/editor/src/workspace.rs`, `crates/editor/src/tab_bar.rs`

### Step 9: Tab label derivation

Implement intelligent tab label derivation:

- File tabs: Show filename (e.g., "main.rs")
- If multiple tabs share the same filename, disambiguate with parent directory (e.g., "src/main.rs" vs "tests/main.rs")
- Terminal tabs: Show "Terminal" (or shell name when available)
- Labels truncated to fit `TAB_MAX_WIDTH` with ellipsis

Add `Tab::display_label()` method that computes the appropriate label.
Add `Workspace::compute_display_labels()` that handles disambiguation.

Location: `crates/editor/src/workspace.rs`

### Step 10: Tab bar overflow scrolling

Handle tab bars with too many tabs to fit:

- Track `view_offset` (integer tab index) in geometry calculation
- When active tab is not fully visible, scroll to show it
- Keyboard cycling automatically scrolls active tab into view
- Tab bar shows scroll indicators (arrows or fade) when clipped

This is a stretch goal for graceful handling of 10-20 tabs. The core functionality works without it.

Location: `crates/editor/src/tab_bar.rs`

## Dependencies

- **workspace_model** (ACTIVE): This chunk builds directly on the workspace model. The `Editor`, `Workspace`, `Tab`, `TabKind`, and `WorkspaceStatus` types are already implemented. The left rail rendering pattern is established.

- **buffer_view_trait** (ACTIVE): The `BufferView` trait and `TabBuffer` enum enable heterogeneous tab content. Terminal tabs will be added in a future chunk.

## Risks and Open Questions

1. **Dirty tab close confirmation**: The GOAL.md mentions "prompts for confirmation if the tab's buffer is dirty." This chunk implements the check (`if tab.dirty`) but does NOT implement a confirmation dialog UI. That would require a modal dialog pattern not yet established. For now, dirty tabs simply don't close on Cmd+W.

2. **Cmd+O file picker integration**: The GOAL.md mentions "Cmd+O to open a file tab (file picker)". This reuses the existing file picker (Cmd+P). We may need to clarify whether Cmd+O should be a distinct command or an alias. For this chunk, treat Cmd+O as an alias for Cmd+P that opens in the current workspace.

3. **Terminal tab creation**: "Cmd+T to open a new terminal tab" depends on the terminal_emulator chunk. For this chunk, Cmd+T creates a new empty file tab. The terminal_emulator chunk will change this behavior.

4. **Tab bar Y offset and coordinate transformation**: Adding a tab bar at the top of the content area requires adjusting Y coordinates for:
   - Content rendering (shift down)
   - Mouse click handling (subtract TAB_BAR_HEIGHT)
   - Viewport calculations (available_height -= TAB_BAR_HEIGHT)

   This is the most complex integration point and may require iteration.

5. **Middle-click to close**: The GOAL.md mentions "Middle-click or close button on tab to close it." Middle-click detection depends on mouse button tracking not yet implemented. Defer to a follow-up if winit doesn't expose middle-click in the current input handling.

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