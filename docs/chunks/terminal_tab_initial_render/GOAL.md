---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/terminal/tests/integration.rs
code_references:
  # Note: pending_terminal_created and spin_poll_terminal_startup were removed by
  # child chunk terminal_viewport_init which fixed the root cause (viewport visible_rows=0).
  # The remaining references are tests that validate terminal initial rendering behavior.
  - ref: crates/editor/src/editor_state.rs#tests::test_poll_agents_dirty_after_terminal_creation
    implements: "Test validating poll_agents returns dirty when terminal produces output"
  - ref: crates/editor/src/editor_state.rs#tests::test_new_terminal_tab_marks_dirty
    implements: "Test validating new_terminal_tab marks viewport dirty"
  - ref: crates/terminal/tests/integration.rs#test_shell_produces_content_after_poll
    implements: "Integration test verifying shell produces visible content after polling"
  - ref: crates/terminal/tests/integration.rs#test_poll_events_returns_true_on_output
    implements: "Integration test verifying poll_events returns true on shell output"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- scroll_bottom_deadzone_v3
- terminal_styling_fidelity
---

# Chunk Goal

## Minor Goal

When a new terminal tab is created via Cmd+Shift+T, nothing renders on screen until the window is resized. The terminal content area appears blank despite `DirtyRegion::FullViewport` being set in `new_terminal_tab()`.

The likely cause is that the dirty region is consumed/rendered before the PTY has produced any output, so the initial shell prompt never triggers a

repaint. A window resize then forces a
 
full
 
redraw which makes the content appear.

This chunk will ensure that newly created terminal tabs render their content immediately — either by scheduling a
 
deferred
 
redraw after PTY output arrives, or by ensuring the render loop polls for PTY readiness before
 
considering
 
the frame
 
complete.

## Success Criteria

- Creating a new terminal tab via Cmd+Shift+T renders the shell prompt immediately without requiring a window resize
- Existing terminal tab functionality (input, scrollback, resize) is unaffected
- No visible flicker or double-render artifacts on tab creation



