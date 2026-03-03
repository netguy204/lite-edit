---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/terminal_buffer.rs
code_references:
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::was_alt_screen
    implements: "Previous alt screen state tracking for mode transition detection"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::poll_events
    implements: "Mode transition detection that forces full viewport dirty on primary/alt screen switch"
narrative: null
investigation: null
subsystems:
  - subsystem_id: renderer
    relationship: uses
  - subsystem_id: viewport_scroll
    relationship: uses
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

When a fullscreen terminal application like Vim or tmux starts in a terminal tab, the application's initial screen content is not painted until the user moves the tab to another pane (triggering a layout change and full repaint). However, animated fullscreen applications like htop paint correctly because they continuously produce output, triggering repeated damage tracking and repaints.

The `terminal_single_pane_refresh` chunk (ACTIVE) fixed a related issue where the single-pane rendering path updated the glyph buffer too early (before acquiring the Metal drawable), causing stale content. That fix moved the glyph buffer update inside the render pass. However, this residual issue suggests that the initial paint from a fullscreen app is still being lost or not triggering a render.

The likely mechanism: when a fullscreen app enters alternate screen mode (`ESC[?1049h`) and draws its initial content, the PTY output arrives and `update_damage()` marks the viewport as dirty. But by the time the render pass runs, either (a) the damage has already been consumed by a prior render that ran before the content was fully written, or (b) the alternate screen transition itself isn't triggering a subsequent render frame. Static apps like Vim then go idle waiting for input, so no further output arrives to trigger another repaint. Animated apps like htop immediately send another frame, masking the issue.

The rendering pipeline flow is: PTY output → `poll_events()` → `update_damage()` → `DirtyRegion::FullViewport` → render. The bug likely lives in the timing between the initial alt-screen content arrival and the render pass consuming the dirty state.

## Success Criteria

- Opening Vim (or another static fullscreen app) in a terminal tab paints its initial screen content within one render frame of the PTY output arriving
- Opening tmux in a terminal tab paints correctly without requiring a pane move
- Animated apps like htop continue to render correctly (no regression)
- The fix does not introduce unnecessary full-viewport invalidations beyond what is already present
- Multi-pane rendering is not regressed