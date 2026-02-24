---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/file_picker.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/lib.rs
  - crates/editor/src/main.rs
  - crates/editor/src/welcome_screen.rs
code_references:
  - ref: crates/editor/src/file_picker.rs
    implements: "NSOpenPanel humble object wrapper with test mock infrastructure"
  - ref: crates/editor/src/file_picker.rs#pick_file
    implements: "Opens macOS native file picker dialog, returns selected file path"
  - ref: crates/editor/src/file_picker.rs#mock_set_next_file
    implements: "Test mock for isolating unit tests from NSOpenPanel"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_cmd_o
    implements: "Cmd+O handler that calls pick_file and associate_file"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- dragdrop_file_paste
- vsplit_scroll
- workspace_initial_terminal
- workspace_session_persistence
---

# Chunk Goal

## Minor Goal

Add a `Cmd+O` keyboard shortcut that opens the macOS native system file picker
(NSOpenPanel) and loads the selected file into the active tab, replacing its
current content and file association. This gives users a standard macOS
"Open File" experience alongside the fuzzy workspace file picker (`Cmd+P`),
allowing them to open any file on the filesystem — not just files within the
current workspace.

## Success Criteria

- Pressing `Cmd+O` in any focus mode (Buffer, Selector, FindInFile) opens the
  macOS native file picker dialog (NSOpenPanel configured to select files, not
  directories).
- Selecting a file loads it into the active tab: buffer contents are replaced,
  the file path is associated, syntax highlighting is applied, and the viewport
  is reset — identical to what `associate_file` does today.
- Cancelling the dialog leaves the current tab unchanged.
- If the active tab is a terminal tab, `Cmd+O` is a no-op (consistent with how
  `associate_file` already handles terminal tabs).
- A new `file_picker_dialog` module (mirroring `dir_picker.rs`) provides a
  `pick_file() -> Option<PathBuf>` function with a test-mode thread-local mock,
  so no modal dialog is shown during unit tests.
- Unit tests cover: Cmd+O with a mocked file path loads the file into the
  active tab; Cmd+O with a cancelled picker (mock returns None) leaves the tab
  unchanged.

## Rejected Ideas

### Reuse the existing `dir_picker` module

The `dir_picker` module could be extended to optionally pick files. Rejected
because the two behaviors require different NSOpenPanel configuration
(`setCanChooseFiles`/`setCanChooseDirectories`) and mixing them into one module
would muddy the interface. A dedicated module keeps the humble-object pattern
clean.

### Open in a new tab instead of replacing the current tab

Could always open the selected file in a new tab (like `Cmd+T` then load).
Rejected per operator specification: the intent is to replace the current tab,
matching how `Cmd+P` file picker confirmation works.