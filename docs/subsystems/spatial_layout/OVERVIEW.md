---
status: DOCUMENTED
code_references:
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode
    implements: "BSP tree for pane spatial partitioning (Leaf/Split enum)"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#PaneRect
    implements: "Computed screen rectangle for a pane leaf"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#PaneHit
    implements: "Hit-test result with zone classification and pane-local coordinates"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#HitZone
    implements: "TabBar vs Content zone classification"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#calculate_pane_rects
    implements: "Authoritative geometry calculation — recursively partitions bounds by split ratios"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#resolve_pane_hit
    implements: "Hit-testing: screen point → PaneHit with local coordinates"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#PaneLayoutNode::find_target_in_direction
    implements: "Directional pane navigation (walks BSP tree for focus switching)"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#move_tab
    implements: "Tab movement between panes with auto-split creation"
    compliance: COMPLIANT
  - ref: crates/editor/src/pane_layout.rs#cleanup_empty_panes
    implements: "Post-move cleanup — collapses panes that lost all tabs"
    compliance: COMPLIANT
  - ref: crates/editor/src/workspace.rs#Workspace
    implements: "Owns pane_root BSP tree, active_pane_id, and pane management methods"
    compliance: COMPLIANT
  - ref: crates/editor/src/workspace.rs#Tab
    implements: "Leaf content unit — owns buffer + viewport, lives inside panes"
    compliance: COMPLIANT
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Coordinate chain entry point — NSView y-flip, rail partition, pane dispatch"
    compliance: COMPLIANT
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_buffer
    implements: "Pane-aware mouse handling — hit-tests panes, routes to buffer/terminal"
    compliance: COMPLIANT
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_pane_viewports
    implements: "Viewport sync on resize/split — updates viewport sizes and terminal PTY grids"
    compliance: COMPLIANT
  - ref: crates/editor/src/editor_state.rs#EditorState::get_pane_content_dimensions
    implements: "Per-pane content dimensions for scroll handlers"
    compliance: COMPLIANT
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position
    implements: "Pane-local pixel → buffer position (unwrapped mode)"
    compliance: COMPLIANT
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped
    implements: "Pane-local pixel → buffer position (wrapped mode via WrapLayout)"
    compliance: COMPLIANT
  - ref: crates/editor/src/context.rs#EditorContext
    implements: "Bridge between layout and editing — carries pane-scoped dimensions and viewport"
    compliance: COMPLIANT
created_after: ["renderer"]
---

# spatial_layout

## Intent

This subsystem manages the spatial organization of editor content and the coordinate transformations that connect user input to buffer state. It answers three questions:

1. **Where are the panes?** — A binary space partition tree divides the editor area into rectangular panes, each computed on demand from split ratios and the available bounds.
2. **What content is where?** — The workspace/tab/pane hierarchy tracks which buffers and terminals live in which spatial regions.
3. **Where did the user click?** — A coordinate transformation chain maps mouse events from macOS window pixels through pane hit-testing down to buffer positions.

Without this subsystem, every mouse handler, renderer, and resize handler would independently compute pane geometry, leading to misaligned clicks, rendering artifacts at pane borders, and terminals resized to wrong dimensions.

## Scope

### In Scope

- **BSP tree** (`PaneLayoutNode`): Binary tree of `Leaf(Pane)` and `Split { direction, ratio, first, second }` nodes. Recursive geometry calculation, pane enumeration, directional traversal.
- **Pane geometry** (`calculate_pane_rects`): The single authoritative function that partitions a bounding rect into per-pane screen rectangles. Consumed by renderer, hit-testing, and viewport sync.
- **Hit-testing** (`resolve_pane_hit`): Maps a screen-space point to a `PaneHit` — identifying the pane, classifying the zone (TabBar vs Content), and computing pane-local coordinates.
- **Coordinate transformation chain**: The full path from NSView pixel coordinates (y=0 at bottom) through y-flip, left rail partition, pane hit-testing, pane-local offset calculation, and finally pixel-to-buffer-position math (both unwrapped and wrapped modes).
- **Workspace/Tab/Pane hierarchy**: `Workspace` owns a `PaneLayoutNode` as `pane_root`. Panes contain tabs. Tabs own buffers + viewports. `active_pane_id` tracks focus.
- **Pane navigation**: Focus switching via `switch_focus(direction)`, tab movement via `move_active_tab(direction)`, auto-split creation when moving to edges.
- **Viewport sync** (`sync_pane_viewports`): After resize or split, recomputes pane rects and updates each tab's viewport size and terminal PTY grid dimensions.
- **EditorContext**: The bridge struct that carries pane-scoped dimensions (`view_height`, `view_width`) and viewport/buffer references into focus target event handlers.

