---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/workspace.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::new_workspace
    implements: "Conditional terminal vs file tab logic for subsequent workspaces"
  - ref: crates/editor/src/workspace.rs#Editor::new_workspace_without_tab
    implements: "Creates workspace without initial tab for terminal injection"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- welcome_scroll
---

# Chunk Goal

## Minor Goal

When the user creates a **second or later workspace** in the session, its
initial tab should be a terminal tab spawned in the workspace's `root_path`.
The **first workspace of the session** (the startup workspace) continues to
show an empty file tab with the welcome screen, preserving onboarding
discoverability of hotkeys for new users.

This gives experienced users the immediate shell-in-project-directory
experience for every workspace they open after the first, while ensuring new
users still encounter the welcome screen that teaches them key bindings.

The relevant code paths:

1. **`EditorState::add_startup_workspace()`** (`editor_state.rs:388`) — the
   first workspace of every session. **Keep existing behavior** (empty file tab
   / welcome screen). No change needed here.

2. **`EditorState::new_workspace()`** (`editor_state.rs:2625`) — user-triggered
   via the directory picker; always the second workspace or later. Change this
   to open a terminal tab instead of an empty file tab. View dimensions are
   valid at call time, so `new_terminal_tab()` can be called directly after
   workspace creation.

The implementation can distinguish these two cases by checking
`self.editor.workspace_count()` at the point of workspace creation: if it is
already ≥ 1 before adding the new one, a terminal tab should be used.

## Success Criteria

- The startup workspace (first of the session) opens with an empty file tab
  showing the welcome screen — identical to current behavior.
- When the user triggers "New Workspace" (via directory picker) while at least
  one workspace already exists, the new workspace opens with a single terminal
  tab running in the selected directory. No empty file tab / welcome screen.
- The terminal label follows the existing naming convention: "Terminal" for the
  first terminal tab in the workspace.
- Existing tests that verify startup-workspace behavior are unaffected.
- New tests cover the second-workspace-gets-terminal case.

## Rejected Ideas

### Always open a terminal tab for every workspace including the first

The initial design proposed a terminal tab for all workspaces. Rejected because
the startup workspace is the primary onboarding moment: users who have never
run the editor need the welcome screen to discover hotkeys. Forcing them into a
terminal immediately would hide that information with no recovery path short of
closing and re-creating the workspace.