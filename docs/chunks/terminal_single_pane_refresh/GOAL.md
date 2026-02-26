---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/renderer/mod.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/renderer/mod.rs#Renderer::render_with_editor
    implements: "Single-pane terminal refresh fix - glyph buffer update moved inside render pass"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::render_with_confirm_dialog
    implements: "Same fix applied to confirm dialog rendering path for consistency"
narrative: null
investigation: null
subsystems:
  - subsystem_id: renderer
    relationship: uses
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- buffer_file_watching
- highlight_injection
---

# Chunk Goal

## Minor Goal

When a terminal is spawned in a single-pane workspace (via Cmd+Shift+T or workspace creation), the tab is added to the tab bar and activated, but the content area does not refresh with TTY output. The pane remains visually stale until the user either:
- Moves the tab to its own pane (triggering multi-pane rendering)
- Opens the terminal in an already multi-pane layout

This is a rendering path divergence between single-pane and multi-pane modes. The single-pane path in `render_with_editor()` (`crates/editor/src/renderer/mod.rs:608-641`) performs an early glyph buffer update before Metal encoding. The multi-pane path delegates to `render_pane()` (`crates/editor/src/renderer/panes.rs:192+`) which independently configures the viewport and updates the glyph buffer per pane.

The likely cause is that in single-pane mode, the glyph buffer update runs with stale or empty terminal content (the PTY hasn't produced output yet for that frame), and subsequent PTY output does not trigger an effective repaint of the content area. The multi-pane `render_pane()` path handles this correctly, suggesting the single-pane path is missing an invalidation signal or a deferred redraw when terminal content arrives.

This is related to but distinct from `terminal_tab_initial_render` (which fixed blank screens due to viewport `visible_rows=0`) and `terminal_viewport_init` (which fixed scroll_to_bottom computing wrong offsets). Both of those chunks are already ACTIVE. This bug specifically manifests as the single-pane rendering path not refreshing when a newly-active terminal tab begins producing output.

## Success Criteria

- Spawning a terminal tab via Cmd+Shift+T in a single-pane workspace renders the shell prompt within one frame of PTY output arrival
- The fix does not regress multi-pane terminal rendering
- The fix does not introduce unnecessary full-viewport invalidations (respect the existing dirty region / invalidation separation architecture)