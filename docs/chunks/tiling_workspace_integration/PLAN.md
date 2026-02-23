<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk integrates the existing binary pane layout tree (from `tiling_tree_model` and `tiling_tab_movement`) into the `Workspace` and `EditorState` models. The key insight is that the `Pane` struct already mirrors the tab-management API of `Workspace` (add_tab, close_tab, switch_tab, etc.), so integration primarily involves:

1. **Replacing `Workspace.tabs` / `Workspace.active_tab` with `pane_root` / `active_pane_id`** — the flat tab list becomes a tree with (initially) a single leaf pane
2. **Forwarding tab operations to the active pane** — `Workspace` becomes a thin delegation layer
3. **Updating `EditorState` delegate methods** — resolve through `active_workspace().active_pane().active_tab()` instead of `active_workspace().active_tab()`
4. **Refactoring mouse coordinate handling** — centralize the NSView y-flip at the entry point (`handle_mouse`) and dispatch pane-local coordinates to handlers

The implementation preserves backward compatibility: with a single pane, the editor behaves identically to before. The pane tree only becomes visible when splits are created (future chunk).

### Coordinate Handling Strategy

The current codebase has accumulated multiple coordinate transform patterns:
- Left rail flips y and uses `calculate_left_rail_geometry` rects
- Tab bar checks `mouse_y >= (view_height - TAB_BAR_HEIGHT)` (NSView coords)
- Buffer content subtracts `RAIL_WIDTH` from x and passes `content_height` for y-flip
- Terminal path does similar transforms with scroll offset compensation

This chunk introduces a **single y-flip at entry** pattern:
1. `handle_mouse` flips y immediately: `screen_y = view_height - nsview_y`
2. All downstream code works in screen space (y=0 at top)
3. Hit-testing uses `PaneRect` values (computed by `calculate_pane_rects`) in screen space
4. Pane-local coordinates are computed by subtracting pane origin at dispatch point

This consolidation prevents future coordinate bugs as the multi-pane system grows.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem for scroll state management within panes. No changes to subsystem internals required. Each `Pane` will eventually have its own `Viewport` (currently tabs own viewports, which is unchanged).

## Sequence

### Step 1: Add pane tree fields to Workspace

Extend `Workspace` in `crates/editor/src/workspace.rs`:

```rust
use crate::pane_layout::{PaneId, PaneLayoutNode, Pane, gen_pane_id, calculate_pane_rects, PaneRect};

pub struct Workspace {
    // Existing fields (keep for now, migrate in step 2):
    // pub tabs: Vec<Tab>,
    // pub active_tab: usize,

    // New pane tree fields:
    pub pane_root: PaneLayoutNode,
    pub active_pane_id: PaneId,
    next_pane_id: u64,
    // ... other existing fields unchanged
}
```

Add `Workspace::gen_pane_id(&mut self) -> PaneId` method.

Location: `crates/editor/src/workspace.rs`

### Step 2: Migrate Workspace::new() and Workspace::with_empty_tab()

Update constructors to create a single-pane tree instead of a flat tab list:

```rust
pub fn new(id: WorkspaceId, label: String, root_path: PathBuf) -> Self {
    let mut next_pane_id = 0u64;
    let pane_id = gen_pane_id(&mut next_pane_id);
    let pane = Pane::new(pane_id, id); // empty pane

    Self {
        id,
        label,
        root_path,
        pane_root: PaneLayoutNode::single_pane(pane),
        active_pane_id: pane_id,
        next_pane_id,
        status: WorkspaceStatus::Idle,
        agent: None,
        tab_bar_view_offset: 0.0, // Now per-pane, but keep for compat
    }
}

pub fn with_empty_tab(id: WorkspaceId, tab_id: TabId, label: String, root_path: PathBuf, line_height: f32) -> Self {
    let mut ws = Self::new(id, label, root_path);
    let tab = Tab::empty_file(tab_id, line_height);
    // Add to the active pane
    if let Some(pane) = ws.pane_root.get_pane_mut(ws.active_pane_id) {
        pane.add_tab(tab);
    }
    ws
}
```

Remove `tabs` and `active_tab` fields from the struct definition.

Location: `crates/editor/src/workspace.rs`

### Step 3: Implement active_pane() and active_pane_mut() accessors

Add pane resolution methods to `Workspace`:

```rust
pub fn active_pane(&self) -> Option<&Pane> {
    self.pane_root.get_pane(self.active_pane_id)
}

pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
    self.pane_root.get_pane_mut(self.active_pane_id)
}
```

Location: `crates/editor/src/workspace.rs`

