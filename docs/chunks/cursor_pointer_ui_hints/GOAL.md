---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths: []
code_references:
  - ref: crates/editor/src/metal_view.rs#CursorRect
    implements: "Rectangular region with coordinates for cursor type mapping"
  - ref: crates/editor/src/metal_view.rs#CursorKind
    implements: "Enum defining pointer vs I-beam cursor types"
  - ref: crates/editor/src/metal_view.rs#CursorRegions
    implements: "Collection of cursor regions for different UI areas"
  - ref: crates/editor/src/metal_view.rs#MetalView::set_cursor_regions
    implements: "API to update cursor regions and trigger recalculation"
  - ref: crates/editor/src/main.rs#EditorController::update_cursor_regions
    implements: "Calculates cursor regions based on current UI layout"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- scroll_bottom_deadzone
- terminal_tab_spawn
- workspace_switching
- cursor_blink_focus
- word_triclass_boundaries
---

# Chunk Goal

## Minor Goal

Change the mouse cursor to a pointer (finger) when hovering over interactive, selectable UI elements, and use a text (I-beam) cursor when hovering over editable text regions. Specifically:

- **Pointer cursor** over: buffer tabs, workspace tabs, file picker entries
- **Text (I-beam) cursor** over: buffer text areas, mini-buffer input

This provides standard visual affordance so users can distinguish clickable/selectable elements from editable text regions at a glance.

## Success Criteria

- Hovering over a buffer tab changes the OS cursor to a pointer (finger/hand)
- Hovering over a workspace tab changes the OS cursor to a pointer
- Hovering over a file entry in the file picker changes the OS cursor to a pointer
- Hovering over the buffer text area shows the text/I-beam cursor
- Hovering over the mini-buffer input area shows the text/I-beam cursor
- Cursor reverts appropriately when moving between regions

