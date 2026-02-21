---
status: COMPLETED
advances_trunk_goal: "Required Properties: Standard text editor interaction patterns"
proposed_chunks:
  - prompt: "Implement a FileIndex: a stateful, background-threaded file index that recursively walks a root directory, caches discovered paths incrementally, watches the filesystem with the notify crate, and answers fuzzy queries instantly from the in-memory cache. Key behaviours: (1) empty query returns recently-selected files first (persisted across sessions in <root>/.lite-edit-recent) then the rest alphabetically; (2) a cache_version() AtomicU64 counter increments on every cache mutation so the file picker can poll for new results and stream them in while the walk is still running; (3) record_selection(path) updates and persists the recency list. Excludes dotfiles, target/, node_modules/. No macOS APIs, no UI."
    chunk_directory: fuzzy_file_matcher
    depends_on: []
  - prompt: "Implement a reusable SelectorWidget model: a struct that holds a query string (editable), a Vec of display items (strings), and a selected index. Keyboard events: Up/Down move the selection (clamped to list bounds), Enter confirms the current selection, Escape cancels, and any other printable character or Backspace edits the query. Mouse events: clicking an item selects it; double-clicking (or a single click if it matches the current selection) confirms. The widget exposes an enum SelectorOutcome { Pending, Confirmed(usize), Cancelled } returned from handle_key/handle_mouse. No file-system logic, no rendering — pure reusable interaction model designed to serve the file picker, a future command palette, and any other type-to-filter UI."
    chunk_directory: selector_widget
    depends_on: []
  - prompt: "Add Metal rendering for the SelectorWidget overlay. When a SelectorWidget is active, render it as a floating panel centered in the window: an opaque background rect, a top row showing the query string with a blinking cursor, and a scrollable list of item strings below it with the selected item drawn in a highlight color. Reuse the existing glyph atlas for text rendering. The panel should have a fixed width (e.g., 60% of the window width) and a maximum height (e.g., 50% of the window height) with clipping for long lists. Wire the dirty-region system so the overlay is redrawn when the widget state changes."
    chunk_directory: selector_rendering
    depends_on: [1]
  - prompt: "Wire the file picker: Cmd+P opens a SelectorWidget overlay populated with filenames from the fuzzy file matcher scanning the process's current working directory. As the user types, re-query the file matcher and update the widget's item list. Arrow keys and mouse clicks navigate. Enter confirms: if the selected (or typed) filename exists, associate it with the buffer; if it doesn't exist, create an empty file at that path and associate it. Escape dismisses the overlay without changing the association. Introduce a focus enum (EditorFocus::Buffer | EditorFocus::Selector) in EditorState so that key and mouse events are routed to the correct target while the overlay is open."
    chunk_directory: file_picker
    depends_on: [0, 1, 2]
  - prompt: "Implement file-buffer association and Cmd+S save. Store an Option<PathBuf> in EditorState representing the file the current buffer is associated with. When the file picker confirms a path: if the file exists, replace the buffer contents with the file's UTF-8 contents; if it's a new file, leave the buffer empty and create the file on disk. Add Cmd+S handling in EditorState: when a path is associated, write the buffer's full content to that path (overwriting). Update the macOS window title to show the filename (just the last path component) when a file is associated, or 'Untitled' when not. No autosave — only explicit Cmd+S triggers a write."
    chunk_directory: file_save
    depends_on: [3]
created_after:
  - editor_ux_refinements
---

## Advances Trunk Goal

This narrative advances the standard text editor interaction patterns required for the editor to be useful beyond ephemeral scratch-pad use. Without the ability to open, associate, and save files, nothing the user types is durable. The file picker, selector widget, and Cmd+S save together constitute the minimum viable file I/O story.

## Driving Ambition

lite-edit currently operates entirely in memory — there is no way to open a file, and nothing is ever written to disk. This narrative adds the first durable interaction: associating the buffer with a file on disk.

The entry point is **Cmd+P**, which opens a **complete-as-you-type file picker** — a floating overlay that shows a filtered list of files in the current directory as the user types. The user can navigate the list with arrow keys or mouse clicks. Pressing Enter opens the highlighted file (initializing the buffer from its contents) or, if the typed name doesn't match an existing file, creates a new empty file at that path. Pressing Escape dismisses the picker without changing anything.

Once a file is associated with the buffer, **Cmd+S** writes the buffer contents back to disk.

The complete-as-you-type selector is explicitly designed as a **reusable widget** — the same model and rendering will serve a future command palette, and potentially a mini-buffer or other picker surfaces. The `SelectorWidget` struct knows nothing about files; it only knows about items, queries, and selection. The file picker wires it to the file system.

## Chunks

1. **Fuzzy file matcher (`FileIndex`)** — Pure Rust: recursively walk a directory on a background thread, stream discovered paths into an in-memory cache, watch the filesystem with `notify` (FSEvents on macOS) to keep the cache live, and score against the cache on demand without blocking. The picker calls `query()` on every keystroke and gets instant results from whatever has been cached so far. No UI, no macOS APIs.

2. **Selector widget model** — Reusable `SelectorWidget` struct: owns a query string, a list of items, and the selected index. Returns a `SelectorOutcome` (Pending / Confirmed / Cancelled) from key and mouse event handlers. Designed to be generic — knows nothing about files, commands, or rendering.

3. **Selector overlay rendering** — Metal rendering for a `SelectorWidget`: a floating panel with a query-input row at the top, a filtered item list below, and the selected item highlighted. Uses the existing glyph atlas. Integrates with the dirty-region system.

4. **File picker (Cmd+P)** — Wires selector widget + file matcher together behind Cmd+P. Adds a focus enum to EditorState to route input to the overlay while it's open. Handles create-if-absent: if the entered name doesn't exist, create an empty file.

5. **File-buffer association and Cmd+S** — Stores `Option<PathBuf>` in EditorState. On file picker confirmation, initialize buffer from file contents (or leave empty for new files). Cmd+S writes buffer to disk. Window title reflects the associated filename.

## Completion Criteria

When complete, a user can:
- Press Cmd+P to open a file picker that filters files in the current directory as they type
- Navigate picker results with arrow keys or mouse clicks
- Press Enter to open the selected file (loading its contents into the buffer) or create a new file if the name is new
- Press Escape to dismiss the picker without changes
- Press Cmd+S to save the buffer's contents back to the associated file
- See the filename reflected in the window title when a file is associated
- Trust that nothing is written to disk except on explicit Cmd+S

The `SelectorWidget` is self-contained and reusable — future features (command palette, etc.) can drop it in without re-implementing the query/navigate/confirm interaction model.
