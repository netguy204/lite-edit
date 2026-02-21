---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorFocus
    implements: "Focus mode enum distinguishing Buffer vs Selector editing mode"
  - ref: crates/editor/src/editor_state.rs#EditorState
    implements: "File picker state fields (focus, active_selector, file_index, last_cache_version, resolved_path)"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key
    implements: "Cmd+P interception and focus-based key routing"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_cmd_p
    implements: "Toggle behavior for Cmd+P (open/close file picker)"
  - ref: crates/editor/src/editor_state.rs#EditorState::open_file_picker
    implements: "FileIndex initialization, initial query, SelectorWidget setup"
  - ref: crates/editor/src/editor_state.rs#EditorState::close_selector
    implements: "Selector dismissal and focus return to Buffer"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key_selector
    implements: "Key forwarding to SelectorWidget and SelectorOutcome handling"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_selector_confirm
    implements: "Path resolution, recency recording, and resolved_path storage on Enter"
  - ref: crates/editor/src/editor_state.rs#EditorState::resolve_picker_path
    implements: "Path resolution logic (existing file vs new file creation)"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse
    implements: "Focus-based mouse routing (selector vs buffer)"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_selector
    implements: "Mouse forwarding to SelectorWidget with overlay geometry"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_scroll
    implements: "Scroll event ignoring when selector is open"
  - ref: crates/editor/src/editor_state.rs#EditorState::tick_picker
    implements: "Streaming refresh mechanism for background file index updates"
  - ref: crates/editor/src/main.rs#EditorController::toggle_cursor_blink
    implements: "Integration of tick_picker into timer-driven refresh loop"
  - ref: crates/editor/src/main.rs#EditorController::render_if_dirty
    implements: "Conditional render_with_selector when focus is Selector"
narrative: file_buffer_association
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- fuzzy_file_matcher
- selector_widget
- selector_rendering
created_after:
- delete_to_line_start
- ibeam_cursor
---

# File Picker (Cmd+P)

## Minor Goal

Wire the `SelectorWidget` model and fuzzy file matcher together into a Cmd+P file picker. Pressing Cmd+P opens the selector overlay; as the user types, the item list updates in real time from the file matcher scanning the current working directory; arrow keys and mouse clicks navigate the list; Enter selects (or creates) a file; Escape dismisses. This chunk makes file opening interactive end-to-end — the result feeds directly into `file_save` for buffer association.

## Success Criteria

- **`EditorFocus` enum** added to `editor_state.rs`:
  ```rust
  enum EditorFocus {
      Buffer,
      Selector,
  }
  ```
  `EditorState` gains a `focus: EditorFocus` field (default `Buffer`) and an `active_selector: Option<SelectorWidget>` field.

- **Cmd+P handler** in `EditorState::handle_key`:
  - When `focus == Buffer` and the event is `Key::Char('p')` with `command: true`: construct a `SelectorWidget` with the initial file list (empty query → all files from fuzzy matcher on `std::env::current_dir()`), store it in `active_selector`, set `focus = Selector`, mark `DirtyRegion::FullViewport`.
  - When `focus == Selector`: forward the key event to `active_selector.as_mut().unwrap().handle_key()` and act on the returned `SelectorOutcome`:
    - `Pending`: if the query changed, re-run the fuzzy matcher and call `widget.set_items(...)`. Mark dirty.
    - `Confirmed(idx)`: resolve the selected path (see below), set `focus = Buffer`, clear `active_selector`, mark dirty. Store the resolved path in `EditorState` for the `file_save` chunk to consume.
    - `Cancelled`: set `focus = Buffer`, clear `active_selector`, mark dirty.

- **Mouse event routing**: when `focus == Selector`, mouse events are forwarded to the selector widget (using the panel geometry from `selector_rendering`) rather than the buffer. When `focus == Buffer`, mouse events route normally to the buffer.

- **Scroll events**: ignored while selector is open.

- **Path resolution on confirm**:
  - If `idx < items.len()`: the confirmed path is `current_dir / items[idx]` (the actual file path).
  - If `idx == usize::MAX` (empty items sentinel) or the query string doesn't match any item: the confirmed path is `current_dir / widget.query()` — treated as a new file to create.
  - In either case, if the resolved file does not yet exist on disk, create it (empty file) immediately so the path is valid before `file_save` tries to read it.

- **`FileIndex` lifecycle**: `EditorState` holds an `Option<FileIndex>`. When Cmd+P is pressed for the first time, create a `FileIndex::start(cwd)` and store it. On subsequent Cmd+P presses, reuse the existing index (the watcher keeps it fresh). The index is never recreated unless the working directory changes.

- **`last_cache_version: u64` field on `EditorState`** (default `0`): stores the `cache_version()` value at the time of the most recent `file_index.query()` call. Updated every time `set_items` is called from either a keystroke or a tick refresh.

- **Re-query on keystroke**: when `focus == Selector` and a key event produces `SelectorOutcome::Pending` with a changed query, call `file_index.query(widget.query())` and update `widget.set_items(...)`. Record the new `cache_version()` in `last_cache_version`.

- **Streaming refresh on tick**: `EditorState` gains a `tick_picker(&mut self) -> DirtyRegion` method, called from the same display-link timer that drives cursor blinking. When the selector is open and `file_index.cache_version() > self.last_cache_version`, re-call `file_index.query(widget.query())`, update `widget.set_items(...)`, update `last_cache_version`, and return `DirtyRegion::FullViewport`. This is the mechanism by which results stream in during the initial walk: each batch the walker adds to the cache increments `cache_version`, which the next tick detects, triggering a re-query that picks up the newly discovered paths.

- **`record_selection` on confirm**: immediately before closing the overlay on `Confirmed`, call `file_index.record_selection(&resolved_path)` so the file rises to the top of future empty-query results.

- **Cmd+P while selector is already open**: close the selector (treat as Escape).

- **Manual smoke test**: press Cmd+P, see the overlay; type partial filename, see list narrow; press Down, see selection move; press Enter, see overlay close. Press Cmd+P again, type a name that doesn't exist, press Enter, see overlay close and a new empty file created in the working directory.