### Step 4: Update Workspace tab operations to delegate to active pane

Reimplement tab methods as thin delegation:

```rust
pub fn add_tab(&mut self, tab: Tab) {
    if let Some(pane) = self.active_pane_mut() {
        pane.add_tab(tab);
    }
}

pub fn close_tab(&mut self, index: usize) -> Option<Tab> {
    self.active_pane_mut()?.close_tab(index)
}

pub fn active_tab(&self) -> Option<&Tab> {
    self.active_pane()?.active_tab()
}

pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
    self.active_pane_mut()?.active_tab_mut()
}

pub fn switch_tab(&mut self, index: usize) {
    if let Some(pane) = self.active_pane_mut() {
        pane.switch_tab(index);
    }
}

pub fn tab_count(&self) -> usize {
    // Sum across all panes
    self.pane_root.all_panes().iter().map(|p| p.tab_count()).sum()
}
```

Location: `crates/editor/src/workspace.rs`

### Step 5: Add all_panes() and all_panes_mut() to Workspace

Add convenience methods for iteration:

```rust
pub fn all_panes(&self) -> Vec<&Pane> {
    self.pane_root.all_panes()
}

pub fn all_panes_mut(&mut self) -> Vec<&mut Pane> {
    self.pane_root.all_panes_mut()
}
```

Location: `crates/editor/src/workspace.rs`

### Step 6: Update poll_standalone_terminals to iterate all panes

Modify the terminal polling to use the pane tree:

```rust
pub fn poll_standalone_terminals(&mut self) -> bool {
    use lite_edit_buffer::BufferView;

    let mut had_events = false;
    for pane in self.pane_root.all_panes_mut() {
        for tab in &mut pane.tabs {
            if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
                // ... existing auto-follow logic unchanged
            }
        }
    }
    had_events
}
```

Location: `crates/editor/src/workspace.rs`

### Step 7: Update EditorState delegate methods

Update the delegation chain in `crates/editor/src/editor_state.rs` to resolve through panes:

```rust
pub fn buffer(&self) -> &TextBuffer {
    self.editor
        .active_workspace()
        .expect("no active workspace")
        .active_pane()
        .expect("no active pane")
        .active_tab()
        .expect("no active tab")
        .as_text_buffer()
        .expect("active tab is not a file tab")
}

pub fn buffer_mut(&mut self) -> &mut TextBuffer {
    self.editor
        .active_workspace_mut()
        .expect("no active workspace")
        .active_pane_mut()
        .expect("no active pane")
        .active_tab_mut()
        .expect("no active tab")
        .as_text_buffer_mut()
        .expect("active tab is not a file tab")
}

// Similar updates for try_buffer, try_buffer_mut, viewport, viewport_mut, associated_file
```

Location: `crates/editor/src/editor_state.rs`

### Step 8: Update new_tab() and new_terminal_tab() in EditorState

These methods now add tabs to the active pane:

