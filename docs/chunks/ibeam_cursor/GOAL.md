---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/metal_view.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView::__reset_cursor_rects
    implements: "I-beam cursor setup via resetCursorRects NSView override"
narrative: editor_ux_refinements
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- mouse_drag_selection
- shift_arrow_selection
- text_selection_rendering
- viewport_scrolling
---
# Chunk Goal

## Minor Goal

Set the mouse cursor to an I-beam (text cursor) when the mouse hovers over the editable area of the editor window. Currently the cursor remains the default arrow pointer over the text area, which breaks the macOS convention that editable text regions display an I-beam. This is a visual-only change at the NSView layer — no buffer or input logic is affected.

## Success Criteria

- When the mouse enters the MetalView bounds, the system cursor changes to `NSCursor.iBeam`
- When the mouse leaves the MetalView bounds, the system cursor reverts to the default arrow
- The I-beam cursor is maintained during mouse movement within the view (including during drag)
- Implementation uses the standard macOS `resetCursorRects` / `addCursorRect:cursor:` API on MetalView
- No functional regressions in existing mouse click, drag, or scroll behavior
