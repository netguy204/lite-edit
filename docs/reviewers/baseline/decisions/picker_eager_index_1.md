---
decision: APPROVE
summary: All success criteria satisfied; eager FileIndex initialization and per-event tick_picker polling correctly implement the fix.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Opening the file picker (Cmd+P) immediately shows the full list of files available in the index at that moment

- **Status**: satisfied
- **Evidence**: `EditorState::new()` (lines 224-229 in editor_state.rs) initializes `file_index` with `Some(FileIndex::start(cwd))` at construction time. When `open_file_picker()` runs, it immediately queries this pre-populated index with `query("")` to populate the selector widget. The background walk starts at app startup, giving it time to populate before Cmd+P is pressed.

### Criterion 2: If the walk is still in progress when the picker opens, items stream in promptly as the cache grows

- **Status**: satisfied
- **Evidence**: `tick_picker()` is now called on every key, mouse, and scroll event (see Criteria 3). This polls the `FileIndex` for cache updates and refreshes the selector items when the cache version changes. Combined with the existing blink-timer polling, updates stream in promptly without requiring user to wait for the 500ms timer.

### Criterion 3: `tick_picker` is called on every key, mouse, and scroll event while the picker is open, in addition to the blink timer

- **Status**: satisfied
- **Evidence**:
  - `handle_key()` (main.rs:224-229): Calls `tick_picker()` and merges dirty region
  - `handle_mouse()` (main.rs:238-243): Calls `tick_picker()` and merges dirty region
  - `handle_scroll()` (main.rs:254-259): Calls `tick_picker()` and merges dirty region
  - The existing `toggle_cursor_blink()` continues to call `tick_picker()` (main.rs:288)

### Criterion 4: The `file_index` field on `EditorState` is initialized at construction time with the current working directory; `open_file_picker` no longer conditionally creates it

- **Status**: satisfied
- **Evidence**:
  - In `EditorState::new()` (editor_state.rs:227-229), `file_index` is initialized as `Some(FileIndex::start(std::env::current_dir().unwrap_or_else(...)))`
  - In `open_file_picker()` (editor_state.rs:447-450), the conditional `if self.file_index.is_none() { ... }` block was removed and replaced with a comment explaining eager initialization

### Criterion 5: All existing tests continue to pass

- **Status**: satisfied
- **Evidence**: `cargo test` passes 553 tests in the editor crate. Two performance tests in `lite-edit-buffer` fail, but these are pre-existing failures unrelated to this chunk (verified by running tests on parent commit d686d344^ which shows the same failures). The buffer crate was not modified by this chunk.
