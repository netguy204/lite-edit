---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/renderer.rs
- crates/editor/src/selector_overlay.rs
- crates/editor/src/main.rs
code_references:
- ref: crates/editor/src/editor_state.rs#EditorFocus::FindInFile
  implements: "Focus variant for find-in-file mode"
- ref: crates/editor/src/editor_state.rs#EditorState::find_mini_buffer
  implements: "MiniBuffer for the find query input"
- ref: crates/editor/src/editor_state.rs#EditorState::search_origin
  implements: "Buffer position from which the search started"
- ref: crates/editor/src/editor_state.rs#EditorState::handle_cmd_f
  implements: "Opens find strip on Cmd+F"
- ref: crates/editor/src/editor_state.rs#EditorState::close_find_strip
  implements: "Closes find strip and restores focus"
- ref: crates/editor/src/editor_state.rs#EditorState::find_next_match
  implements: "Case-insensitive forward substring search with wrap-around"
- ref: crates/editor/src/editor_state.rs#EditorState::handle_key_find
  implements: "Key routing for find mode (Escape, Enter, input)"
- ref: crates/editor/src/editor_state.rs#EditorState::run_live_search
  implements: "Live search and buffer selection update"
- ref: crates/editor/src/editor_state.rs#EditorState::advance_to_next_match
  implements: "Enter advances to next match"
- ref: crates/editor/src/selector_overlay.rs#FindStripGeometry
  implements: "Layout geometry for the find strip"
- ref: crates/editor/src/selector_overlay.rs#calculate_find_strip_geometry
  implements: "Calculate find strip positioning"
- ref: crates/editor/src/selector_overlay.rs#FindStripGlyphBuffer
  implements: "Glyph buffer for rendering find strip"
- ref: crates/editor/src/renderer.rs#Renderer::draw_find_strip
  implements: "Render the find strip at the bottom of the viewport"
narrative: minibuffer
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- mini_buffer_model
created_after:
- text_buffer
- buffer_view_trait
- file_picker_scroll
- line_wrap_rendering
---

# Chunk Goal

## Minor Goal

Add a live find-in-file feature (Cmd+F) that uses `MiniBuffer` as its input.
The user types a search query; the editor selects and scrolls to the nearest
forward match in the main buffer in real time while the minibuffer strip retains
focus. Enter advances to the next match; Escape dismisses.

## Success Criteria

### Focus model

- **`EditorFocus`** gains a new variant `FindInFile`.
- **Cmd+F** while `focus == Buffer` opens the find strip: creates a `MiniBuffer`,
  records the cursor position at open time (`search_origin`), transitions
  `focus` to `FindInFile`, marks `DirtyRegion::FullViewport`.
- **Cmd+F** while `focus == FindInFile` is a no-op (does not close or reset).
- **Escape** while `focus == FindInFile` closes the strip, restores
  `focus = Buffer`, leaves the main buffer's cursor and selection at the
  last-matched position, marks dirty.
- **Key routing**: when `focus == FindInFile`, all key events go to the find
  handler (not the buffer or selector). Mouse events and scroll events route
  to the buffer as if it were focused (the user can scroll the buffer while
  searching).

### Live search

- After every key event that changes the minibuffer's content, run
  `find_next_match(query, start_pos)` on the main `TextBuffer`:
  - Search forward from `search_origin`, wrapping around at the end of the
    buffer.
  - Case-insensitive, substring match on the raw content string.
  - If a match is found: set the main buffer's selection to cover the match
    range and scroll the viewport to make it visible. The minibuffer still owns
    focus.
  - If no match is found: clear the main buffer's selection.
- The search origin is fixed at the cursor position when Cmd+F was pressed; it
  does not advance as content changes (only Enter advances it — see below).

### Enter: advance to next match

- **Enter** while `focus == FindInFile`:
  - Advances `search_origin` to one character past the end of the current match.
  - Runs `find_next_match` from the new origin (wrapping as needed).
  - Selects and scrolls to the new match.
  - Does **not** close the strip.

### Find strip rendering

- The find strip is a one-line-tall bar anchored to the **bottom** of the
  viewport (not a floating overlay).
- It renders: a dim label `find:` followed by the `MiniBuffer` content
  (characters left-to-right) and a blinking cursor at the query cursor position.
- The strip uses the same background color as the selector overlay panel.
- The cursor blinks on the same timer as the main buffer cursor.
- Rendering is handled in `renderer.rs` via a new `render_with_find_strip`
  path (analogous to `render_with_selector`), with layout helpers in
  `selector_overlay.rs` or a dedicated section of that file.

### `EditorState` additions

- `find_mini_buffer: Option<MiniBuffer>` — holds the active query input.
- `search_origin: Position` — the buffer position from which the current
  search session started.
- `handle_cmd_f(&mut self)` — opens the find strip.
- `handle_key_find(&mut self, event: KeyEvent)` — routes keys:
  - `Key::Escape` → `close_find_strip()`.
  - `Key::Return` → advance origin and re-search.
  - Everything else → `mini_buffer.handle_key(event)`; if content changed,
    re-run the live search.
- `close_find_strip(&mut self)` — clears `find_mini_buffer`, resets focus to
  `Buffer`, marks dirty.

### Unit tests

- Cmd+F transitions focus to `FindInFile` and sets `find_mini_buffer`.
- Escape dismisses and returns focus to `Buffer`.
- Typing into the strip triggers a match selection in the main buffer
  (test via inspecting `buffer.selection_range()` after a key event).
- Enter advances the search origin past the current match.
- Cmd+F while already open is a no-op (focus remains `FindInFile`, strip
  remains open).
- No match found → buffer selection is cleared.