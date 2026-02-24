---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_tab_bar_click
    implements: "Multi-pane tab bar click routing with pane hit-testing"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- welcome_scroll
---

# Chunk Goal

## Minor Goal

Fix tab click routing in split pane layouts so that each pane independently
controls its own active tab. Currently, clicking a tab title in the top or
left pane causes the bottom or right pane's active tab to update instead, and
clicking a tab in the bottom or right pane has no effect at all.

This is a correctness bug in how mouse click events for tab bars are dispatched
to the appropriate split pane.

## Success Criteria

- Clicking a tab in the top pane of a horizontal split activates that tab in
  the top pane only; the bottom pane is unaffected.
- Clicking a tab in the bottom pane of a horizontal split activates that tab
  in the bottom pane only; the top pane is unaffected.
- Clicking a tab in the left pane of a vertical split activates that tab in
  the left pane only; the right pane is unaffected.
- Clicking a tab in the right pane of a vertical split activates that tab in
  the right pane only; the left pane is unaffected.
- Tab clicks continue to work correctly in non-split (single-pane) layouts.