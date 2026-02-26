---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/context.rs
- crates/editor/src/buffer_target.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/context.rs#EditorContext
    implements: "Added content_mutated field to track content mutations"
  - ref: crates/editor/src/context.rs#EditorContext::set_content_mutated
    implements: "Helper method to mark that a content mutation occurred"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Calls set_content_mutated() for mutating commands (insert, delete, paste, cut)"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key_buffer
    implements: "Uses content_mutated flag instead of dirty_region heuristic to gate tab.dirty"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- fallback_glyph_metrics
---

# Chunk Goal

## Minor Goal

Non-mutating keyboard operations (arrow key navigation, Command+A select all) incorrectly set `tab.dirty = true`. This causes the unsaved-changes indicator to appear even though no content was modified. Mouse-based operations (clicking, scrolling) do not exhibit this bug because they follow a separate code path that lacks the faulty heuristic.

The root cause is in `editor_state.rs` `handle_key_buffer()` (lines 1717-1728): after processing any key event, the code checks `self.dirty_region.is_dirty()` and marks the tab dirty if true. But `dirty_region` is a **rendering** concept — arrow keys mark cursor lines dirty for repaint, and Command+A marks the full viewport dirty for selection highlighting. Neither operation mutates buffer content, yet the heuristic treats all dirty-region activity as content mutation.

The fix must distinguish content-mutating operations (character insert, delete, paste, cut) from non-mutating operations (cursor movement, selection changes, scrolling) when deciding whether to set `tab.dirty`.

## Success Criteria

- Arrow key navigation (up, down, left, right) does not set `tab.dirty`
- Command+A (select all) does not set `tab.dirty`
- Shift+arrow selection does not set `tab.dirty`
- Content-mutating operations (typing, delete, backspace, paste, cut) still correctly set `tab.dirty`
- The unsaved-changes indicator (tab tint) only appears after actual content modification
- Existing tests continue to pass