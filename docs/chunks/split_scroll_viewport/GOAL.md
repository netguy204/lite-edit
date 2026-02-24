---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/viewport.rs
  - crates/editor/tests/viewport_test.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::sync_pane_viewports
    implements: "Core per-pane viewport synchronization when layout changes"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_size
    implements: "Triggers sync_pane_viewports on height change"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_dimensions
    implements: "Triggers sync_pane_viewports on dimension change"
  - ref: crates/editor/src/viewport.rs#test_update_size_reduces_visible_lines_after_split
    implements: "Test: visible_lines decreases after split"
  - ref: crates/editor/src/viewport.rs#test_scroll_offset_clamped_after_resize
    implements: "Test: scroll offset clamping after resize"
  - ref: crates/editor/src/viewport.rs#test_viewport_at_bottom_becomes_scrollable_after_resize
    implements: "Test: content becomes scrollable when viewport shrinks"
  - ref: crates/editor/src/viewport.rs#test_scroll_clamping_on_extreme_resize
    implements: "Test: extreme resize scroll clamping"
  - ref: crates/editor/src/editor_state.rs#test_vsplit_reduces_visible_lines
    implements: "Integration test: split reduces visible_lines per pane"
  - ref: crates/editor/src/editor_state.rs#test_tab_becomes_scrollable_after_split
    implements: "Integration test: tabs become scrollable after split"
  - ref: crates/editor/src/editor_state.rs#test_resize_updates_all_pane_viewports
    implements: "Integration test: resize propagates to all panes"
  - ref: crates/editor/src/editor_state.rs#test_scroll_clamped_on_shrink
    implements: "Integration test: scroll offset clamped on shrink"
narrative: null
investigation: null
subsystems:
  - subsystem_id: viewport_scroll
    relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- viewport_emacs_navigation
- pane_scroll_isolation
---

# Chunk Goal

## Minor Goal

After a horizontal split (vertical direction — top/bottom panes), tabs in the resulting panes report an incorrect number of visible lines. A tab whose content previously fit entirely in a full-height pane does not become scrollable when the split reduces the pane's vertical space below what the content requires.

The renderer recalculates `visible_lines` from `pane_content_height / line_height` each frame via `configure_viewport_for_pane()`, but the tab's own `Viewport` may not receive the updated visible-line count in a way that enables scroll-range clamping. This means a tab that was "at bottom" (all content visible, no scroll needed) before the split stays pinned in that state even though content now extends beyond the reduced viewport.

Fixing this ensures that the integrated terminal and file buffers remain fully navigable in split layouts, directly supporting the project goal of tabs being interchangeable regardless of context.

## Success Criteria

- After a horizontal split, each resulting pane's tab reports `visible_lines` consistent with its actual content height (pane height minus tab-bar height, divided by line height).
- A tab whose line count exceeds the post-split visible-line count is scrollable — scroll input moves the viewport and all content is reachable.
- A tab that was already scrolled before a split clamps its scroll offset to the new maximum without jumping or leaving blank space.
- No regression in single-pane visible-line calculation or scroll behavior.