---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_active_tab_viewport
    implements: "Helper method that syncs active tab's viewport to current window dimensions"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_tab
    implements: "Calls sync_active_tab_viewport after creating new tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::switch_tab
    implements: "Calls sync_active_tab_viewport after switching to another tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::associate_file
    implements: "Calls sync_active_tab_viewport after file picker confirmation"
  - ref: crates/editor/src/editor_state.rs#test_new_tab_viewport_is_sized
    implements: "Regression test verifying new tabs have correct visible_lines"
  - ref: crates/editor/src/editor_state.rs#test_switch_tab_viewport_is_sized
    implements: "Regression test verifying tab switching maintains correct visible_lines"
  - ref: crates/editor/src/editor_state.rs#test_associate_file_viewport_is_sized
    implements: "Regression test verifying file picker flow maintains correct visible_lines"
  - ref: crates/editor/src/editor_state.rs#test_sync_viewport_skips_when_no_view_height
    implements: "Edge case test for initial state before window resize"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- selector_hittest_tests
- selector_smooth_render
- resize_click_alignment
- tab_bar_interaction
---

# Chunk Goal

## Minor Goal

Fix a bug where clicking in the buffer of any non-first tab fails to visually
move the cursor to the clicked position. The cursor position IS updated
internally, but no redraw occurs, so the display remains frozen until the user
scrolls (which triggers a full-viewport repaint and reveals the cursor at its
correct location).

**Root cause**: `Viewport` is created with `visible_lines = 0` and only
receives a correct value when `update_viewport_size` / `update_viewport_dimensions`
is called. That call happens on window resize and at initial setup — but only for
the *active* tab. New tabs created via Cmd+T and tabs switched to with no
intervening resize keep `visible_lines = 0`.

`dirty_lines_to_region` computes the visible range as
`[first_visible_line, first_visible_line + visible_lines)`. When `visible_lines
= 0` the range is empty, so every `DirtyLines::Single` and `DirtyLines::Range`
maps to `DirtyRegion::None`. After a mouse click the cursor is repositioned via
`ctx.buffer.set_cursor(position)` and `ctx.mark_cursor_dirty()` is called, but
`mark_cursor_dirty` → `dirty_lines_to_region` → `DirtyRegion::None`, so
`render_if_dirty` skips the repaint. The old cursor image remains until scrolling
triggers `DirtyRegion::FullViewport`.

**Fix**: whenever a tab becomes the active tab — whether through `new_tab`,
`switch_tab`, or `open_file_picker` confirming a file into a new tab — ensure
the new active tab's viewport is sized to the current window dimensions. The
stored `view_height` in `EditorState` is correct (it is a global field updated
on every resize); it just needs to be propagated to the newly active tab's
`Viewport` via `update_viewport_size`.

## Success Criteria

- Clicking anywhere in the buffer of a non-first tab immediately moves the
  cursor to the clicked position with no scrolling required first.
- The fix applies to tabs created by Cmd+T and to tabs opened via the file
  picker (Cmd+O).
- Switching away from a tab and back, then clicking, also places the cursor
  correctly (i.e., the fix is not limited to newly-created tabs).
- Clicking on the first tab continues to work correctly (no regression).
- A regression test is added: create a second tab (without resizing), click at
  a specific position, and assert that the dirty region is non-empty and the
  cursor landed at the expected buffer position.
- All existing viewport and click-positioning tests continue to pass.