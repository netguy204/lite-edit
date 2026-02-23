---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/pane_layout.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Workspace
    implements: "Workspace model with pane_root, active_pane_id, and next_pane_id fields"
  - ref: crates/editor/src/workspace.rs#Workspace::new
    implements: "Workspace constructor creating single Leaf pane"
  - ref: crates/editor/src/workspace.rs#Workspace::with_empty_tab
    implements: "Workspace constructor with single pane containing one tab"
  - ref: crates/editor/src/workspace.rs#Workspace::active_pane
    implements: "Accessor resolving active pane through pane_root"
  - ref: crates/editor/src/workspace.rs#Workspace::active_pane_mut
    implements: "Mutable accessor resolving active pane through pane_root"
  - ref: crates/editor/src/workspace.rs#Workspace::add_tab
    implements: "Tab addition delegating to active pane"
  - ref: crates/editor/src/workspace.rs#Workspace::close_tab
    implements: "Tab close delegating to active pane"
  - ref: crates/editor/src/workspace.rs#Workspace::switch_tab
    implements: "Tab switching delegating to active pane"
  - ref: crates/editor/src/workspace.rs#Workspace::active_tab
    implements: "Active tab accessor delegating to active pane"
  - ref: crates/editor/src/workspace.rs#Workspace::tab_count
    implements: "Tab count in active pane"
  - ref: crates/editor/src/workspace.rs#Workspace::total_tab_count
    implements: "Total tab count across all panes"
  - ref: crates/editor/src/workspace.rs#Workspace::all_panes
    implements: "All panes accessor for iteration"
  - ref: crates/editor/src/workspace.rs#Workspace::all_panes_mut
    implements: "Mutable all panes accessor for iteration"
  - ref: crates/editor/src/workspace.rs#Workspace::poll_standalone_terminals
    implements: "Terminal polling iterating all panes"
  - ref: crates/editor/src/editor_state.rs#EditorState::buffer
    implements: "Buffer accessor resolving through active_workspace→active_pane→active_tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::buffer_mut
    implements: "Mutable buffer accessor resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::try_buffer
    implements: "Optional buffer accessor resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::try_buffer_mut
    implements: "Optional mutable buffer accessor resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::viewport
    implements: "Viewport accessor resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::viewport_mut
    implements: "Mutable viewport accessor resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::associated_file
    implements: "Associated file accessor resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_tab
    implements: "New tab creation operating on active pane"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_terminal_tab
    implements: "New terminal tab creation operating on active pane"
  - ref: crates/editor/src/editor_state.rs#EditorState::close_active_tab
    implements: "Close active tab operating on active pane"
  - ref: crates/editor/src/editor_state.rs#EditorState::next_tab
    implements: "Tab cycling operating on active pane"
  - ref: crates/editor/src/editor_state.rs#EditorState::prev_tab
    implements: "Reverse tab cycling operating on active pane"
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_active_tab_viewport
    implements: "Viewport sync resolving through pane tree"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Mouse coordinate handling with flip-once-at-entry pattern"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_buffer
    implements: "Buffer mouse handling receiving screen-space coordinates"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_selector
    implements: "Selector mouse handling receiving screen-space coordinates"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_tab_bar_click
    implements: "Tab bar click handling with screen-space coordinates"
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position
    implements: "Buffer position calculation expecting screen-space y (no internal flip)"
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped
    implements: "Wrapped buffer position calculation expecting screen-space y"
  - ref: crates/editor/src/pane_layout.rs#Pane
    implements: "Pane struct with tabs and active_tab (mirrors Workspace tab API)"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode
    implements: "Binary pane layout tree with Leaf and Split variants"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::get_pane
    implements: "Pane lookup by ID"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::get_pane_mut
    implements: "Mutable pane lookup by ID"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::all_panes
    implements: "All panes traversal"
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::all_panes_mut
    implements: "Mutable all panes traversal"
  - ref: crates/editor/src/pane_layout.rs#calculate_pane_rects
    implements: "Pane rectangle calculation in screen space"
narrative: null
investigation: tiling_pane_layout
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- tiling_tab_movement
created_after:
- welcome_screen_startup
---
# Chunk Goal

## Minor Goal

Integrate the binary pane layout tree into the `Workspace` and `EditorState` models, replacing the flat `Workspace.tabs` / `Workspace.active_tab` with a pane tree. This is the bridge between the standalone pane tree data model (chunks 1-2) and the rest of the editor.

A single-pane workspace (one `Leaf` node) must behave identically to the old flat model — this chunk should not break any existing functionality. The pane tree becomes visible to the rest of the editor only when a split is created.

This chunk also refactors mouse coordinate handling to follow a strict "flip once, dispatch in pane-local coordinates" pipeline. The current editor has accumulated 7+ bug-fix chunks from ad-hoc coordinate transforms scattered across handlers. The multi-pane design requires getting this right from the start.

## Success Criteria

- **Workspace model changes**:
  - `Workspace.tabs` and `Workspace.active_tab` replaced with `Workspace.pane_root: PaneLayoutNode` and `Workspace.active_pane_id: PaneId`.
  - `Workspace` gains a `next_pane_id: u64` counter for generating unique pane IDs.
  - `Workspace::active_pane() -> Option<&Pane>` and `active_pane_mut() -> Option<&mut Pane>` accessors that resolve through `pane_root` using `active_pane_id`.
  - `Workspace::new()` and `Workspace::with_empty_tab()` create a single `Leaf` pane.
  - `Workspace::add_tab()`, `close_tab()`, `switch_tab()`, `active_tab()`, `active_tab_mut()` delegate to the active pane.
  - `Workspace::tab_count()` sums across all panes.

- **EditorState delegate updates**:
  - `EditorState::buffer()`, `buffer_mut()`, `try_buffer()`, `try_buffer_mut()`, `viewport()`, `viewport_mut()`, `associated_file()` resolve through `active_workspace().active_pane().active_tab()`.
  - `new_tab()`, `new_terminal_tab()`, `close_active_tab()`, `next_tab()`, `prev_tab()` operate on the active pane.
  - `sync_active_tab_viewport()` operates on the active pane's active tab.

- **Terminal polling**:
  - `Workspace::poll_standalone_terminals()` iterates all panes (via `all_panes_mut()`) to poll terminals in every pane, not just the active one.

- **Mouse coordinate refactoring**:
  - `handle_mouse()` flips y from NSView (bottom-left origin) to screen space (top-left origin) once at entry. All downstream code works in screen space.
  - Hit-testing uses `PaneRect` values computed by `calculate_pane_rects()` in screen space. A single loop determines which pane (if any) was clicked.
  - Clicks within a pane's content region are transformed to pane-local coordinates (subtract pane origin) at the dispatch point. `pixel_to_buffer_position` and terminal cell mapping receive pane-local coordinates only.
  - Clicks within a pane's tab bar region are routed to that pane's tab bar handler with pane-local x coordinates.
  - No handler downstream of the dispatch point subtracts `RAIL_WIDTH`, `TAB_BAR_HEIGHT`, or any other global offset — those are accounted for in the pane rect computation.

- **Backward compatibility**:
  - With a single pane (no splits), the editor behaves identically to before this chunk. All existing tests pass. The tab bar renders at the top, content renders below, mouse clicks land on the correct positions.

- **Agent tab handling**:
  - Agent terminals (`AgentTerminal` placeholder tabs) continue to work within panes. The agent handle remains on the `Workspace`, and agent terminal access resolves through the pane containing the agent tab.
