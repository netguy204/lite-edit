---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::try_buffer
    implements: "Safe Option-returning accessor for TextBuffer that returns None for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::try_buffer_mut
    implements: "Safe mutable Option-returning accessor for TextBuffer that returns None for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::active_tab_is_file
    implements: "Cheap check for code paths that need to early-return on non-file tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_size
    implements: "Guards viewport size updates to handle terminal tabs with 0 line count"
  - ref: crates/editor/src/editor_state.rs#EditorState::update_viewport_dimensions
    implements: "Guards viewport dimension updates to handle terminal tabs with 0 line count"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_cmd_f
    implements: "Guards find-in-file to no-op for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::run_live_search
    implements: "Guards live search to early-return for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::advance_to_next_match
    implements: "Guards search advancement to early-return for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::toggle_cursor_blink
    implements: "Handles cursor blink for both file and terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::cursor_dirty_region
    implements: "Returns FullViewport for terminal tabs where cursor is part of the grid"
  - ref: crates/editor/src/editor_state.rs#EditorState::associate_file
    implements: "Guards file association to no-op for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#EditorState::save_file
    implements: "Guards save operation to no-op for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#test_active_tab_is_file
    implements: "Tests that active_tab_is_file correctly identifies tab types"
  - ref: crates/editor/src/editor_state.rs#test_try_buffer_on_terminal_tab
    implements: "Tests that try_buffer returns None for terminal tabs"
  - ref: crates/editor/src/editor_state.rs#test_terminal_tab_save_no_panic
    implements: "Tests that Cmd+S doesn't panic on terminal tabs"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on:
- terminal_tab_spawn
created_after:
- file_search_path_matching
- terminal_tab_spawn
---

# Chunk Goal

## Minor Goal

After spawning a terminal tab via `Cmd+Shift+T`, the editor crashes with `"active tab is not a file tab"` at `editor_state.rs:120`. The `buffer()` and `buffer_mut()` helpers unconditionally call `.as_text_buffer().expect(...)`, but many code paths (key handling, scrolling, search, rendering prep) call these helpers even when the active tab is a terminal tab. This chunk makes all text-editing code paths safe when the active tab is not a file tab.

The core problem: `EditorState` has dozens of call sites that go through `buffer()` / `buffer_mut()` to access the active tab's `TextBuffer`. When a terminal tab is active, these calls panic. The fix must either guard each call site to no-op when the active tab is a terminal, or restructure the helpers to return `Option` and propagate accordingly.

## Success Criteria

- Opening the editor and pressing `Cmd+Shift+T` to spawn a terminal tab does not crash
- While a terminal tab is active, text-editing keybindings (typing, cursor movement, selection, search, etc.) are harmlessly ignored or appropriately handled
- Switching back to a file tab (`Cmd+1`, clicking, etc.) restores normal editing behavior
- No panics from `buffer()` or `buffer_mut()` regardless of which tab type is active
- All existing tests continue to pass