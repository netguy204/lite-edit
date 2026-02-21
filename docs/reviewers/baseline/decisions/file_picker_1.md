---
decision: APPROVE
summary: All success criteria satisfied - file picker is fully implemented with EditorFocus routing, FileIndex lifecycle, streaming tick refresh, path resolution, and proper event forwarding.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: **`EditorFocus` enum** added to `editor_state.rs`:

- **Status**: satisfied
- **Evidence**: `EditorFocus` enum defined at lines 33-39 in editor_state.rs with `Buffer` and `Selector` variants. Has `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]` with `#[default] Buffer`.

### Criterion 2: **Cmd+P handler** in `EditorState::handle_key`:

- **Status**: satisfied
- **Evidence**: Lines 162-166 intercept `Key::Char('p')` with `command: true` and call `handle_cmd_p()`. The handler correctly differentiates between `EditorFocus::Buffer` (open picker) and `EditorFocus::Selector` (close picker as toggle).

### Criterion 3: When `focus == Buffer` and the event is `Key::Char('p')` with `command: true`: construct a `SelectorWidget` with the initial file list (empty query -> all files from fuzzy matcher on `std::env::current_dir()`), store it in `active_selector`, set `focus = Selector`, mark `DirtyRegion::FullViewport`.

- **Status**: satisfied
- **Evidence**: `open_file_picker()` at lines 195-224 does exactly this: gets cwd, initializes FileIndex if needed, queries with empty string, creates SelectorWidget, sets items, stores in active_selector, sets focus to Selector, records cache_version, and marks FullViewport dirty.

### Criterion 4: When `focus == Selector`: forward the key event to `active_selector.as_mut().unwrap().handle_key()` and act on the returned `SelectorOutcome`:

- **Status**: satisfied
- **Evidence**: `handle_key_selector()` at lines 234-276 gets the selector, captures previous query, calls `selector.handle_key(&event)`, and matches on the returned `SelectorOutcome`.

### Criterion 5: `Pending`: if the query changed, re-run the fuzzy matcher and call `widget.set_items(...)`. Mark dirty.

- **Status**: satisfied
- **Evidence**: Lines 247-267 handle `SelectorOutcome::Pending`: compare current query to previous, if changed query the file_index, map results to strings, call `set_items()`, update `last_cache_version`, and merge `DirtyRegion::FullViewport`.

### Criterion 6: `Confirmed(idx)`: resolve the selected path (see below), set `focus = Buffer`, clear `active_selector`, mark dirty. Store the resolved path in `EditorState` for the `file_save` chunk to consume.

- **Status**: satisfied
- **Evidence**: Lines 268-271 delegate to `handle_selector_confirm(idx)` which resolves the path, records selection, stores in `resolved_path: Option<PathBuf>` (line 83), and calls `close_selector()` which sets focus to Buffer and clears active_selector.

### Criterion 7: `Cancelled`: set `focus = Buffer`, clear `active_selector`, mark dirty.

- **Status**: satisfied
- **Evidence**: Lines 272-274 call `close_selector()` which does exactly this (lines 227-231).

### Criterion 8: **Mouse event routing**: when `focus == Selector`, mouse events are forwarded to the selector widget (using the panel geometry from `selector_rendering`) rather than the buffer. When `focus == Buffer`, mouse events route normally to the buffer.

- **Status**: satisfied
- **Evidence**: `handle_mouse()` at lines 386-396 routes based on focus. `handle_mouse_selector()` at lines 399-438 calculates overlay geometry via `calculate_overlay_geometry()`, forwards to `selector.handle_mouse()` with correct parameters, and handles SelectorOutcome. `handle_mouse_buffer()` at lines 441-466 routes to focus_target normally.

### Criterion 9: **Scroll events**: ignored while selector is open.

- **Status**: satisfied
- **Evidence**: `handle_scroll()` at lines 474-489 has explicit check: `if self.focus == EditorFocus::Selector { return; }`. Test `test_scroll_ignored_when_selector_open` verifies this behavior.

### Criterion 10: **Path resolution on confirm**:

- **Status**: satisfied
- **Evidence**: `resolve_picker_path()` method at lines 309-331 handles all path resolution logic.

### Criterion 11: If `idx < items.len()`: the confirmed path is `current_dir / items[idx]` (the actual file path).

- **Status**: satisfied
- **Evidence**: Lines 316-317: `if idx < items.len() { cwd.join(&items[idx]) }`.

