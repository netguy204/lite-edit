<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk introduces the two-level workspace model for lite-edit:

1. **Data Model Layer**: Define `Editor`, `Workspace`, `Tab`, and supporting types (`WorkspaceId`, `TabId`, `TabKind`, `WorkspaceStatus`) in a new `workspace` module.

2. **State Migration**: Refactor `EditorState` to use the new workspace model. The current `buffer`, `viewport`, `associated_file` fields become properties of the active workspace's active tab. The `EditorState` becomes a thin wrapper around `Editor`.

3. **Left Rail UI**: Render a vertical strip on the left edge showing workspace tiles with labels and status indicators. This extends the existing Metal rendering pipeline.

4. **Interaction Layer**: Add keyboard navigation (Cmd+1..9 for workspace switching), mouse click handling for the left rail, and workspace create/close commands (Cmd+N, Cmd+Shift+W).

**Strategy**: Work inside-out — define the data model first, migrate existing single-buffer behavior to work within the model, then layer on the UI and interactions. This preserves the existing editor functionality throughout the migration.

**Testing Approach**: Per TESTING_PHILOSOPHY.md:
- Data model operations (workspace creation, tab switching, etc.) are pure state manipulation and will be unit tested with semantic assertions.
- Left rail rendering is humble view code — we test the layout math and state, not the Metal rendering itself.
- Keyboard/mouse handlers follow the existing `FocusTarget` pattern and are tested by simulating events and asserting on state changes.

## Sequence

### Step 1: Define workspace data model types

Create `crates/editor/src/workspace.rs` with:

```rust
pub type WorkspaceId = u64;
pub type TabId = u64;

pub enum WorkspaceStatus {
    Idle,       // No agent, just editing
    Running,    // Agent working autonomously
    NeedsInput, // Agent waiting for user
    Stale,      // Waiting too long
    Completed,  // Agent finished successfully
    Errored,    // Agent crashed or errored
}

pub enum TabKind {
    File,
    Terminal,
    AgentOutput,
    Diff,
}

pub struct Tab {
    pub id: TabId,
    pub label: String,
    pub buffer: Box<dyn BufferView>,
    pub kind: TabKind,
    pub dirty: bool,   // unsaved changes
    pub unread: bool,  // new output since last viewed (terminals)
}

pub struct Workspace {
    pub id: WorkspaceId,
    pub label: String,
    pub root_path: PathBuf,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub status: WorkspaceStatus,
}

pub struct Editor {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    next_workspace_id: u64,
    next_tab_id: u64,
}
```

Add ID generation methods to `Editor` and basic workspace/tab manipulation methods.

**Tests**:
- Creating an `Editor` with an empty workspace succeeds
- `Editor::active_workspace()` returns the correct workspace
- `Workspace::active_tab()` returns the correct tab
- Adding/removing workspaces updates state correctly

Location: `crates/editor/src/workspace.rs`

### Step 2: Implement Tab with BufferView and viewport

Extend the `Tab` struct to own the buffer's viewport (for scroll position) and provide methods to access the underlying buffer:

```rust
pub struct Tab {
    pub id: TabId,
    pub label: String,
    buffer: Box<dyn BufferView>,
    pub viewport: Viewport,
    pub kind: TabKind,
    pub dirty: bool,
    pub unread: bool,
    pub associated_file: Option<PathBuf>,
}
```

Add methods:
- `Tab::new_file(buffer, label, path)` - Creates a file tab
- `Tab::buffer(&self) -> &dyn BufferView`
- `Tab::buffer_mut(&mut self) -> &mut dyn BufferView` (returns `Option` or uses a downcast)
- `Tab::as_text_buffer_mut(&mut self) -> Option<&mut TextBuffer>` for editable buffers

The challenge here is that `BufferView` is a trait, but we need mutable access for editing. Since `TextBuffer` implements `BufferView`, and for now all tabs are file tabs backed by `TextBuffer`, we can use `Any` downcasting or store the concrete type and return `&dyn BufferView` for rendering.

**Design decision**: Store `TextBuffer` directly in file tabs (not boxed trait), use an enum:
```rust
pub enum TabBuffer {
    File(TextBuffer),
    // Future: Terminal(TerminalBuffer),
}
```

This avoids trait object downcasting complexity while still allowing heterogeneous tabs in the future.

**Tests**:
- Creating a file tab with content succeeds
- `Tab::buffer()` returns a valid BufferView reference
- `Tab::as_text_buffer_mut()` returns `Some` for file tabs

Location: `crates/editor/src/workspace.rs`