### Out of Scope

- **Scroll state management**: `RowScroller`, `Viewport`, `WrapLayout`, and `DirtyRegion` belong to the `viewport_scroll` subsystem. This subsystem *consumes* viewport_scroll primitives (scroll_fraction_px, buffer_line_for_screen_row, screen_pos_to_buffer_col) for the final step of coordinate mapping.
- **Rendering**: Drawing pane frames, divider lines, tab bars, and buffer content belongs to the `renderer` subsystem. The renderer *consumes* pane rects from this subsystem.
- **Buffer content**: TextBuffer, gap buffer, editing operations, selections, undo — all outside scope. This subsystem computes buffer *positions*, not buffer *contents*.
- **Left rail layout**: The workspace switcher rail uses simple fixed-width geometry (`x < RAIL_WIDTH`), not the BSP tree. Its layout is trivial and self-contained in `editor_state.rs`.

## Invariants

### Hard Invariants

1. **`calculate_pane_rects` is the single authoritative geometry function.** Every consumer of pane screen positions — renderer, hit-testing, viewport sync, terminal resize — calls this function. There is no cached or pre-computed pane geometry that can drift. Pane rects are computed on demand from the BSP tree and current bounds.

2. **The y-coordinate flip happens exactly once, at `handle_mouse` entry.** macOS NSView uses bottom-left origin (y=0 at bottom). `handle_mouse` flips to screen space (y=0 at top) immediately. All downstream code — rail checks, pane hit-testing, coordinate translation — works in screen space. Adding a second flip would invert everything.

3. **`resolve_pane_hit` produces pane-local coordinates.** The returned `local_x` and `local_y` are relative to the pane's content origin (pane top-left + tab bar height subtracted). Consumers receive coordinates where (0, 0) is the top-left of the pane's content area, ready for scroll-adjusted pixel-to-position conversion.

4. **`sync_pane_viewports` must be called after any layout change.** Resize events, pane splits, pane collapses, and workspace switches all invalidate viewport dimensions. Without sync, viewports report wrong visible line counts, scroll clamping breaks, and terminal PTY grids have wrong dimensions.

5. **BSP tree splits are binary.** Each split node has exactly two children (`first`, `second`) with a `direction` and `ratio`. Multi-way splits are expressed as nested binary splits. This keeps geometry calculation simple (one `ratio` per level) and directional navigation well-defined.

### Soft Conventions

1. **Prefer `resolve_pane_hit` over manual pane rect iteration.** Callers should not call `calculate_pane_rects` and iterate themselves — `resolve_pane_hit` encapsulates the zone classification and local coordinate math.

2. **Tab bar height is a parameter, not a constant in pane_layout.** `resolve_pane_hit` takes `tab_bar_height` as a parameter so the BSP tree module has no dependency on UI styling constants. The caller (editor_state.rs) provides the value.

3. **pixel_to_buffer_position uses floor, not round.** Clicking in the top portion of a line should target that line, not the one above. Both the unwrapped and wrapped variants use `.floor()` for the screen row calculation.

## Implementation Locations

### Pane BSP Tree (`crates/editor/src/pane_layout.rs`)

The core geometry engine. `PaneLayoutNode` is a recursive enum — `Leaf(Pane)` nodes hold tabs, `Split` nodes hold direction, ratio, and two children. All geometry is stateless: `calculate_pane_rects` recursively subdivides the bounding rect by each split's ratio, producing `Vec<PaneRect>` at the leaves.