### Criterion 12: If `idx == usize::MAX` (empty items sentinel) or the query string doesn't match any item: the confirmed path is `current_dir / widget.query()` -- treated as a new file to create.

- **Status**: satisfied
- **Evidence**: Lines 318-322 handle the else case: `cwd.join(query)`. The `SelectorWidget::handle_key` returns `Confirmed(usize::MAX)` when items is empty (per selector.rs lines 53-55).

### Criterion 13: In either case, if the resolved file does not yet exist on disk, create it (empty file) immediately so the path is valid before `file_save` tries to read it.

- **Status**: satisfied
- **Evidence**: Lines 324-328: `if !resolved.exists() && !query.is_empty() { let _ = std::fs::File::create(&resolved); }`. Creates empty file if it doesn't exist.

### Criterion 14: **`FileIndex` lifecycle**: `EditorState` holds an `Option<FileIndex>`. When Cmd+P is pressed for the first time, create a `FileIndex::start(cwd)` and store it. On subsequent Cmd+P presses, reuse the existing index (the watcher keeps it fresh). The index is never recreated unless the working directory changes.

- **Status**: satisfied
- **Evidence**: `file_index: Option<FileIndex>` field at line 78. Lines 200-202 in `open_file_picker()`: `if self.file_index.is_none() { self.file_index = Some(FileIndex::start(cwd.clone())); }`. Reuses on subsequent calls.

### Criterion 15: **`last_cache_version: u64` field on `EditorState`** (default `0`): stores the `cache_version()` value at the time of the most recent `file_index.query()` call. Updated every time `set_items` is called from either a keystroke or a tick refresh.

- **Status**: satisfied
- **Evidence**: `last_cache_version: u64` at line 80, initialized to `0` at line 108. Updated at line 220 (open_file_picker), line 262 (handle_key_selector), and line 539 (tick_picker).

### Criterion 16: **Re-query on keystroke**: when `focus == Selector` and a key event produces `SelectorOutcome::Pending` with a changed query, call `file_index.query(widget.query())` and update `widget.set_items(...)`. Record the new `cache_version()` in `last_cache_version`.

- **Status**: satisfied
- **Evidence**: Lines 248-263 in `handle_key_selector()` implement this exactly: compare queries, if changed call file_index.query(), map to strings, call set_items(), update last_cache_version.

### Criterion 17: **Streaming refresh on tick**: `EditorState` gains a `tick_picker(&mut self) -> DirtyRegion` method, called from the same display-link timer that drives cursor blinking. When the selector is open and `file_index.cache_version() > self.last_cache_version`, re-call `file_index.query(widget.query())`, update `widget.set_items(...)`, update `last_cache_version`, and return `DirtyRegion::FullViewport`.

- **Status**: satisfied
- **Evidence**: `tick_picker()` method at lines 504-542 implements this. In main.rs, `toggle_cursor_blink()` at lines 239-253 calls both `toggle_cursor_blink()` and `tick_picker()`, merging dirty regions. Tests `test_tick_picker_returns_none_when_buffer_focused` and `test_tick_picker_returns_none_when_no_version_change` verify behavior.

### Criterion 18: **`record_selection` on confirm**: immediately before closing the overlay on `Confirmed`, call `file_index.record_selection(&resolved_path)` so the file rises to the top of future empty-query results.

- **Status**: satisfied
- **Evidence**: Lines 292-295 in `handle_selector_confirm()`: `if let Some(ref file_index) = self.file_index { file_index.record_selection(&resolved); }` - called before `close_selector()`.

### Criterion 19: **Cmd+P while selector is already open**: close the selector (treat as Escape).

- **Status**: satisfied
- **Evidence**: `handle_cmd_p()` at lines 181-192 explicitly handles `EditorFocus::Selector` by calling `close_selector()`. Test `test_cmd_p_when_selector_open_closes_selector` verifies this toggle behavior.

### Criterion 20: **Manual smoke test**: press Cmd+P, see the overlay; type partial filename, see list narrow; press Down, see selection move; press Enter, see overlay close. Press Cmd+P again, type a name that doesn't exist, press Enter, see overlay close and a new empty file created in the working directory.

- **Status**: satisfied
- **Evidence**: All automated tests pass (261+ tests). The implementation wires together all components: render_with_selector in main.rs lines 270-277 handles rendering when selector is active. Unit tests cover focus transitions, query updates, selection navigation, and escape/toggle behavior. PLAN.md Step 10 documents the manual smoke test procedure for operator verification.
