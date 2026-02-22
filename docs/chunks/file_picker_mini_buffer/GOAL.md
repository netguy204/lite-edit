---
status: ACTIVE
ticket: null
parent_chunk: file_picker
code_paths:
- crates/editor/src/selector.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/selector_overlay.rs
code_references:
  - ref: crates/editor/src/selector.rs#SelectorWidget
    implements: "MiniBuffer-backed selector widget with full editing affordances"
  - ref: crates/editor/src/selector.rs#SelectorWidget::new
    implements: "Zero-argument constructor creating MiniBuffer with default FontMetrics"
  - ref: crates/editor/src/selector.rs#SelectorWidget::query
    implements: "Query accessor delegating to mini_buffer.content()"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_key
    implements: "Key handling with MiniBuffer delegation for query editing"
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

Port the file picker's query-input field from its current bespoke `String`
mutation to use `MiniBuffer`. After this change every text-editing affordance
available in the main buffer — word-jump, kill-line, shift-selection, clipboard
— works inside the file picker query box, for free, with no new logic.

## Success Criteria

- **`SelectorWidget`** in `crates/editor/src/selector.rs` replaces its
  `query: String` field with `mini_buffer: MiniBuffer`.

- **`SelectorWidget::new()`** keeps its zero-argument signature. It constructs
  the internal `MiniBuffer` with suitable default `FontMetrics` (the metrics
  only affect internal viewport bookkeeping, which is irrelevant for a
  single-line query field; they do not affect rendered output).

- **`SelectorWidget::query()`** delegates to `mini_buffer.content()`. The
  return type may become `String` if the gap-buffer API only exposes owned
  values; all callers in `editor_state.rs` and `selector_overlay.rs` are
  updated accordingly.

- **`SelectorWidget::handle_key`** removes its `Backspace` and `Char` branches
  entirely. The new catch-all arm:
  1. Captures `prev_query = self.mini_buffer.content()` before delegating.
  2. Calls `self.mini_buffer.handle_key(event.clone())`.
  3. If `mini_buffer.content() != prev_query`, resets `selected_index` to 0.
  4. Returns `SelectorOutcome::Pending`.
  Up/Down/Return/Escape are still handled directly by `SelectorWidget` before
  the catch-all, so they never reach `MiniBuffer`.

- **All existing `SelectorWidget` unit tests pass** without modification. The
  observable behaviour — query string changes on Backspace and printable chars,
  Up/Down navigation, Enter/Escape outcomes — is unchanged; only the mechanism
  behind query editing is different.

- **Manual smoke test**: open the file picker (Cmd+P), type a partial filename,
  then press Option+Backspace to delete a word, Ctrl+A to jump to start, and
  Cmd+V to paste — all behave as they do in the main editor.

## Relationship to Parent

The parent chunk (`file_picker`) established `SelectorWidget` with a minimal
query-editing model: character append on printable keys, `pop()` on Backspace.
This chunk supersedes that editing logic by replacing the raw `String` with a
`MiniBuffer`. List navigation, confirmation, and cancellation from the parent
chunk are unchanged.