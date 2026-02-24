---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/Cargo.toml
- crates/editor/src/session.rs
- crates/editor/src/main.rs
- crates/editor/src/lib.rs
- crates/editor/tests/session_persistence.rs
code_references:
  - ref: crates/editor/src/session.rs#SessionData
    implements: "Root session data structure for serialization"
  - ref: crates/editor/src/session.rs#SessionData::from_editor
    implements: "Extracts serializable state from live editor model"
  - ref: crates/editor/src/session.rs#SessionData::restore_into_editor
    implements: "Restores Editor from deserialized session data"
  - ref: crates/editor/src/session.rs#WorkspaceData
    implements: "Serializable workspace representation with root path and pane layout"
  - ref: crates/editor/src/session.rs#PaneLayoutData
    implements: "Serializable pane tree structure (leaf/split variants)"
  - ref: crates/editor/src/session.rs#PaneData
    implements: "Serializable pane with tabs and active tab index"
  - ref: crates/editor/src/session.rs#TabData
    implements: "Serializable file tab (absolute path only)"
  - ref: crates/editor/src/session.rs#session_file_path
    implements: "Platform-specific session file location (~/Library/Application Support/lite-edit/)"
  - ref: crates/editor/src/session.rs#save_session
    implements: "Atomic write of session to disk"
  - ref: crates/editor/src/session.rs#load_session
    implements: "Graceful loading with schema version validation"
  - ref: crates/editor/src/main.rs#AppDelegate::application_will_terminate
    implements: "Save session on clean application exit"
  - ref: crates/editor/src/main.rs#AppDelegate::has_cli_argument
    implements: "CLI override detection to skip session restoration"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_window
    implements: "Session restoration or directory picker on startup"
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

When the application exits, the current ordered list of workspaces should be
saved to disk. When the application next launches, those workspaces should be
restored — in the same order — so the user continues where they left off.

This directly supports the "minimal footprint / fast startup" property of the
project: a fast cold start is only truly useful if it brings the user back to
their exact prior state rather than an empty slate.

## Success Criteria

- On clean application exit, a session file is written to disk (location TBD
  at planning time, likely `~/.config/lite-edit/session.json` or the macOS app
  support directory). The file captures, per workspace:
  - The workspace root directory path
  - The pane layout (split structure, orientation, and sizes)
  - For each pane: the ordered list of open file-backed tabs (by absolute path)
    and which tab was active
  - Which workspace was active at exit
- On next launch, if a session file exists, the application restores all
  workspaces in the same order, with each workspace's pane layout and
  file-backed tabs reopened. The previously active workspace and tab are
  focused.
- Terminal tabs are **not** restored (terminals cannot be meaningfully
  serialized); they are dropped silently.
- If no session file exists (first launch or deleted), the existing startup
  behavior (welcome screen / workspace dialog) is preserved unchanged.
- If a workspace root directory no longer exists on disk, that workspace is
  silently skipped.
- If an individual saved file path no longer exists on disk, that tab is
  silently skipped; other tabs in the pane are still restored.
- The session file is overwritten on each clean exit; there is no history or
  rolling backup.

## Rejected Ideas