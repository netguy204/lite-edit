---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/renderer.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/selector_overlay.rs
code_references:
- ref: crates/editor/src/selector_overlay.rs#FindStripState
  implements: "State struct for passing find strip info to render_with_editor"
- ref: crates/editor/src/selector_overlay.rs#calculate_find_strip_geometry_in_pane
  implements: "Geometry calculation for find strip within pane bounds"
- ref: crates/editor/src/renderer.rs#Renderer::render_with_editor
  implements: "Unified rendering with optional find_strip parameter"
- ref: crates/editor/src/renderer.rs#Renderer::draw_find_strip_in_pane
  implements: "Draws find strip within pane bounds using scissor rect"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: implements
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- fallback_glyph_metrics
---

# Chunk Goal

## Minor Goal

Fix the find-in-file (Cmd+F) feature so it renders correctly within a multi-pane
layout instead of expanding the focused pane to fill the entire screen.

Currently, pressing Cmd+F in a multi-pane setup causes the focused pane to visually
expand to fill the entire window. When the search is dismissed, panes return to their
original layout. This happens because `render_with_find_strip()` is a separate
rendering path from `render_with_editor()` and has no awareness of the pane tree — it
renders as if the active pane is the only pane.

The fix should eliminate the separate `render_with_find_strip` path and fold find strip
rendering into `render_with_editor`, which already handles both single-pane and
multi-pane layouts. The find strip state can be passed as an optional parameter
(similar to how `selector: Option<&SelectorWidget>` is already passed). The find strip
draws within the focused pane's bounds in both cases.

## Success Criteria

- `render_with_find_strip` is removed; `render_with_editor` handles find strip rendering
- Pressing Cmd+F in a multi-pane layout opens the find strip within the focused pane
  without altering the size or visibility of any pane
- The find strip is rendered at the bottom of the focused pane's rect, not the full window
- All other panes remain visible and correctly rendered while the find strip is active
- Live search, match highlighting, and scroll-to-match still work correctly within the
  pane's viewport
- Dismissing the find strip returns to normal multi-pane rendering with no visual glitch
- Single-pane find strip behavior is unchanged