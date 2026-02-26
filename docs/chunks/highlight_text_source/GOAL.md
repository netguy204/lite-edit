---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/syntax/src/highlighter.rs
- crates/editor/src/highlighted_buffer.rs
- crates/editor/src/editor_state.rs
code_references:
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::highlight_spans_for_line
    implements: "Buffer-sourced span generation - decouples text content from highlighter's internal source"
  - ref: crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_spans_with_external_text
    implements: "Internal helper for building spans using externally-provided line text"
  - ref: crates/editor/src/highlighted_buffer.rs#HighlightedBufferView::styled_line
    implements: "Reads line text from buffer (source of truth) and applies highlighter spans"
  - ref: crates/editor/src/highlighted_buffer.rs#HighlightedBufferViewMut::styled_line
    implements: "Mutable view version - same buffer-first text retrieval pattern"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_insert_text
    implements: "Added sync_active_tab_highlighter call after text insertion"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_set_marked_text
    implements: "Added sync_active_tab_highlighter call after IME marked text"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_unmark_text
    implements: "Added sync_active_tab_highlighter call after IME cancellation"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_file_drop
    implements: "Added sync_active_tab_highlighter call after file drop insertion"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- buffer_file_watching
- highlight_injection
---

# Chunk Goal

## Minor Goal

Decouple rendered text content from the syntax highlighter's source copy so that
the buffer is always the source of truth for what text is displayed.

Currently, `HighlightedBufferView::styled_line()` delegates entirely to
`SyntaxHighlighter::highlight_line()`, which reads line text from the
highlighter's internal `self.source` field. When the buffer is mutated via
`handle_insert_text()` (the macOS `insertText:` path for regular keypresses),
the highlighter is not synced — so the renderer draws **stale text** from the
highlighter while positioning the **cursor** from the current buffer state. The
result is that typed characters are invisible until a different action (arrow
keys, Enter) triggers `handle_key_buffer()` → `sync_active_tab_highlighter()`.

Four buffer mutation paths outside `handle_key_buffer` are missing the sync:
- `handle_insert_text()` — regular keyboard input via macOS `insertText:`
- `handle_set_marked_text()` — IME composition
- `handle_unmark_text()` — IME cancellation
- `handle_file_drop()` — drag-and-drop file path insertion

The fix has two parts:

1. **Architectural**: `styled_line()` should read the **text** from the buffer
   (always current) and get only **color/style spans** from the highlighter.
   This makes rendering resilient to highlighter sync gaps — the worst case
   becomes slightly stale colors for one frame, rather than invisible text.

2. **Sync coverage**: All four missing mutation paths should call
   `sync_active_tab_highlighter()` so that highlight colors stay current.
   This is still needed even after the architectural change — without it,
   syntax colors would remain stale until the next `handle_key` action.

## Success Criteria

- Typing characters in a syntax-highlighted file buffer immediately displays
  them without requiring cursor movement or Enter to trigger a re-render.
- `HighlightedBufferView::styled_line()` reads line text from the `TextBuffer`,
  not from `SyntaxHighlighter::source`.
- The highlighter provides span/style information that is applied to the buffer's
  current text content.
- When the highlighter is stale (not yet synced after a mutation), the correct
  text is still rendered — potentially with slightly outdated syntax colors.
- Existing syntax highlighting behavior is preserved for the synced case (colors
  are correct when the highlighter is up to date).
- All four non-`handle_key` mutation paths (`handle_insert_text`,
  `handle_set_marked_text`, `handle_unmark_text`, `handle_file_drop`) call
  `sync_active_tab_highlighter()` so highlight colors stay current.

## Rejected Ideas

### Just add `sync_active_tab_highlighter()` to `handle_insert_text()`

Adding the sync call would fix the immediate symptom but leaves a fragile
architecture: any future code path that mutates the buffer without syncing the
highlighter would reintroduce invisible text. Making the buffer the text source
of truth for rendering eliminates this class of bug entirely.