---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Workspace::switch_to_tab_by_id
    implements: "Cross-pane tab switching for goto-definition navigation"
  - ref: crates/editor/src/editor_state.rs#EditorState::goto_cross_file_definition
    implements: "Cross-file navigation: opens target file in new tab or switches to existing tab, then positions cursor at definition"
  - ref: crates/editor/src/editor_state.rs#EditorState::go_back
    implements: "Cross-tab navigation support for returning to previous position in a different tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::open_file_in_new_tab
    implements: "Helper to open a file in a new tab for cross-file navigation"
  - ref: crates/editor/src/editor_state.rs#EditorState::ensure_cursor_visible_in_active_tab
    implements: "Scrolls viewport to reveal cursor after cross-file navigation"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: uses
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_spawn_reliability
- treesitter_gotodef_type_resolution
---

# Chunk Goal

## Minor Goal

Cross-file goto-definition does not work. When a user Cmd+clicks (or presses
F12) on a symbol defined in a different file — e.g., `DirtyLines` in
`buffer_view.rs` which is defined in `types.rs` — nothing visible happens. The
target file does not open in a new tab.

The Stage 2 cross-file path in `EditorState::goto_definition()` resolves the
symbol correctly via the `SymbolIndex`, but `goto_cross_file_definition()` calls
`associate_file()` which silently replaces the content of the current tab rather
than opening the target file in a new tab. This is destructive — the user loses
their place in the original file.

This chunk fixes `goto_cross_file_definition()` to:

1. Check if the target file is already open in an existing tab (via
   `workspace.find_tab_by_path()`)
2. If found, switch to that tab
3. If not found, create a new tab, load the target file into it, and switch to it
4. Move the cursor to the definition position and scroll the viewport to reveal it

## Success Criteria

- Cmd+click on a symbol defined in another file opens that file in a new tab and
  positions the cursor at the definition
- If the target file is already open in a tab, that tab is activated instead of
  creating a duplicate
- The original file remains open and unmodified in its tab
- The jump stack records the original position so Ctrl+O navigates back
- The viewport scrolls to reveal the cursor at the definition site
- The bug is verified fixed: Cmd+click `DirtyLines` in `buffer_view.rs` opens
  `types.rs` in a new tab with cursor on the `DirtyLines` definition


