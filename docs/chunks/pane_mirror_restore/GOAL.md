---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/session.rs
- crates/editor/src/workspace.rs
- crates/editor/src/renderer/panes.rs
- crates/editor/src/renderer/mod.rs
- crates/editor/tests/session_persistence.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Workspace::set_next_pane_id
    implements: "Setter for next_pane_id counter to prevent ID collisions after session restore"
  - ref: crates/editor/src/session.rs#SessionData::restore_into_editor
    implements: "next_pane_id synchronization during session restoration"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::render_with_editor
    implements: "Glyph buffer update skip in multi-pane mode to prevent cache contamination"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::render_with_confirm_dialog
    implements: "Glyph buffer update skip in multi-pane mode (confirm dialog path)"
  - ref: crates/editor/src/renderer/panes.rs#Renderer::render_pane
    implements: "Styled line cache clearing between pane renders for render isolation"
  - ref: crates/editor/tests/session_persistence.rs#test_empty_pane_restore_no_id_collision
    implements: "Regression test for pane ID collision after empty pane restore"
  - ref: crates/editor/tests/session_persistence.rs#test_create_pane_after_restore
    implements: "Comprehensive test for creating new panes after session restore"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- focus_stack
- grapheme_cluster_awareness
- invalidation_separation
- quad_buffer_prealloc
- renderer_decomposition
- styled_line_cache
- unicode_ime_input
---

# Chunk Goal

## Minor Goal

Fix a bug where a restored workspace with a pane that originally contained only
a terminal tab causes that pane to mirror the content of the active (focused)
pane. After session restore, the terminal-only pane gets an empty "Untitled"
placeholder tab (since terminal tabs are filtered during serialization). This
placeholder tab enters a broken state where:

1. **Input duplication**: Typing in the focused pane inserts text into both the
   focused pane's buffer AND appears in the restored pane
2. **Render mirroring**: The restored pane renders whatever content the active
   pane shows, including when switching tabs in the active pane
3. **Partial recovery**: Creating a NEW tab in the restored pane works correctly
   while that new tab is active, but switching back to the original placeholder
   Untitled tab re-triggers the mirroring behavior

### Bug context

The session restoration path (`session.rs:into_pane`) correctly creates an empty
`Tab::empty_file()` when a pane has no restorable tabs (line 577-582). The input
routing (`editor_state.rs:handle_insert_text`) correctly routes to only the
active pane's active tab via `active_workspace_mut() → active_tab_mut()`. The
renderer (`renderer/panes.rs:render_pane`) correctly gets each pane by ID and
renders its own tab content. Yet the bug persists, suggesting a subtle
interaction between:

- **Session restoration state**: Something about the restored empty tab differs
  from a freshly created one (possibly `workspace.next_pane_id` not being
  updated after pane tree replacement, causing ID conflicts on later operations)
- **Render pipeline early update**: `render_with_editor` (mod.rs:606-641) always
  updates the glyph buffer with the active tab's content before the multi-pane
  render loop — this wasted work in multi-pane mode could mask or interact with
  the bug
- **Styled line cache**: The `StyledLineCache` is shared across pane render
  passes within a single frame and indexed by line number — cross-pane
  contamination is possible if resize/clear semantics don't fully isolate
- **`next_pane_id` drift**: `restore_into_editor` uses a local `next_pane_id`
  counter but never writes it back to `workspace.next_pane_id`, which could
  cause pane ID collisions when new panes are created later

## Success Criteria

- **Primary**: After restoring a workspace that had a terminal-only pane, the
  restored pane displays its own content (empty Untitled) independently from
  other panes — no input duplication, no render mirroring
- **Regression test**: Add a test verifying that session restoration with an
  empty pane (terminal-only, filtered during save) produces a pane with correct
  independent state
- **`next_pane_id` audit**: Verify `workspace.next_pane_id` is correctly updated
  after session restore to be >= all restored pane IDs (prevent future ID
  collisions)
- **Render isolation audit**: Confirm the glyph buffer and styled line cache are
  fully isolated between pane render passes (no stale data leaks)
- The redundant glyph buffer update at `render_with_editor:606-641` should be
  skipped in multi-pane mode (it currently runs for all modes, wasting work and
  potentially masking the root cause)