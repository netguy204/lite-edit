---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/viewport.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/viewport.rs#Viewport::dirty_lines_to_region_wrapped
    implements: "Guard against zero visible lines returning FullViewport for degenerate viewport"
  - ref: crates/editor/src/viewport.rs#Viewport::dirty_lines_to_region
    implements: "Same guard for non-wrapped dirty region conversion"
  - ref: crates/editor/src/editor_state.rs#EditorState::cursor_dirty_region
    implements: "Defense-in-depth guard returning FullViewport when viewport uninitialized"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- dialog_pointer_cursor
- file_open_picker
- pane_cursor_click_offset
- pane_tabs_interaction
---

# Chunk Goal

## Minor Goal

Fix a bug where the cursor in the active pane stops blinking under certain
conditions. The NSTimer continues to fire and toggle `cursor_visible`, but the
dirty region returned by `toggle_cursor_blink()` fails to trigger a repaint —
so the cursor freezes in whatever visibility state it was in when the bug was
entered.

**Observed symptoms:**

- Cursor stops blinking in the active pane (appears frozen on or off)
- Scrolling the pane causes the cursor to visibly blink again, because scroll
  events mark `FullViewport` dirty, which triggers a full repaint that picks up
  the continuously-toggling `cursor_visible` state
- Stopping the scroll freezes the cursor at whatever blink phase it happened to
  be in (on if stopped during visible phase, off if stopped during hidden phase)
- Switching lite-edit workspaces sometimes breaks out of the stalled state, but
  not reliably. Switching macOS Spaces also does not reliably clear the state.

**Likely root cause area:**

`cursor_dirty_region()` in `editor_state.rs` computes a
`DirtyLines::Single(cursor_line)` and maps it through
`dirty_lines_to_region_wrapped()`. If the viewport state, wrap layout, or
visible-lines count becomes stale relative to the actual screen state, this
mapping can return `DirtyRegion::None` even though the cursor is on-screen —
causing `toggle_cursor_blink()` to toggle the boolean without triggering a
repaint. The fact that workspace switches don't reliably clear the state
suggests the staleness may persist across viewport reconfigurations. The
investigation should focus on when and how viewport metadata can drift out of
sync with what's actually rendered.

## Success Criteria

- The cursor blinks reliably in the active pane under all normal editing
  conditions (no unexplained stalls)
- Identify and fix the specific condition that causes `cursor_dirty_region()` to
  return `None` / a no-op dirty region when the cursor is actually visible
- Add a test that reproduces the stale-viewport condition (if feasible) and
  verifies the cursor dirty region is non-empty when the cursor is on-screen