---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::new
    implements: "Eager FileIndex initialization at app startup"
  - ref: crates/editor/src/main.rs#EditorController::handle_key
    implements: "tick_picker call on keystroke for streaming refresh"
  - ref: crates/editor/src/main.rs#EditorController::handle_mouse
    implements: "tick_picker call on mouse event for streaming refresh"
  - ref: crates/editor/src/main.rs#EditorController::handle_scroll
    implements: "tick_picker call on scroll event for streaming refresh"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- tab_bar_content_clip
- click_scroll_fraction_alignment
---

# Chunk Goal

## Minor Goal

The file picker (Cmd+P) opens showing only a single file — the lone entry from the
persisted recency list — because `FileIndex` is initialized lazily at first Cmd+P
press.  The background directory walk has had zero time to populate the cache, so
`query("")` returns only recency entries.  The full list doesn't appear until the
500 ms blink timer fires `tick_picker`, which the user perceives as "any event
triggers it" because they naturally interact before half a second has passed.

Fix this by:

1. **Eager `FileIndex` start** — start `FileIndex` when `EditorState` is constructed
   (i.e. at app startup), not lazily inside `open_file_picker`.  The walk runs in the
   background and, in practice, completes long before the user ever presses Cmd+P.

2. **Poll `tick_picker` on every event** — call `tick_picker` in `handle_key`,
   `handle_mouse`, and `handle_scroll` in `EditorController` (in addition to the
   existing blink-timer call).  This guarantees that any cache update that arrived
   between blink ticks is surfaced immediately, without the user noticing a delay.

## Success Criteria

- Opening the file picker (Cmd+P) immediately shows the full list of files available in
  the index at that moment; it does not show only recency entries while the walk runs.
- If the walk is still in progress when the picker opens, items stream in promptly as the
  cache grows — no event interaction required.
- `tick_picker` is called on every key, mouse, and scroll event while the picker is open,
  in addition to the blink timer.
- The `file_index` field on `EditorState` is initialized at construction time with the
  current working directory; `open_file_picker` no longer conditionally creates it.
- All existing tests continue to pass.