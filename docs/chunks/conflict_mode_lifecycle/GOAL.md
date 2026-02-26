---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/tab_bar.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/drain_loop.rs
code_references:
  - ref: crates/editor/src/workspace.rs#Tab::conflict_mode
    implements: "Conflict mode flag on Tab struct (default false)"
  - ref: crates/editor/src/editor_state.rs#EditorState::is_tab_in_conflict_mode
    implements: "Check if tab is in conflict mode by path"
  - ref: crates/editor/src/editor_state.rs#EditorState::merge_file_tab
    implements: "Sets conflict_mode = true when merge produces conflicts"
  - ref: crates/editor/src/editor_state.rs#EditorState::save_file
    implements: "Clears conflict_mode on save and re-checks disk for changes during conflict resolution"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_file_changed
    implements: "Suppresses FileChanged events for tabs in conflict mode"
  - ref: crates/editor/src/tab_bar.rs#CONFLICT_INDICATOR_COLOR
    implements: "Distinct visual indicator color (Catppuccin red/pink) for conflict mode tabs"
  - ref: crates/editor/src/tab_bar.rs#TabInfo::is_conflict
    implements: "TabInfo field to propagate conflict_mode to rendering"
  - ref: crates/editor/src/tab_bar.rs#TabBarGlyphBuffer::update
    implements: "Renders conflict indicator with priority over dirty/unread indicators"
narrative: null
investigation: concurrent_edit_sync
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- three_way_merge
created_after:
- emacs_keybindings
- terminal_close_guard
- welcome_file_backed
---

# Chunk Goal

## Minor Goal

Implement the conflict mode lifecycle: when a three-way merge produces conflicts, the tab enters conflict mode which suppresses further auto-merge until the user saves. This completes the concurrent-edit UX by handling the conflict case gracefully.

The mental model is: "conflicts pause auto-sync. Save to resume." When the user saves (Cmd+S), conflict mode clears, the base snapshot updates to the saved content, and auto-merge resumes. If the disk has changed again while in conflict mode, a new merge cycle triggers immediately after save.

Add a visual indicator on the tab so the user can see at a glance that a tab has unresolved conflicts.

## Success Criteria

- `Tab` has a `conflict_mode: bool` field (default false)
- When `three_way_merge` returns a conflict result, `conflict_mode` is set to true
- While `conflict_mode == true`, incoming `FileChanged` events for that tab are ignored (no reload, no merge)
- When the user saves (Cmd+S) a tab in conflict mode:
  - `conflict_mode` is set to false
  - `base_content` is updated to the saved content
  - If the disk content differs from the saved content (external edit arrived during conflict resolution), a new merge cycle triggers
- The tab bar renders a distinct visual indicator when `conflict_mode == true` (e.g., a different color or icon distinguishing it from the normal dirty indicator)
- Closing a tab in conflict mode follows the existing dirty-close confirm dialog behavior (no special handling needed)
- The dirty flag remains true throughout the conflict lifecycle (conflict markers are unsaved edits)

## Rejected Ideas

### Auto-detect conflict marker removal

We could scan the buffer for `<<<<<<<` / `>>>>>>>` patterns and automatically exit conflict mode when all markers are removed, without requiring the user to save.

Rejected because: Adds complexity (when to scan? false positives in markdown/docs about git?) for marginal UX gain. Save is already the natural "I'm done" gesture and has the important side effect of updating the base snapshot, which is necessary for subsequent merges to work correctly.

### Explicit resolve keybinding

We could add a dedicated keybinding (e.g., Cmd+Shift+M) to signal conflict resolution.

Rejected because: Adds a new concept the user must learn. Save already does what we need — clears dirty state, updates the base snapshot, and is universally understood.