### Step 3: Implement Workspace methods

Add workspace-level operations:

```rust
impl Workspace {
    pub fn new(id: WorkspaceId, label: String, root_path: PathBuf) -> Self
    pub fn add_tab(&mut self, tab: Tab)
    pub fn close_tab(&mut self, index: usize) -> Option<Tab>
    pub fn active_tab(&self) -> Option<&Tab>
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab>
    pub fn switch_tab(&mut self, index: usize)
}
```

The workspace owns a `Vec<Tab>` and tracks which tab is active.

**Tests**:
- Creating a workspace with a default tab works
- Switching tabs updates `active_tab`
- Closing the active tab selects an adjacent tab
- Closing the last tab leaves the workspace empty (or with a placeholder)

Location: `crates/editor/src/workspace.rs`

### Step 4: Implement Editor workspace operations

Add editor-level operations:

```rust
impl Editor {
    pub fn new() -> Self // Creates with one empty workspace
    pub fn new_workspace(&mut self, label: String, root_path: PathBuf) -> WorkspaceId
    pub fn close_workspace(&mut self, index: usize) -> Option<Workspace>
    pub fn active_workspace(&self) -> Option<&Workspace>
    pub fn active_workspace_mut(&mut self) -> Option<&mut Workspace>
    pub fn switch_workspace(&mut self, index: usize)
    pub fn workspace_count(&self) -> usize
}
```

**Tests**:
- Creating an editor initializes with one workspace
- `switch_workspace` updates `active_workspace`
- `new_workspace` adds a workspace and optionally switches to it
- `close_workspace` removes the workspace and selects an adjacent one
- Cannot close the last workspace (or it creates an empty replacement)

Location: `crates/editor/src/workspace.rs`

### Step 5: Refactor EditorState to use Editor

Modify `EditorState` to wrap the workspace model:

```rust
pub struct EditorState {
    pub editor: Editor,  // Replaces: buffer, viewport, associated_file
    pub dirty_region: DirtyRegion,
    pub focus_target: BufferFocusTarget,
    pub cursor_visible: bool,
    // ... other fields remain
}
```

Add delegate methods that forward to the active workspace/tab:

```rust
impl EditorState {
    pub fn buffer(&self) -> Option<&TextBuffer> { ... }
    pub fn buffer_mut(&mut self) -> Option<&mut TextBuffer> { ... }
    pub fn viewport(&self) -> Option<&Viewport> { ... }
    pub fn viewport_mut(&mut self) -> Option<&mut Viewport> { ... }
    pub fn associated_file(&self) -> Option<&PathBuf> { ... }
}
```

Update all call sites that previously accessed `self.buffer`, `self.viewport`, `self.associated_file` to use these delegate methods.

**Critical**: This step should preserve all existing functionality. The editor with one workspace and one tab should behave identically to before.

**Tests**:
- All existing `EditorState` tests continue to pass
- Creating `EditorState::empty()` results in one workspace with one empty tab
- `buffer()` and `buffer_mut()` return the active tab's buffer

Location: `crates/editor/src/editor_state.rs`

### Step 6: Update key/mouse handlers for workspace model

Update the event handlers in `EditorState` to work with the workspace model:

- `handle_key_buffer` forwards to the active tab's buffer
- `handle_mouse_buffer` uses the active tab's viewport
- Scroll handling uses the active tab's viewport
- `save_file` uses the active tab's `associated_file`

This is largely mechanical — replacing `self.buffer` with `self.editor.active_workspace().unwrap().active_tab().unwrap().as_text_buffer()` (wrapped in helper methods).

**Tests**:
- Existing key/mouse tests continue to pass
- Typing in the editor modifies the active tab's buffer

Location: `crates/editor/src/editor_state.rs`

### Step 7: Update renderer integration

Update the sync between `EditorState` and `Renderer`:

- The renderer's `buffer` field should be populated from the active workspace's active tab
- `sync_renderer_buffer` in `EditorController` pulls from `state.buffer()`
- Viewport sync uses `state.viewport()`

**Tests**:
- Existing rendering behavior unchanged (visual verification)
- Switching tabs should switch what's rendered (later, once UI is implemented)

Location: `crates/editor/src/main.rs`

### Step 8: Define left rail layout constants and geometry

Create `crates/editor/src/left_rail.rs` with layout constants:

```rust
pub const RAIL_WIDTH: f32 = 56.0;      // pixels (scaled)
pub const TILE_HEIGHT: f32 = 48.0;
pub const TILE_PADDING: f32 = 4.0;
pub const STATUS_INDICATOR_SIZE: f32 = 8.0;

pub struct LeftRailGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub tile_rects: Vec<(f32, f32, f32, f32)>,  // (x, y, w, h) for each workspace tile
}

pub fn calculate_left_rail_geometry(
    view_height: f32,
    workspace_count: usize,
) -> LeftRailGeometry { ... }
```

This is pure math, fully testable.

**Tests**:
- Geometry with 1 workspace produces 1 tile rect
- Geometry with 5 workspaces produces 5 tile rects in a vertical stack
- Tile rects don't exceed view height

Location: `crates/editor/src/left_rail.rs`

### Step 9: Add workspace status color mapping

Define colors for workspace status indicators:

```rust
pub fn status_color(status: &WorkspaceStatus) -> [f32; 4] {
    match status {
        WorkspaceStatus::Idle => [0.5, 0.5, 0.5, 1.0],       // Gray
        WorkspaceStatus::Running => [0.2, 0.8, 0.2, 1.0],    // Green
        WorkspaceStatus::NeedsInput => [0.9, 0.8, 0.1, 1.0], // Yellow
        WorkspaceStatus::Stale => [0.9, 0.6, 0.1, 1.0],      // Orange
        WorkspaceStatus::Completed => [0.2, 0.7, 0.2, 1.0],  // Checkmark green
        WorkspaceStatus::Errored => [0.9, 0.2, 0.2, 1.0],    // Red
    }
}
```

**Tests**:
- Each status maps to a distinct color

Location: `crates/editor/src/left_rail.rs`

### Step 10: Create LeftRailGlyphBuffer

Similar to `SelectorGlyphBuffer`, create a glyph buffer for the left rail:

```rust
pub struct LeftRailGlyphBuffer {
    // Manages vertices for:
    // - Rail background
    // - Workspace tiles
    // - Status indicators
    // - Workspace labels
}

impl LeftRailGlyphBuffer {
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        editor: &Editor,
        geometry: &LeftRailGeometry,
        active_workspace: usize,
    );
}
```

**Tests**:
- Unit tests for glyph layout math (similar to existing glyph buffer tests)

Location: `crates/editor/src/left_rail.rs`

### Step 11: Integrate left rail rendering into Renderer

Add left rail rendering to the `Renderer`:

```rust
impl Renderer {
    pub fn render_with_left_rail(
        &mut self,
        view: &MetalView,
        editor: &Editor,
        selector: Option<&SelectorWidget>,
        // ...
    );
}
```

This draws:
1. Left rail background
2. Workspace tiles
3. Selected workspace highlight
4. Status indicators
5. Workspace labels (abbreviated)
6. Then the content area (offset by RAIL_WIDTH)

**Note**: The content area rendering needs to be offset by `RAIL_WIDTH` — the glyph buffer positions must account for this.

Location: `crates/editor/src/renderer.rs`

### Step 12: Offset content area rendering

Update `GlyphBuffer::update_from_buffer_with_cursor` and related methods to accept an `x_offset` parameter, which will be `RAIL_WIDTH` when the left rail is visible.

Alternatively, handle this in the shader/uniforms by passing a content area offset.

**Tests**:
- Content renders to the right of the rail (visual verification)

Location: `crates/editor/src/glyph_buffer.rs`, `crates/editor/src/renderer.rs`

### Step 13: Add Cmd+1..9 workspace switching

Intercept Cmd+1 through Cmd+9 in `handle_key`:

```rust
if event.modifiers.command && !event.modifiers.control && !event.modifiers.shift {
    if let Key::Char(c) = event.key {
        if let Some(digit) = c.to_digit(10) {
            if digit >= 1 && digit <= 9 {
                let idx = (digit - 1) as usize;
                if idx < self.editor.workspace_count() {
                    self.editor.switch_workspace(idx);
                    self.dirty_region.merge(DirtyRegion::FullViewport);
                    return;
                }
            }
        }
    }
}
```

**Tests**:
- Cmd+1 switches to first workspace
- Cmd+2 with only one workspace is a no-op
- Cmd+3 with 3 workspaces switches to third

Location: `crates/editor/src/editor_state.rs`

### Step 14: Add Cmd+N new workspace

Intercept Cmd+N in `handle_key`:

```rust
if let Key::Char('n') = event.key {
    if event.modifiers.command && !event.modifiers.control {
        let workspace_id = self.editor.new_workspace(
            "untitled".to_string(),
            std::env::current_dir().unwrap_or_default(),
        );
        // Switch to the new workspace
        self.editor.switch_workspace(self.editor.workspaces.len() - 1);
        self.dirty_region.merge(DirtyRegion::FullViewport);
        return;
    }
}
```

