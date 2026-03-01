---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/drain_loop.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::associate_file
    implements: "Clear styled line cache when buffer is replaced via file association"
  - ref: crates/editor/src/editor_state.rs#EditorState::reload_file_tab
    implements: "Clear styled line cache when buffer is replaced via file reload"
  - ref: crates/editor/src/editor_state.rs#test_associate_file_clears_styled_line_cache
    implements: "Test verifying cache invalidation on associate_file"
  - ref: crates/editor/src/editor_state.rs#test_reload_file_tab_clears_styled_line_cache
    implements: "Test verifying cache invalidation on reload_file_tab"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: uses
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_spawn_reliability
- treesitter_gotodef_type_resolution
---

# Chunk Goal

## Minor Goal

Clear the styled line cache when buffer content is replaced by file reload or
file association, so that the renderer displays the actual buffer content
instead of stale cached lines.

The `StyledLineCache` (owned by `GlyphBuffer` on the `Renderer`) is a single
global cache indexed by buffer line number. It is correctly cleared on tab
switch (`switch_tab` sets `clear_styled_line_cache = true`) but is **not**
cleared when the buffer content is replaced in-place via:

1. **`reload_file_tab()`** — called when a `FileChanged` event arrives for a
   clean tab. Replaces the buffer with `*buffer = TextBuffer::from_str(...)`,
   but `TextBuffer::from_str()` initializes with `dirty_lines: DirtyLines::None`,
   so no dirty lines are reported and the cache serves old rendered lines.

2. **`associate_file()`** — called when the file picker confirms a selection
   or Cmd+O opens a file. Same issue: buffer is replaced but cache is not
   invalidated. Since the file picker operates on the current active tab
   (no tab switch occurs), the tab-switch cache clear is never triggered.

This causes two user-visible symptoms:
- A file modified externally on disk does not visually update in the editor,
  even though the buffer data is correct (visible via terminal `cat`).
- Closing and reopening the same file still shows stale content, because
  `associate_file()` replaces the buffer without clearing the cache.

This directly supports the project goal of a responsive, correct editing
experience — stale rendering undermines trust in the editor's display.

## Success Criteria

- After an external file modification triggers `reload_file_tab()`, the
  rendered content matches the new file content on the next frame.
- After `associate_file()` loads a file into the current tab, the rendered
  content matches the loaded file content on the next frame.
- The styled line cache is fully cleared (not partially invalidated) in both
  cases, since the entire buffer is replaced and line-level invalidation
  cannot correctly track a wholesale buffer swap.
- Existing tab-switch cache clearing continues to work correctly.
- No performance regression: cache clearing only occurs on buffer replacement,
  not on every frame or every keystroke.

## Root Cause

The render pipeline in `drain_loop.rs:525-532` checks two paths:
1. `take_clear_styled_line_cache()` — if true, clears the entire cache
2. Otherwise, `take_dirty_lines()` — invalidates specific lines

Both `reload_file_tab()` and `associate_file()` replace the buffer but trigger
neither path: they don't set `clear_styled_line_cache = true`, and the fresh
`TextBuffer` reports `DirtyLines::None`.

**Fix approach**: Set `self.clear_styled_line_cache = true` in both
`reload_file_tab()` and `associate_file()` after replacing the buffer. This
uses the existing cache invalidation infrastructure without adding new
mechanisms.