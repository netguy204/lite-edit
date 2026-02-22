---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/workspace.rs
code_references: []
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
<!--
╔══════════════════════════════════════════════════════════════════════════════╗
║  DO NOT DELETE THIS COMMENT BLOCK until the chunk complete command is run.   ║
║                                                                              ║
║  AGENT INSTRUCTIONS: When editing this file, preserve this entire comment    ║
║  block. Only modify the frontmatter YAML and the content sections below      ║
║  (Minor Goal, Success Criteria, Relationship to Parent). Use targeted edits  ║
║  that replace specific sections rather than rewriting the entire file.       ║
╚══════════════════════════════════════════════════════════════════════════════╝
-->

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