```rust
pub fn new_tab(&mut self) {
    let tab_id = self.editor.gen_tab_id();
    let line_height = self.editor.line_height();
    let tab = Tab::empty_file(tab_id, line_height);

    if let Some(ws) = self.editor.active_workspace_mut() {
        ws.add_tab(tab); // Delegates to active pane
    }

    self.sync_active_tab_viewport();
    self.dirty_region.merge(DirtyRegion::FullViewport);
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 9: Update close_active_tab() to handle pane collapse

When closing the last tab in a pane with splits, the pane should remain (or be cleaned up by cleanup_empty_panes). For single-pane workspace, create a new empty tab:

```rust
pub fn close_active_tab(&mut self) {
    if let Some(ws) = self.editor.active_workspace_mut() {
        let pane_count = ws.pane_root.pane_count();
        let tab_count_in_pane = ws.active_pane().map(|p| p.tab_count()).unwrap_or(0);

        if tab_count_in_pane > 1 || pane_count > 1 {
            // Close the tab
            ws.close_tab(ws.active_pane().map(|p| p.active_tab).unwrap_or(0));

            // If pane is now empty and there are multiple panes, cleanup
            if pane_count > 1 {
                use crate::pane_layout::{cleanup_empty_panes, CleanupResult};
                if let CleanupResult::Collapsed = cleanup_empty_panes(&mut ws.pane_root) {
                    // Active pane may have changed; pick a valid one
                    let panes = ws.pane_root.all_panes();
                    if !panes.is_empty() {
                        ws.active_pane_id = panes[0].id;
                    }
                }
            }
        } else {
            // Single tab in single pane: replace with empty tab
            ws.close_tab(0);
            let tab_id = self.editor.gen_tab_id();
            let line_height = self.editor.line_height();
            ws.add_tab(Tab::empty_file(tab_id, line_height));
        }
    }

    self.sync_active_tab_viewport();
    self.dirty_region.merge(DirtyRegion::FullViewport);
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 10: Update sync_active_tab_viewport()

Resolve through the pane tree:

```rust
fn sync_active_tab_viewport(&mut self) {
    // Same logic but with pane chain:
    let line_count = match self.editor.active_workspace()
        .and_then(|ws| ws.active_pane())
        .and_then(|pane| pane.active_tab())
        .and_then(|tab| tab.as_text_buffer())
    {
        Some(buf) => buf.line_count(),
        None => return,
    };

    // ... rest of method unchanged
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 11: Refactor handle_mouse() entry point for y-flip

Implement the "flip once at entry" pattern. This is the main coordinate refactoring:

```rust
pub fn handle_mouse(&mut self, event: MouseEvent) {
    use crate::input::MouseEventKind;

    // Step 1: Flip y-coordinate ONCE at entry
    // NSView uses bottom-left origin (y=0 at bottom)
    // We convert to screen space (y=0 at top) for all downstream code
    let (nsview_x, nsview_y) = event.position;
    let screen_x = nsview_x;
    let screen_y = (self.view_height as f64) - nsview_y;

    // Create screen-space event
    let screen_event = MouseEvent {
        kind: event.kind,
        position: (screen_x, screen_y),
        modifiers: event.modifiers,
        click_count: event.click_count,
    };

    // Step 2: Hit-test against UI regions in screen space

    // Check left rail (x < RAIL_WIDTH)
    if screen_x < RAIL_WIDTH as f64 {
        if let MouseEventKind::Down = screen_event.kind {
            let geometry = calculate_left_rail_geometry(self.view_height, self.editor.workspace_count());
            // geometry.tile_rects are already in screen space
            for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
                if tile_rect.contains(screen_x as f32, screen_y as f32) {
                    self.switch_workspace(idx);
                    return;
                }
            }
        }
        return;
    }

    // Check tab bar (y < TAB_BAR_HEIGHT in screen space)
    if screen_y < TAB_BAR_HEIGHT as f64 {
        if let MouseEventKind::Down = screen_event.kind {
            self.handle_tab_bar_click(screen_x as f32, screen_y as f32);
        }
        return;
    }

    // Step 3: Route to appropriate handler with screen-space coordinates
    match self.focus {
        EditorFocus::Selector => {
            self.handle_mouse_selector(screen_event);
        }
        EditorFocus::Buffer | EditorFocus::FindInFile => {
            self.handle_mouse_buffer(screen_event);
        }
    }
}
```

Note: This changes the coordinate convention. All handlers now receive screen-space coordinates where y=0 is at the top.

Location: `crates/editor/src/editor_state.rs`

### Step 12: Update handle_mouse_buffer() for screen-space input

Adjust the buffer/terminal mouse handler to work with screen-space coordinates:

```rust
fn handle_mouse_buffer(&mut self, event: MouseEvent) {
    // event.position is now in screen space (y=0 at top)
    let (screen_x, screen_y) = event.position;

    // Content area starts after rail and below tab bar
    // In screen space: content_x = screen_x - RAIL_WIDTH
    //                 content_y = screen_y - TAB_BAR_HEIGHT
    let content_x = screen_x - RAIL_WIDTH as f64;
    let content_y = screen_y - TAB_BAR_HEIGHT as f64;

    // Bounds check
    if content_x < 0.0 || content_y < 0.0 {
        return;
    }

    // For multi-pane: hit-test pane_rects to find which pane was clicked
    // For now (single pane), the entire content area is the active pane

    let pane_local_event = MouseEvent {
        kind: event.kind,
        position: (content_x, content_y),
        modifiers: event.modifiers,
        click_count: event.click_count,
    };

    // ... rest of handler adjusted for pane-local coords
    // pixel_to_buffer_position now receives content_y directly (no flip needed)
}
```

This requires updating `pixel_to_buffer_position` in `buffer_target.rs` to accept y coordinates that are already in screen space (top-down). Currently it does a y-flip internally; that flip will be removed.

Location: `crates/editor/src/editor_state.rs`, `crates/editor/src/buffer_target.rs`

### Step 13: Update pixel_to_buffer_position to expect screen-space y

Remove the internal y-flip from `buffer_target.rs`:

```rust
fn pixel_to_buffer_position(
    x: f64,
    y: f64,  // Now expected to be screen-space (y=0 at top of content)
    // ... other params
) -> Position {
    // Remove: let flipped_y = content_height as f64 - y;
    // Just use y directly, accounting for scroll:
    let screen_row = ((y + scroll_fraction_px as f64) / line_height as f64) as usize;
    // ... rest of function
}
```

Location: `crates/editor/src/buffer_target.rs`

### Step 14: Update handle_tab_bar_click to use screen-space coords

The tab bar click handler now receives screen-space coordinates:

```rust
fn handle_tab_bar_click(&mut self, screen_x: f32, screen_y: f32) {
    // screen_y is already relative to top of screen
    // Tab bar occupies y=[0, TAB_BAR_HEIGHT)
    // tab_bar_local_y = screen_y (already in tab bar space since we checked y < TAB_BAR_HEIGHT)

    let tab_bar_x = screen_x - RAIL_WIDTH; // Content starts after rail
    // ... rest of hit-testing against tab rects
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 15: Update handle_mouse_selector for screen-space coords

The selector handler now receives screen-space coordinates directly:

```rust
fn handle_mouse_selector(&mut self, event: MouseEvent) {
    // event.position is screen-space (y=0 at top)
    // No flip needed - overlay geometry is already screen-space

    let selector = match self.active_selector.as_mut() {
        Some(s) => s,
        None => return,
    };

    // ... geometry calculation unchanged

    // Forward directly (no flip)
    let outcome = selector.handle_mouse(
        event.position,
        event.kind,
        geometry.item_height as f64,
        geometry.list_origin_y as f64,
    );

    // ... rest unchanged
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 16: Update Agent tab handling

Ensure agent terminals work with the pane tree. The `AgentTerminal` placeholder pattern is unchanged - agent tabs are still identified by `TabBuffer::AgentTerminal` and the actual terminal lives in `Workspace.agent`. Update access patterns to go through panes:

```rust
// In Editor::active_buffer_view:
pub fn active_buffer_view(&self) -> Option<&dyn BufferView> {
    let workspace = self.active_workspace()?;
    let pane = workspace.active_pane()?;
    let tab = pane.active_tab()?;

    if tab.buffer.is_agent_terminal() {
        workspace.agent_terminal().map(|t| t as &dyn BufferView)
    } else {
        Some(tab.buffer())
    }
}
```

Location: `crates/editor/src/workspace.rs`

### Step 17: Update Workspace tests

Update existing workspace tests to work with the pane-based model. Key changes:
- `ws.tabs[i]` → `ws.active_pane().unwrap().tabs[i]`
- Tab count assertions may need adjustment for multi-pane semantics

Location: `crates/editor/src/workspace.rs` (test module)

### Step 18: Add integration tests for backward compatibility

Add tests verifying single-pane behavior matches old flat model:
- Tab operations work through the pane
- Terminal polling iterates correctly
- Mouse clicks land on correct positions
- Tab bar rendering is unchanged

Location: `crates/editor/src/workspace.rs` or `crates/editor/tests/`

### Step 19: Update code_paths in GOAL.md

Add the files touched by this implementation to the chunk's code_paths frontmatter:
- `crates/editor/src/workspace.rs`
- `crates/editor/src/editor_state.rs`
- `crates/editor/src/buffer_target.rs`
- `crates/editor/src/pane_layout.rs`

Location: `docs/chunks/tiling_workspace_integration/GOAL.md`

## Dependencies

- **tiling_tree_model** (ACTIVE): Provides `PaneLayoutNode`, `Pane`, `PaneRect`, `calculate_pane_rects`
- **tiling_tab_movement** (ACTIVE): Provides `move_tab`, `cleanup_empty_panes`, `MoveResult`, `CleanupResult`

Both dependency chunks are already implemented (status: ACTIVE), so all required types and functions are available.

## Risks and Open Questions

1. **Backward compatibility breakage**: The coordinate handling refactor touches many mouse event paths. Risk: existing behavior may regress if any path still expects NSView coordinates after the flip. Mitigation: comprehensive mouse click tests before and after.

2. **Tab bar scroll offset migration**: `Workspace.tab_bar_view_offset` is currently per-workspace, but with panes it should be per-pane (which `Pane` already has). Need to decide: keep the workspace-level field for single-pane compat, or remove it? Plan: remove it since `Pane.tab_bar_view_offset` already exists.

3. **Terminal mouse handling complexity**: The terminal path has extra scroll compensation logic. Verify this works correctly with the new coordinate convention.

4. **Agent terminal location**: Agent terminals have a special relationship with workspaces (agent handle on workspace, terminal accessed via `workspace.agent_terminal()`). This doesn't change fundamentally - agent tabs can exist in any pane.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
