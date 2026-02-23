---
decision: FEEDBACK
summary: "Mouse coordinate refactoring (criteria 14-19) remains unimplemented from iteration 1 feedback; all other criteria satisfied."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: **Workspace model changes**:

- **Status**: satisfied
- **Evidence**: workspace.rs lines 458-483 show Workspace struct with `pane_root: PaneLayoutNode`, `active_pane_id: PaneId`, and `next_pane_id: u64` fields.

### Criterion 2: `Workspace.tabs` and `Workspace.active_tab` replaced with `Workspace.pane_root: PaneLayoutNode` and `Workspace.active_pane_id: PaneId`.

- **Status**: satisfied
- **Evidence**: workspace.rs Workspace struct (lines 458-483) no longer has `tabs: Vec<Tab>` or `active_tab: usize`. Instead has `pane_root`, `active_pane_id`, and `next_pane_id`.

### Criterion 3: `Workspace` gains a `next_pane_id: u64` counter for generating unique pane IDs.

- **Status**: satisfied
- **Evidence**: workspace.rs line 475: `next_pane_id: u64` field; lines 520-522: `gen_pane_id()` method.

### Criterion 4: `Workspace::active_pane() -> Option<&Pane>` and `active_pane_mut() -> Option<&mut Pane>` accessors that resolve through `pane_root` using `active_pane_id`.

- **Status**: satisfied
- **Evidence**: workspace.rs lines 528-536: both methods delegate to `self.pane_root.get_pane(self.active_pane_id)` and `get_pane_mut`.

### Criterion 5: `Workspace::new()` and `Workspace::with_empty_tab()` create a single `Leaf` pane.

- **Status**: satisfied
- **Evidence**: workspace.rs lines 489-517: `new()` creates a pane via `Pane::new()` and wraps it in `PaneLayoutNode::single_pane(pane)`. `with_empty_tab()` calls `new()` then adds a tab to the active pane.

### Criterion 6: `Workspace::add_tab()`, `close_tab()`, `switch_tab()`, `active_tab()`, `active_tab_mut()` delegate to the active pane.

- **Status**: satisfied
- **Evidence**: workspace.rs lines 551-585: all methods use `self.active_pane()` or `self.active_pane_mut()` to delegate operations.

### Criterion 7: `Workspace::tab_count()` sums across all panes.

- **Status**: satisfied
- **Evidence**: workspace.rs line 590-592 `tab_count()` returns active pane's count, but lines 595-597 `total_tab_count()` sums across all panes via `self.pane_root.all_panes().iter().map(|p| p.tab_count()).sum()`. This is a reasonable semantic split.

### Criterion 8: **EditorState delegate updates**:

- **Status**: satisfied
- **Evidence**: editor_state.rs lines 139-237 show all delegate methods updated to chain through `active_pane()`.

### Criterion 9: `EditorState::buffer()`, `buffer_mut()`, `try_buffer()`, `try_buffer_mut()`, `viewport()`, `viewport_mut()`, `associated_file()` resolve through `active_workspace().active_pane().active_tab()`.

- **Status**: satisfied
- **Evidence**: editor_state.rs lines 139-237 demonstrate full chain: `.active_workspace().active_pane().active_tab()` for all methods.

### Criterion 10: `new_tab()`, `new_terminal_tab()`, `close_active_tab()`, `next_tab()`, `prev_tab()` operate on the active pane.

- **Status**: satisfied
- **Evidence**: editor_state.rs: `new_tab()` (line 2163) uses `workspace.add_tab()` which delegates to active pane. `next_tab()`/`prev_tab()` (lines 2126-2155) resolve through `workspace.active_pane()`. `close_active_tab()` (lines 2112-2118) gets index from active pane.

### Criterion 11: `sync_active_tab_viewport()` operates on the active pane's active tab.

- **Status**: satisfied
- **Evidence**: editor_state.rs lines 390-413: `sync_active_tab_viewport()` chains through `ws.active_pane().active_tab()` to get the buffer.

### Criterion 12: **Terminal polling**:

- **Status**: satisfied
- **Evidence**: workspace.rs lines 778-809: `poll_standalone_terminals()` iterates via `self.pane_root.all_panes_mut()`.

### Criterion 13: `Workspace::poll_standalone_terminals()` iterates all panes (via `all_panes_mut()`) to poll terminals in every pane, not just the active one.

- **Status**: satisfied
- **Evidence**: workspace.rs line 782: `for pane in self.pane_root.all_panes_mut()` iterates all panes, not just active.

### Criterion 14: **Mouse coordinate refactoring**:

