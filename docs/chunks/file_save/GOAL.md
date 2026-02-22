---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::associated_file
    implements: "Getter method accessing the active tab's associated file path"
  - ref: crates/editor/src/editor_state.rs#EditorState::associate_file
    implements: "File loading with UTF-8 lossy conversion, cursor/scroll reset"
  - ref: crates/editor/src/editor_state.rs#EditorState::window_title
    implements: "Derives window title from associated filename or 'Untitled'"
  - ref: crates/editor/src/editor_state.rs#EditorState::save_file
    implements: "Writes buffer content to associated file path"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_selector_confirm
    implements: "Integrates file picker confirmation with associate_file"
  - ref: crates/editor/src/main.rs#EditorController::update_window_title_if_needed
    implements: "Updates NSWindow title when associated file changes"
  - ref: crates/editor/src/main.rs#EditorController::last_window_title
    implements: "Caches window title to avoid redundant NSWindow updates"
narrative: file_buffer_association
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- file_picker
created_after:
- delete_to_line_start
- ibeam_cursor
---

# File-Buffer Association and Cmd+S Save

## Minor Goal

Implement the durable half of file I/O: when the file picker confirms a path, load the file's contents into the buffer (or leave it empty for new files), store the path in `EditorState`, and update the window title. Add Cmd+S to write the buffer back to that path. After this chunk, the editor can open real files and save them — the minimum viable persistence story is complete.

## Success Criteria

- **`associated_file: Option<PathBuf>` field on `EditorState`** (initialised to `None`).

- **`associate_file(path: PathBuf)` method on `EditorState`**:
  1. If the file at `path` exists: read its contents as UTF-8 (replace replacement character `\u{FFFD}` for non-UTF-8 bytes rather than panicking), replace the entire buffer with those contents (`TextBuffer::from_str(&contents)`), reset the cursor to `(0, 0)`, reset the viewport scroll offset to 0.
  2. If the file does not exist (newly created by the file picker): leave the buffer as-is (empty), do not attempt to read.
  3. Store `path` in `associated_file`.
  4. Update the macOS window title via `NSWindow::setTitle_` (or equivalent Objective-C bridge call) to the last path component (filename only). When `associated_file` is `None`, the title should read `"Untitled"`.
  5. Mark `DirtyRegion::FullViewport`.

- **`file_picker` chunk integration**: when the file picker resolves a path (on `Confirmed`), call `state.associate_file(resolved_path)` immediately before closing the overlay.

- **Cmd+S handler** in `EditorState::handle_key`:
  - Event: `Key::Char('s')` with `command: true` and `!control`.
  - If `associated_file` is `None`: no-op (no file to save to; no error shown in this chunk).
  - If `associated_file` is `Some(path)`: write `buffer.content()` as UTF-8 to `path` using `std::fs::write`. On write error: no-op (errors are silently swallowed for now — error reporting is out of scope).
  - Cmd+S does NOT modify the buffer or move the cursor.
  - Cmd+S does NOT mark any dirty region (the buffer content is unchanged visually).

- **No autosave**: the file is only written on explicit Cmd+S.

- **Unit tests**:
  - `associate_file` with an existing file: buffer content matches file, cursor at `(0, 0)`, `associated_file` is `Some`.
  - `associate_file` with a non-existent path: buffer unchanged, `associated_file` is `Some`.
  - Cmd+S with `associated_file == None`: no-op, buffer unchanged.
  - Cmd+S with `associated_file == Some(path)`: file on disk contains buffer content after the call.
  - Cmd+S does not set the dirty flag.
  - Replacing the buffer via `associate_file` correctly resets scroll offset and cursor.
