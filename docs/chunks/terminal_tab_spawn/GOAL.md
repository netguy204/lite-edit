---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::new_terminal_tab
    implements: "Creates a new standalone terminal tab with shell spawning, dimension calculation, and numbered labeling"
  - ref: crates/editor/src/editor_state.rs#EditorState::terminal_tab_count
    implements: "Helper to count existing terminal tabs for sequential label numbering"
  - ref: crates/editor/src/editor_state.rs#EditorState::poll_agents
    implements: "Extended to poll standalone terminals via workspace.poll_standalone_terminals()"
  - ref: crates/editor/src/workspace.rs#Workspace::poll_standalone_terminals
    implements: "Polls PTY events for all standalone terminal tabs in a workspace"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key
    implements: "Cmd+Shift+T keybinding handler that calls new_terminal_tab()"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_emulator
- agent_lifecycle
created_after:
- tab_bar_content_clip
- click_scroll_fraction_alignment
---

# Chunk Goal

## Minor Goal

Wire `Cmd+Shift+T` in `EditorState::handle_key` to spawn a new standalone terminal tab in the active workspace. The `terminal_emulator` chunk built `TerminalBuffer` and `Tab::new_terminal`, and the `agent_lifecycle` chunk built the PTY polling loop — but neither chunk added a user-accessible keybinding to open a terminal. This chunk adds that missing entry point.

When the user presses `Cmd+Shift+T`, a new tab backed by a `TerminalBuffer` running the user's default shell (from `$SHELL`, falling back to `/bin/sh`) should appear in the active workspace's tab bar and become the active tab.

## Success Criteria

- `Cmd+Shift+T` creates a new terminal tab in the active workspace and switches to it
- The shell process is spawned using `$SHELL`, falling back to `/bin/sh`
- The terminal is sized to the current viewport dimensions (cols × rows derived from font metrics and view size at the moment of creation)
- The tab label is `"Terminal"`, or `"Terminal 2"`, `"Terminal 3"`, etc. when multiple terminal tabs exist in the same workspace
- Pressing `Cmd+Shift+T` again opens a second terminal tab (multiple terminals supported)
- The existing `Cmd+T` behaviour (new empty file tab) is unchanged
- The existing test asserting `Cmd+Shift+T` does nothing is updated to assert the new behaviour