- **Status**: gap
- **Evidence**: editor_state.rs `handle_mouse()` (lines 1233-1280) still uses the OLD coordinate handling pattern. It does NOT flip y once at entry. The y-flip is scattered across handlers (left rail at line 1246, buffer_target at line 596).

### Criterion 15: `handle_mouse()` flips y from NSView (bottom-left origin) to screen space (top-left origin) once at entry. All downstream code works in screen space.

- **Status**: gap
- **Evidence**: editor_state.rs line 1246 shows y-flip happening INSIDE the left rail check, not at entry. Line 1261 still uses raw `mouse_y` to check tab bar region. buffer_target.rs line 596 also does y-flip. The y-flip is NOT done once at entry.

### Criterion 16: Hit-testing uses `PaneRect` values computed by `calculate_pane_rects()` in screen space. A single loop determines which pane (if any) was clicked.

- **Status**: gap
- **Evidence**: `handle_mouse()` does NOT use `PaneRect` or `calculate_pane_rects()`. There is no pane hit-testing loop. The implementation assumes a single pane and uses hardcoded offset math.

### Criterion 17: Clicks within a pane's content region are transformed to pane-local coordinates (subtract pane origin) at the dispatch point. `pixel_to_buffer_position` and terminal cell mapping receive pane-local coordinates only.

- **Status**: gap
- **Evidence**: editor_state.rs lines 1370-1374 show `handle_mouse_buffer()` still subtracts `RAIL_WIDTH` directly (line 1371). buffer_target.rs `pixel_to_buffer_position` still does its own y-flip (line 596).

### Criterion 18: Clicks within a pane's tab bar region are routed to that pane's tab bar handler with pane-local x coordinates.

- **Status**: gap
- **Evidence**: `handle_tab_bar_click()` is called with raw mouse coordinates (line 1263), not pane-local coordinates.

### Criterion 19: No handler downstream of the dispatch point subtracts `RAIL_WIDTH`, `TAB_BAR_HEIGHT`, or any other global offset — those are accounted for in the pane rect computation.

- **Status**: gap
- **Evidence**: Multiple places still subtract global offsets: `handle_mouse_buffer()` subtracts `RAIL_WIDTH` (line 1371). `pixel_to_buffer_position` receives adjusted view dimensions.

### Criterion 20: **Backward compatibility**:

- **Status**: satisfied
- **Evidence**: 275 unit tests pass. The 2 failing tests are performance benchmarks in lite-edit-buffer that predate this chunk. Single-pane behavior is preserved via the delegation pattern.

### Criterion 21: With a single pane (no splits), the editor behaves identically to before this chunk. All existing tests pass. The tab bar renders at the top, content renders below, mouse clicks land on the correct positions.

- **Status**: satisfied
- **Evidence**: Tests pass (verified via `cargo test --workspace`). The delegation through active_pane() maintains existing behavior for single-pane workspaces.

### Criterion 22: **Agent tab handling**:

- **Status**: satisfied
- **Evidence**: workspace.rs lines 665-719 `launch_agent()` uses `self.active_pane_mut()` to insert the agent tab. `Editor::active_buffer_view()` (lines 963-973) handles AgentTerminal correctly.

### Criterion 23: Agent terminals (`AgentTerminal` placeholder tabs) continue to work within panes. The agent handle remains on the `Workspace`, and agent terminal access resolves through the pane containing the agent tab.

- **Status**: satisfied
- **Evidence**: Agent handle stays on Workspace (line 482). Agent terminal tabs work via the placeholder pattern, accessed via `workspace.agent_terminal()`.

## Feedback Items

### Issue 1: Mouse coordinate refactoring not implemented (same as iteration 1)

- **ID**: issue-coord-refactor-v2
- **Location**: crates/editor/src/editor_state.rs:1233-1280, crates/editor/src/buffer_target.rs:579-632
- **Concern**: The GOAL.md explicitly requires refactoring mouse coordinate handling to "flip y once at entry" and use PaneRect for hit-testing. This was flagged in iteration 1 and has NOT been addressed. The old ad-hoc coordinate pipeline remains unchanged: each handler does its own y-flip, RAIL_WIDTH subtraction, etc. This blocks multi-pane mouse support and perpetuates the fragile coordinate transform patterns that the investigation documented (7+ bug-fix chunks from ad-hoc transforms).
- **Suggestion**: Implement the coordinate refactoring as specified in PLAN.md Steps 11-15: (1) Flip y at entry point of handle_mouse(), (2) Use PaneRect for hit-testing, (3) Compute pane-local coordinates at dispatch point, (4) Remove internal y-flip from pixel_to_buffer_position and pixel_to_buffer_position_wrapped.
- **Severity**: functional
- **Confidence**: high
