---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/buffer/src/types.rs
  - crates/buffer/src/text_buffer.rs
  - crates/buffer/src/gap_buffer.rs
  - crates/syntax/src/edit.rs
  - crates/editor/src/context.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/workspace.rs
code_references:
  - ref: crates/buffer/src/types.rs#MutationResult
    implements: "Bundle DirtyLines with EditInfo for tracked mutations"
  - ref: crates/buffer/src/types.rs#EditInfo
    implements: "Byte-offset information for tree-sitter InputEdit construction"
  - ref: crates/buffer/src/types.rs#EditInfo::for_insert
    implements: "Factory method for insertion edit info"
  - ref: crates/buffer/src/types.rs#EditInfo::for_delete
    implements: "Factory method for deletion edit info"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::byte_offset_at
    implements: "Convert (line, col) position to byte offset for tree-sitter"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::byte_len
    implements: "Total byte length for tree-sitter buffer bounds"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::insert_char_tracked
    implements: "Tracked character insertion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::insert_str_tracked
    implements: "Tracked string insertion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_backward_tracked
    implements: "Tracked backward deletion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_forward_tracked
    implements: "Tracked forward deletion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_selection_tracked
    implements: "Tracked selection deletion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_backward_word_tracked
    implements: "Tracked backward word deletion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_forward_word_tracked
    implements: "Tracked forward word deletion returning MutationResult"
  - ref: crates/buffer/src/text_buffer.rs#TextBuffer::delete_to_line_start_tracked
    implements: "Tracked delete-to-line-start returning MutationResult"
  - ref: crates/syntax/src/edit.rs#EditEvent
    implements: "Tree-sitter edit event with byte and position coordinates"
  - ref: crates/syntax/src/edit.rs#From<EditInfo>::from
    implements: "Conversion from buffer EditInfo to syntax EditEvent"
  - ref: crates/editor/src/context.rs#EditorContext::edit_info
    implements: "Edit info field for passing tracked mutation data to caller"
  - ref: crates/editor/src/workspace.rs#Tab::notify_edit
    implements: "Incremental syntax tree update via SyntaxHighlighter::edit()"
  - ref: crates/editor/src/editor_state.rs#EditorState::notify_active_tab_edit
    implements: "Route incremental edit events to active tab's highlighter"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key_buffer
    implements: "Key event routing with incremental parse support"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_insert_text
    implements: "Text insertion with incremental parse support"
narrative: null
investigation: treesitter_editing
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- pty_wakeup_reliability
---

# Chunk Goal

## Minor Goal

Switch all tree-sitter parsing from the full-reparse path (`update_source()`) to the incremental `edit()` path. Currently, every buffer mutation triggers a complete re-parse of the file via `SyntaxHighlighter::update_source()`, which passes `None` as the old tree. The incremental path (`SyntaxHighlighter::edit()`) already exists and passes the old tree to tree-sitter for minimal re-parsing (~120µs vs full parse), but it is never called because the editor doesn't construct `EditEvent` values at mutation sites.

This chunk closes that gap by having buffer mutations surface the byte-offset information needed to construct `EditEvent`, then wiring all mutation sites in `editor_state.rs` to call `Tab::notify_edit()` instead of `Tab::sync_highlighter()`.

This is a prerequisite for tree-sitter-based intelligent indent and go-to-definition (see investigation `treesitter_editing`), since those features add query execution to the edit cycle and need the parse to be fast. It also directly supports GOAL.md's <8ms keystroke-to-glyph P99 latency target on large files.

## Success Criteria

- All buffer mutation paths in `editor_state.rs` (`handle_key_buffer`, `handle_insert_text`, `handle_set_marked_text`, `handle_unmark_text`, file-drop insertion) call `Tab::notify_edit()` with a valid `EditEvent` instead of `Tab::sync_highlighter()`
- `Tab::sync_highlighter()` / `SyntaxHighlighter::update_source()` is no longer called on the per-keystroke path (may be retained for initial file open or full-file reload)
- `TextBuffer` mutation methods (`insert_char`, `insert_str`, `delete_backward`, `delete_forward`, `delete_selection`, etc.) return or expose the byte-offset information needed to construct an `EditEvent` (start_byte, old_end_byte, new_end_byte, plus row/col positions)
- Syntax highlighting remains visually correct after the switch (same colors, same viewport behavior)
- Incremental parse time on a 5000+ line file is measurably faster than full reparse (verify with existing benchmarks or a manual timing check)