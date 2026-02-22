---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/left_rail.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
  - crates/editor/src/renderer.rs
  - crates/editor/src/glyph_buffer.rs
  - crates/editor/src/context.rs
  - crates/editor/src/selector_overlay.rs
code_references:
  - ref: crates/editor/src/workspace.rs#WorkspaceId
    implements: "Unique identifier type for workspaces"
  - ref: crates/editor/src/workspace.rs#TabId
    implements: "Unique identifier type for tabs"
  - ref: crates/editor/src/workspace.rs#WorkspaceStatus
    implements: "Workspace status enum for agent lifecycle indicators (Idle, Running, NeedsInput, Stale, Completed, Errored)"
  - ref: crates/editor/src/workspace.rs#TabKind
    implements: "Tab content type enum (File, Terminal, AgentOutput, Diff)"
  - ref: crates/editor/src/workspace.rs#TabBuffer
    implements: "Enum wrapper for buffer types to avoid trait object downcasting"
  - ref: crates/editor/src/workspace.rs#Tab
    implements: "Tab struct owning buffer, viewport, and metadata (dirty, unread, associated_file)"
  - ref: crates/editor/src/workspace.rs#Tab::new_file
    implements: "Factory for file-backed tabs"
  - ref: crates/editor/src/workspace.rs#Tab::buffer_and_viewport_mut
    implements: "Borrow-checker-friendly method to access both buffer and viewport mutably"
  - ref: crates/editor/src/workspace.rs#Workspace
    implements: "Workspace struct containing tabs, active_tab index, status, and root_path"
  - ref: crates/editor/src/workspace.rs#Workspace::add_tab
    implements: "Add tab and switch to it"
  - ref: crates/editor/src/workspace.rs#Workspace::close_tab
    implements: "Close tab with active_tab index adjustment"
  - ref: crates/editor/src/workspace.rs#Editor
    implements: "Top-level Editor containing workspaces and ID generation"
  - ref: crates/editor/src/workspace.rs#Editor::new_workspace
    implements: "Create new workspace with empty tab and switch to it"
  - ref: crates/editor/src/workspace.rs#Editor::close_workspace
    implements: "Close workspace with active_workspace index adjustment"
  - ref: crates/editor/src/workspace.rs#Editor::switch_workspace
    implements: "Switch to workspace by index"
  - ref: crates/editor/src/left_rail.rs#RAIL_WIDTH
    implements: "Left rail width constant (56px)"
  - ref: crates/editor/src/left_rail.rs#status_color
    implements: "WorkspaceStatus to color mapping for status indicators"
  - ref: crates/editor/src/left_rail.rs#TileRect
    implements: "Tile rectangle with hit-testing via contains()"
  - ref: crates/editor/src/left_rail.rs#LeftRailGeometry
    implements: "Computed geometry for left rail including tile_rects"
  - ref: crates/editor/src/left_rail.rs#calculate_left_rail_geometry
    implements: "Pure layout function for left rail geometry calculation"
  - ref: crates/editor/src/left_rail.rs#LeftRailGlyphBuffer
    implements: "GPU buffer management for left rail rendering"
  - ref: crates/editor/src/left_rail.rs#LeftRailGlyphBuffer::update
    implements: "Build vertex/index buffers for rail background, tiles, indicators, and labels"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_workspace
    implements: "Cmd+N workspace creation handler"
  - ref: crates/editor/src/editor_state.rs#EditorState::close_active_workspace
    implements: "Cmd+Shift+W workspace close handler"
  - ref: crates/editor/src/editor_state.rs#EditorState::switch_workspace
    implements: "Cmd+1..9 workspace switch handler"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Left rail click detection and workspace switching"
  - ref: crates/editor/src/editor_state.rs#EditorState::window_title
    implements: "Window title with workspace label when multiple workspaces"
  - ref: crates/editor/src/renderer.rs#Renderer::render_with_editor
    implements: "Main render entry point with left rail and content area"
  - ref: crates/editor/src/renderer.rs#Renderer::draw_left_rail
    implements: "Left rail rendering with background, tiles, status indicators, and labels"
  - ref: crates/editor/src/renderer.rs#Renderer::set_content_x_offset
    implements: "Content area horizontal offset for left rail"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::set_x_offset
    implements: "Content area x offset storage"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphLayout::position_for_with_xy_offset
    implements: "Position calculation with X and Y offset for left rail and scrolling"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphLayout::quad_vertices_with_xy_offset
    implements: "Quad generation with X and Y offset"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- buffer_view_trait
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Implement the workspace data model and left rail UI — the top level of the two-level tab hierarchy. This is the Composer-like layer: each workspace represents an agent's working context (or a standalone editing environment), and the left rail provides persistent, at-a-glance awareness of all workspaces.

**Data model:**

```rust
struct Editor {
    workspaces: Vec<Workspace>,
    active_workspace: usize,
}

struct Workspace {
    id: WorkspaceId,
    label: String,           // branch name or user-assigned
    root_path: PathBuf,      // worktree root
    tabs: Vec<Tab>,          // content tabs (files, terminals, etc.)
    active_tab: usize,
    status: WorkspaceStatus, // drives left rail indicator
}

struct Tab {
    id: TabId,
    label: String,
    buffer: Box<dyn BufferView>,
    kind: TabKind,           // File, Terminal, AgentOutput, Diff
    dirty: bool,             // unsaved changes (files)
    unread: bool,            // new output since last viewed (terminals)
}

enum WorkspaceStatus {
    Idle,          // ⚪ no agent, just editing
    Running,       // 🟢 agent working autonomously
    NeedsInput,    // 🟡 agent waiting for user
    Stale,         // 🟠 waiting too long
    Completed,     // ✅ agent finished successfully
    Errored,       // 🔴 agent crashed or errored
}
```

**Left rail UI:**

- Vertical strip on the left edge (~48-64px wide)
- Always visible, even with a single workspace
- Each workspace rendered as a compact tile: short label + status indicator
- Selected workspace is visually highlighted
- Clicking a workspace switches the content area to that workspace's tabs
- Keyboard: Cmd+1..9 for direct workspace switching

**Workspace management:**
- Create new workspace (Cmd+N or equivalent)
- Close workspace (Cmd+Shift+W, with confirmation if agent running or unsaved files)
- Workspaces ordered by creation time initially (reordering is a future enhancement)

## Success Criteria

- `Editor`, `Workspace`, and `Tab` data structures are implemented with the relationships described above
- Left rail renders on the left edge showing all workspaces with labels and status indicators
- Clicking a workspace in the left rail switches the content area to that workspace
- Cmd+1..9 keyboard shortcuts switch between workspaces
- Creating a new workspace adds it to the left rail with a default tab
- Closing a workspace removes it from the left rail
- The content area correctly shows only the active workspace's content
- Status indicators visually distinguish all `WorkspaceStatus` variants (distinct colors/icons)
- Left rail is always visible, even with a single workspace
- With one workspace, the editor feels like a normal editor (left rail is minimal/unobtrusive)