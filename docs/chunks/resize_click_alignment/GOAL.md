---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/row_scroller.rs
- crates/editor/src/viewport.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/row_scroller.rs#RowScroller::update_size
    implements: "Re-clamp scroll_offset_px to new valid bounds when viewport height changes"
  - ref: crates/editor/src/viewport.rs#Viewport::update_size
    implements: "Propagate buffer_line_count to RowScroller for scroll clamping on resize"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_dimensions
    implements: "Pass current buffer line count through to viewport on resize"
  - ref: crates/editor/src/row_scroller.rs#RowScroller::tests::test_resize_clamps_scroll_offset
    implements: "Regression test verifying scroll offset is clamped after viewport resize"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: null
created_after:
- terminal_input_encoding
- find_in_file
- cursor_wrap_scroll_alignment
- row_scroller_extract
---

# Chunk Goal

## Minor Goal

After resizing the window (e.g., entering or leaving fullscreen), clicking in the
editor places the cursor at the wrong vertical position. The cursor lands on a
different line than the one that was clicked. The misalignment persists until the
user scrolls the viewport, at which point everything snaps back into agreement.

The root cause is believed to be in the resize path. When `handle_resize` is
called, `update_viewport_dimensions` updates `visible_lines` inside the
`RowScroller`, but the existing `scroll_offset_px` is **not re-clamped** to the
new valid range. After a resize that enlarges the viewport (e.g., going
fullscreen), the maximum scrollable offset decreases. If the stored
`scroll_offset_px` is now beyond the new maximum, `first_visible_line` (derived
as `floor(scroll_offset_px / line_height)`) will be larger than what the renderer
actually draws, causing a systematic offset between the rendered content position
and the Y-coordinate used to compute the clicked buffer line.

Scrolling fixes the symptom because any scroll event passes through
`set_scroll_offset_px`, which re-clamps the offset and realigns the two
coordinate systems.

The fix is to re-clamp `scroll_offset_px` as part of `Viewport::update_size` (or
wherever `visible_lines` is updated on resize), so that the rendered position and
the click-mapping coordinate system always agree immediately after a window resize.
`EditorState::update_viewport_dimensions` should pass the current buffer line count
through to the viewport so a valid clamp can be performed.

## Success Criteria

- After entering fullscreen (or any resize that changes the window height),
  clicking a line in the editor moves the cursor to exactly the line that was
  clicked, with no further scrolling required.
- Clicking immediately after resize is correct for any pre-resize scroll position,
  including positions near the bottom of large documents.
- Existing viewport and click-positioning tests continue to pass.
- A regression test is added: simulate a resize that shrinks `max_offset_px`, assert
  that `scroll_offset_px` is clamped and that the click-to-line mapping matches the
  rendered line under the clicked pixel.