---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/pane_layout.rs
code_references: []
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