Key design choice: pane IDs are simple `u64` counters. The tree is walked recursively for lookup (`get_pane`, `get_pane_mut`). This is adequate because pane counts are small (typically 1-4).

`resolve_pane_hit` builds on `calculate_pane_rects` to add zone classification (TabBar vs Content) and pane-local coordinate computation. It subtracts the pane's origin and tab bar height to produce coordinates ready for scroll-adjusted buffer position mapping.

Pane navigation (`find_target_in_direction`) walks upward from a source pane looking for a compatible split ancestor, then descends into the sibling subtree to find the nearest leaf. `move_tab` uses this to move tabs between panes, creating new splits when moving to an edge.

### Workspace and Tab Hierarchy (`crates/editor/src/workspace.rs`)

`Workspace` owns the BSP tree as `pane_root: PaneLayoutNode` and tracks `active_pane_id: PaneId` for focus. It provides convenience methods that delegate to pane_layout functions: `switch_focus(direction)`, `move_active_tab(direction)`, `active_pane()` / `active_pane_mut()`.

`Tab` is the leaf content unit — it owns a `TabBuffer` (File/Terminal/AgentTerminal), a `Viewport`, and metadata (kind, dirty flag, unread flag). The tab constructors create `Viewport::new(line_height)`, linking each tab to the viewport_scroll subsystem.

The two-level hierarchy:
```
Workspace
  └── PaneLayoutNode (BSP tree)
        └── Pane (leaf)
              └── Tab (owns buffer + viewport)
```

### Coordinate Chain Entry (`crates/editor/src/editor_state.rs`)

`handle_mouse` is the single entry point for all mouse events. It performs:
1. y-flip (NSView → screen space)
2. Left rail check (`x < RAIL_WIDTH`)
3. Pane hit-test via `resolve_pane_hit`
4. Zone dispatch (TabBar → `handle_tab_bar_click`, Content → `handle_mouse_buffer`)

`handle_mouse_buffer` uses the `PaneHit`'s local coordinates to create events with pane-relative positions. For file tabs, it builds an `EditorContext` sized to the pane's content dimensions and delegates to the buffer's `handle_mouse`. For terminal tabs, it computes cell row/col from pixel coordinates.

`sync_pane_viewports` is called on resize and layout changes. It calls `calculate_pane_rects` with the full content area, then for each pane rect:
- Updates tab viewport size: `viewport.update_size(content_height, line_count)`
- Resizes terminal PTY grids: `terminal.resize(cols, rows)`

`get_pane_content_dimensions` provides per-pane content sizes for scroll event handlers, ensuring scroll delta handling uses the correct viewport dimensions for the specific pane being scrolled.

### Pixel-to-Buffer Position (`crates/editor/src/buffer_target.rs`)

Two functions complete the coordinate chain:

**`pixel_to_buffer_position`** (unwrapped mode): Takes pane-local `(x, y)` already in screen space. Adds `scroll_fraction_px` to y (compensating for the renderer's sub-pixel translation), divides by `line_height` to get screen line, adds `scroll_offset` (first visible line) to get buffer line, divides x by `char_width` for column.

**`pixel_to_buffer_position_wrapped`** (wrapped mode): Same y-compensation and screen row calculation, but then uses `Viewport::buffer_line_for_screen_row` to find which buffer line owns the absolute screen row (accounting for lines wrapping to multiple rows), and `WrapLayout::screen_pos_to_buffer_col` to map the screen column back through the wrap fold.

Both functions clamp results to valid buffer bounds.

### EditorContext (`crates/editor/src/context.rs`)

The bridge struct that carries pane-scoped layout information into focus target event handlers. Contains `view_height` and `view_width` (set to the pane's content dimensions by `handle_mouse_buffer`), plus mutable references to the tab's buffer and viewport. Provides `wrap_layout()` for creating a `WrapLayout` sized to the current pane, and `ensure_cursor_visible()` for scroll adjustment after cursor-moving operations.

## Known Deviations

No known deviations. The coordinate chain is consistent — y-flip at entry, `resolve_pane_hit` for pane-local conversion, `pixel_to_buffer_position` variants for the final mapping. All geometry consumers use `calculate_pane_rects` as the authoritative source.
