---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_event.rs
  - crates/editor/src/event_channel.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_event.rs#EditorEvent::FileDrop
    implements: "FileDrop event variant now includes position field for pane-aware routing"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_file_drop
    implements: "send_file_drop accepts position parameter"
  - ref: crates/editor/src/metal_view.rs#MetalView::__perform_drag_operation
    implements: "Extract draggingLocation from NSDraggingInfo and convert to screen coordinates"
  - ref: crates/editor/src/metal_view.rs#MetalView::__accepts_first_mouse
    implements: "Returns true for click-through behavior when window is inactive"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_drop
    implements: "Forward file drop with position to EditorState"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_file_drop
    implements: "Position-aware pane routing using resolve_pane_hit instead of active_pane_id"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- app_nap_activity_assertions
- app_nap_blink_timer
- app_nap_file_watcher_pause
- highlight_text_source
- merge_conflict_render
- minibuffer_input
- terminal_single_pane_refresh
---

# Chunk Goal

## Minor Goal

Fix drag-and-drop file insertion so that dragging an image file onto a specific terminal pane inserts the file path into *that* pane — not whichever pane happened to be active before the drop.

The `dragdrop_file_paste` chunk implemented file drag-and-drop, but it has a pane-targeting bug: the drop event carries no position information, so it always routes to the currently active pane. In a multi-pane layout (e.g., file buffer + terminal running Claude Code), dragging an image onto the terminal pane sends the path to the file buffer if that was last focused. Combined with a second issue — `acceptsFirstMouse:` is not implemented — the window activation click/drag doesn't focus the target pane either, making the problem worse when coming from another application.

### Root Cause

Three issues compound:

1. **`FileDrop` event has no position data.** `__perform_drag_operation` in `metal_view.rs` extracts file paths from the drag pasteboard but discards the `draggingLocation` available from `NSDraggingInfo`. The event sent is `EditorEvent::FileDrop(Vec<String>)` — paths only, no coordinates.

2. **`handle_file_drop` routes to the active pane.** In `editor_state.rs:2887`, the handler gets `active_workspace_mut().active_tab_mut()` — whatever pane was last focused — rather than resolving which pane the drop landed on. If the active pane is a file buffer, the path is inserted there instead of the terminal.

3. **`acceptsFirstMouse:` not implemented.** `MetalView` does not override `acceptsFirstMouse:` (default: `false`). When the lite-edit window is not key, the first click/drag activates the window but the `mouseDown:` event is NOT delivered to the view. This means there's no opportunity for pane focus to switch before the drop handler fires.

### Fix Approach

1. Add drop position to `FileDrop`: change `FileDrop(Vec<String>)` to `FileDrop { paths: Vec<String>, position: (f64, f64) }` carrying the `draggingLocation` from `NSDraggingInfo`.
2. In `handle_file_drop`, use `resolve_pane_hit` to determine which pane the drop landed on, and route the file paths to that specific pane's terminal or buffer — regardless of `active_pane_id`.
3. Override `acceptsFirstMouse:` to return `true` so that window-activation clicks also deliver `mouseDown:` to the view, allowing pane focus to update on click-to-focus from another app.

## Success Criteria

- Dragging a file from Finder onto a terminal pane in a multi-pane layout inserts the path into that terminal, even if a different pane was previously active.
- Dragging a file onto an unfocused lite-edit window delivers the path to the pane under the drop point.
- `acceptsFirstMouse:` returns `true`, so clicking a pane in an unfocused window both activates the window and focuses that pane.
- Existing behavior preserved: dragging onto a file buffer still inserts the path as text; dragging onto a single-pane terminal still works.

## Rejected Ideas

### Inline image rendering (Kitty/iTerm2 protocols)

Rendering images inline in the terminal via escape-sequence protocols (Kitty Graphics Protocol, iTerm2 OSC 1337) is a separate, much larger effort requiring renderer changes. This chunk is strictly about ensuring file paths arrive as text so Claude Code can read them.

---