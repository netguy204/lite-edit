---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths: []
code_references:
  - ref: crates/input/src/lib.rs#ScrollDelta
    implements: "Mouse position field for hover-scroll targeting"
  - ref: crates/input/src/lib.rs#ScrollDelta::with_position
    implements: "Constructor for ScrollDelta with mouse position"
  - ref: crates/editor/src/metal_view.rs#MetalView::convert_scroll_event
    implements: "Extract mouse position from NSEvent and convert to pixels"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_scroll
    implements: "Hover-targeted pane scroll routing entry point"
  - ref: crates/editor/src/editor_state.rs#EditorState::find_pane_at_scroll_position
    implements: "Pane hit-testing using mouse position and calculate_pane_rects"
  - ref: crates/editor/src/editor_state.rs#EditorState::scroll_pane
    implements: "Pane-targeted scroll execution without changing focus"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tiling_focus_keybindings
- tiling_multi_pane_render
- startup_workspace_dialog
---

# Chunk Goal

## Minor Goal

Route scroll events to the pane under the mouse cursor rather than the focused pane. Currently, scroll events always go to the active/focused pane regardless of where the mouse cursor is. In a multi-pane tiling layout, each pane should scroll independently, and the pane that receives the scroll event should be determined by the mouse cursor's screen position at the time of the scroll — matching the behavior of VS Code, terminal multiplexers, and other editors with split panes.

This supports the project goal of providing a responsive, native editing experience by making multi-pane workflows feel natural and predictable.

## Success Criteria

- The `scrollWheel:` handler in `metal_view.rs` extracts the mouse location from the NSEvent and includes it in the `ScrollDelta` (or a wrapper struct) passed to the scroll handler callback.
- The scroll event routing logic (`handle_scroll` in `editor_state.rs` or the controller in `main.rs`) uses the mouse position + `calculate_pane_rects` to determine which pane the cursor is over, and routes the scroll to that pane's viewport.
- When the cursor is over a non-focused pane, scrolling that pane does NOT change which pane is focused (hover-scroll, not hover-focus).
- When only a single pane exists, behavior is unchanged from today.
- Existing scroll tests continue to pass; new tests verify hover-targeted scroll routing in a multi-pane layout.