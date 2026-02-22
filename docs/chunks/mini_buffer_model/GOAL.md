---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/mini_buffer.rs
- crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer
    implements: "Single-line editing buffer struct with TextBuffer, Viewport, and font metrics"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::new
    implements: "Constructor creating empty single-line buffer with initialized viewport"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::handle_key
    implements: "Key event handling with single-line invariant enforcement (filters Return, Up, Down)"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::content
    implements: "Accessor returning buffer text as String"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::cursor_col
    implements: "Accessor returning cursor column position"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::selection_range
    implements: "Accessor returning selection as column range Option<(usize, usize)>"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::has_selection
    implements: "Boolean accessor for selection state"
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::clear
    implements: "Resets buffer to empty state, removing content and selection"
narrative: minibuffer
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- text_buffer
- buffer_view_trait
- file_picker_scroll
- line_wrap_rendering
---

# Chunk Goal

## Minor Goal

Introduce `MiniBuffer` — a reusable single-line editing model that provides
the full affordance set of the main editor buffer (word-jump, kill-line,
shift-selection, clipboard, Emacs-style Ctrl bindings) while enforcing a
single-line invariant. It is the shared primitive that the file picker query
field and the find-in-file strip will both build on.

## Success Criteria

- **New file** `crates/editor/src/mini_buffer.rs` added to the module list in
  `crates/editor/src/main.rs`.

- **`MiniBuffer` struct** owns a `TextBuffer` (from `lite_edit_buffer`), a
  `Viewport`, and a `BufferFocusTarget`. All fields are private.

- **`MiniBuffer::new(font_metrics: FontMetrics) -> MiniBuffer`** — creates an
  empty single-line buffer.

- **`MiniBuffer::handle_key(&mut self, event: KeyEvent)`** — delegates to
  `BufferFocusTarget::handle_key` after filtering events that would break the
  single-line invariant:
  - `Key::Return` — no-op (do not insert a newline).
  - `Key::Up`, `Key::Down` — no-op (no multi-line cursor movement).
  - All other keys pass through unchanged.

- **`MiniBuffer::content(&self) -> &str`** (or `-> String` if the gap-buffer
  API only exposes owned values) — the current buffer text, always a single
  line containing no `\n`.

- **`MiniBuffer::cursor_col(&self) -> usize`** — the cursor's column position
  (`buffer.cursor_position().col`).

- **`MiniBuffer::selection_range(&self) -> Option<(usize, usize)>`** — the
  active selection as `(start_col, end_col)` byte columns, or `None`.

- **`MiniBuffer::has_selection(&self) -> bool`**.

- **`MiniBuffer::clear(&mut self)`** — resets the buffer to empty and removes
  any selection.

- **Single-line invariant** is structural: only Return and Up/Down are filtered;
  every other affordance from `BufferFocusTarget` passes through unmodified.
  `content()` will therefore never contain `\n`.

- **Unit tests** covering:
  - Typing characters builds `content()`.
  - Backspace removes the last character; Backspace on empty is a no-op.
  - Alt+Backspace (`option: true`) deletes a word backward.
  - Ctrl+K kills to end of line.
  - Option+Left / Option+Right move the cursor by word.
  - Shift+Right extends the selection; `selection_range()` returns the correct
    column span.
  - Return is a no-op (content unchanged, no newline inserted).
  - Up and Down are no-ops.
  - Cmd+A selects all; `selection_range()` covers the full content.
  - `clear()` empties content and removes selection.