---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/tab_bar.rs
- crates/editor/src/editor_state.rs
code_references:
- ref: "crates/editor/src/tab_bar.rs#TAB_DIRTY_INACTIVE_COLOR"
  implements: "Dim red tint color constant for inactive dirty tabs"
- ref: "crates/editor/src/tab_bar.rs#TAB_DIRTY_ACTIVE_COLOR"
  implements: "Dim red tint color constant for active dirty tabs"
- ref: "crates/editor/src/tab_bar.rs#TabBarGlyphBuffer::update"
  implements: "Tab background color selection based on dirty state (phases 2 and 3)"
- ref: "crates/editor/src/editor_state.rs#EditorState::handle_key"
  implements: "Sets tab.dirty = true after buffer mutations via dirty_region heuristic"
- ref: "crates/editor/src/editor_state.rs#EditorState::save_file"
  implements: "Clears tab.dirty = false on successful file write"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- tiling_workspace_integration
- workspace_dir_picker
- workspace_identicon
---
# Chunk Goal

## Minor Goal

When an editor buffer tab has unsaved changes, its background color should be tinted with a very dim red to provide an at-a-glance visual cue that the buffer is dirty.

**Current state:** The `Tab` struct in `workspace.rs` has a `dirty: bool` field, and the tab bar rendering in `tab_bar.rs` reads `is_dirty` to show a yellow indicator dot (`DIRTY_INDICATOR_COLOR`). However, **no production code ever sets `tab.dirty = true`** — the flag is initialized to `false` and never flipped when the user edits a buffer. The underlying buffer (`lite-edit-buffer` crate) also does not track a "modified since last save" state. This means the entire dirty-indicator system is dead code.

This chunk needs to:

1. **Wire up the dirty flag:** After any mutation in `handle_key_buffer` (character insert, delete, paste, etc.), set the active tab's `dirty` flag to `true`. Clear it back to `false` on successful save.
2. **Add dim red background tint:** When `is_dirty` is true, render the tab background with a very dim red tint instead of the normal background color — for both active and inactive tab states.
3. **Keep the existing yellow indicator dot** as an additional signal (it will now actually appear since the flag is wired up).

This supports the project goal of a responsive, native editing experience by giving users immediate visual feedback about buffer state.

## Success Criteria

- Editing a file buffer sets `tab.dirty = true` on the active tab
- Saving a file clears `tab.dirty` back to `false`
- Dirty tabs render with a very dim red background tint, distinct from clean tabs
- Both active and inactive dirty tabs have appropriate tinted variants (active still distinguishable from inactive)
- The red tint is subtle/dim — noticeable but not distracting, consistent with the Catppuccin Mocha dark theme
- Clean tabs continue to render with their existing background colors unchanged
- The existing yellow dirty indicator dot now appears correctly alongside the tinted background
- Unit tests verify: dirty flag is set on edit, cleared on save, and dirty tabs use tinted background colors