**Tests**:
- Cmd+N creates a new workspace and switches to it
- New workspace has one empty tab

Location: `crates/editor/src/editor_state.rs`

### Step 15: Add Cmd+Shift+W close workspace

Intercept Cmd+Shift+W in `handle_key`:

```rust
if let Key::Char('w') = event.key {
    if event.modifiers.command && event.modifiers.shift && !event.modifiers.control {
        // Don't close the last workspace
        if self.editor.workspace_count() > 1 {
            // TODO: Check for running agents or unsaved files (future chunk)
            self.editor.close_workspace(self.editor.active_workspace);
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
        return;
    }
}
```

**Tests**:
- Cmd+Shift+W closes the active workspace when there are multiple
- Cmd+Shift+W with one workspace is a no-op
- After closing, the previous workspace becomes active (or next if closing first)

Location: `crates/editor/src/editor_state.rs`

### Step 16: Add mouse click handling for left rail

Update `handle_mouse` to detect clicks in the left rail:

```rust
fn handle_mouse(&mut self, event: MouseEvent) {
    // Check if click is in left rail region
    if event.position.x < RAIL_WIDTH {
        if let MouseEventKind::Down = event.kind {
            // Calculate which workspace was clicked
            let geometry = calculate_left_rail_geometry(self.view_height, self.editor.workspace_count());
            for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
                if self.point_in_rect(event.position, tile_rect) {
                    self.editor.switch_workspace(idx);
                    self.dirty_region.merge(DirtyRegion::FullViewport);
                    return;
                }
            }
        }
        return; // Don't forward to buffer
    }

    // Existing buffer mouse handling, but offset position by RAIL_WIDTH
    // ...
}
```

**Tests**:
- Clicking a workspace tile in the rail switches to that workspace
- Clicking outside any tile is a no-op
- Mouse events in the content area (right of rail) are forwarded to buffer

Location: `crates/editor/src/editor_state.rs`

### Step 17: Update window title for workspaces

Update `window_title()` to include workspace context when multiple workspaces exist:

```rust
pub fn window_title(&self) -> String {
    let workspace = self.editor.active_workspace()?;
    let tab_name = workspace.active_tab()
        .and_then(|t| t.associated_file.as_ref())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");

    if self.editor.workspace_count() > 1 {
        format!("{} — {}", tab_name, workspace.label)
    } else {
        tab_name.to_string()
    }
}
```

**Tests**:
- With one workspace, title is just the filename
- With multiple workspaces, title includes workspace label

Location: `crates/editor/src/editor_state.rs`

### Step 18: Integration testing and polish

- Verify all existing tests pass
- Run the editor and verify:
  - Left rail is always visible on the left edge
  - Single workspace looks minimal and unobtrusive
  - Workspace tiles show labels and status indicators
  - Clicking tiles switches workspaces
  - Cmd+1..9 switches workspaces
  - Cmd+N creates new workspace
  - Cmd+Shift+W closes workspace (with >1 workspace)
  - Content area renders correctly to the right of the rail

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments:
```rust
// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
```

## Dependencies

- **buffer_view_trait** (ACTIVE): This chunk depends on the `BufferView` trait being implemented for `TextBuffer`. The `Tab` struct uses `BufferView` for polymorphic buffer access.

## Risks and Open Questions

1. **Viewport per tab vs shared viewport**: Each tab should have its own scroll position, so `Viewport` must be moved into `Tab`. This is straightforward but affects many call sites.

2. **Downcast complexity**: Accessing the mutable `TextBuffer` inside a `Tab` for editing requires either:
   - Storing concrete types and using enum dispatch (chosen approach)
   - Using `Any` downcasting (more complex)

   The enum approach is simpler but requires updating the enum when new buffer types are added.

3. **Content area offset**: The left rail shifts all content rendering to the right. This affects:
   - Glyph buffer positioning
   - Mouse click position translation
   - Viewport calculations

   Need to ensure this offset is applied consistently.

4. **Selector overlay with left rail**: When the file picker is open, it should still appear centered over the content area, not the full window. Need to adjust `calculate_overlay_geometry` to account for the rail width.

5. **Performance**: With multiple workspaces, we maintain multiple `TextBuffer` instances. Each buffer has its own gap buffer and line index. This is intentional (workspaces are independent) but worth noting for memory-constrained scenarios.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->