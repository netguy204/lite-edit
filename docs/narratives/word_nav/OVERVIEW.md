---
status: DRAFTING
advances_trunk_goal: "Required Properties: Standard text editor interaction patterns"
proposed_chunks:
  - prompt: "Extract word boundary scanning from `delete_backward_word` into two private helper functions in `crates/buffer/src/text_buffer.rs`. Add `fn word_boundary_left(chars: &[char], col: usize) -> usize` — scans leftward from `col` through characters sharing the same class as `chars[col-1]` (using `char::is_whitespace()`) and returns the leftmost column of that run; returns `col` when `col == 0`. Add `fn word_boundary_right(chars: &[char], col: usize) -> usize` — scans rightward from `col` through characters sharing the same class as `chars[col]` and returns the first column past the end of the run; returns `col` when `col >= chars.len()`. Both helpers operate only on the provided slice — no buffer access, no cursor state. Each must carry a `// Spec: docs/trunk/SPEC.md#word-model` comment. Refactor `delete_backward_word` to replace its inline scan loop with a call to `word_boundary_left` (behaviour must remain identical). Add direct unit tests for both helpers covering: empty slice, single-char run, full-line run, non-whitespace run, whitespace run, col at 0, col at len, col in the middle of a run."
    chunk_directory: word_boundary_primitives
    depends_on: []
  - prompt: "Add Alt+Left and Alt+Right word-jump navigation to TextBuffer and wire it through the command pipeline. The word model is defined in docs/trunk/SPEC.md#word-model; use the `word_boundary_left` and `word_boundary_right` helpers extracted in the preceding chunk. Add `move_word_left` and `move_word_right` methods to TextBuffer. For `move_word_right`: call `word_boundary_right` to get the right edge of the run at the cursor; if that lands the cursor on a whitespace run, call `word_boundary_right` again to skip to the end of the following non-whitespace run (stopping at line end). For `move_word_left`: call `word_boundary_left` to get the left edge of the run before the cursor; if that lands at the start of a whitespace run, call `word_boundary_left` again to skip to the start of the preceding non-whitespace run (stopping at col 0). Both methods clear any active selection (consistent with `move_left`/`move_right`). Each carries a `// Spec: docs/trunk/SPEC.md#word-model` comment. Add `MoveWordLeft` and `MoveWordRight` to the `Command` enum in `buffer_target.rs`; wire `Option+Left` → `MoveWordLeft` and `Option+Right` → `MoveWordRight` in `resolve_command` (before the plain Left/Right arms). Execute via `execute_command` with `mark_cursor_dirty` + `ensure_cursor_visible`. Add unit tests: cursor mid-word, at word start, at word end, on whitespace between words, at line start, at line end, empty line, single-character word."
    chunk_directory: word_jump_navigation
    depends_on: [0]
  - prompt: "Add Alt+D to delete from the cursor to the end of the current word or whitespace run (forward word deletion). The word model is defined in docs/trunk/SPEC.md#word-model. Add `delete_forward_word` to TextBuffer using the `word_boundary_right` helper extracted in chunk 0: call `word_boundary_right(chars, cursor.col)` to find the right edge of the current run, then delete from `cursor.col` to that column. Never crosses a newline — stops at the line boundary. If there is an active selection, delete the selection instead (consistent with all other deletion operations). If the cursor is at the end of the line, no-op (return `DirtyLines::None`). Add a `// Spec: docs/trunk/SPEC.md#word-model` comment. Add `DeleteForwardWord` to the `Command` enum and wire `Option+'d'` → `DeleteForwardWord` in `resolve_command` (before the plain `Key::Char('d')` arm). Execute via `execute_command`. Add unit tests: cursor mid-word on non-whitespace, cursor on whitespace between words, cursor at line end, cursor at line start, selection active (deletes selection), line with only whitespace."
    chunk_directory: word_forward_delete
    depends_on: [0]
  - prompt: "Add double-click word selection. The word model is defined in docs/trunk/SPEC.md#word-model; use the `word_boundary_left` and `word_boundary_right` helpers from chunk 0 (exposed from the buffer crate as `pub(crate)` or tested via TextBuffer methods). Implementation has two parts: (1) In `metal_view.rs`, extend `MouseEvent` or `MouseEventKind` to carry `click_count: u32`, populated from `NSEvent.clickCount`. (2) In `buffer_target.rs`'s `handle_mouse`, detect `click_count == 2` on a `Down` event: convert pixel to buffer position with `pixel_to_buffer_position`, get the line content, call `word_boundary_left(chars, col + 1)` for the word start and `word_boundary_right(chars, col)` for the word end (both on the same character class as `chars[col]`), then set selection anchor at word start and cursor at word end using `set_selection_anchor` + `move_cursor_preserving_selection`. Mark cursor line dirty. Add a `// Spec: docs/trunk/SPEC.md#word-model` comment. Add integration tests: double-click mid-word selects word, double-click at word start selects word, double-click on whitespace selects whitespace run, double-click on empty line is a no-op."
    chunk_directory: word_double_click_select
    depends_on: [0, 1]
created_after: ["file_buffer_association"]
---

## Advances Trunk Goal

This narrative advances the **Required Properties: Standard text editor interaction patterns** trunk goal. It adds the word-oriented navigation and editing primitives that macOS users expect from any text editor: Alt+Left/Right for word-boundary cursor jumps, Alt+D for forward word deletion, and double-click for word selection. All features share a single consistent word model defined in `docs/trunk/SPEC.md#word-model` and implemented through shared helpers.

## Driving Ambition

lite-edit currently navigates and edits at the character level (single-step arrow keys, delete-one-char) or at the line level (Home/End, Ctrl+K, Cmd+Backspace). There is no word-level interaction layer. Users with muscle memory from any macOS editor — Xcode, VS Code, TextEdit — will instinctively reach for Alt+Arrow to jump words and double-click to select a word, and currently get either no response or wrong behavior.

The existing `delete_backward_word` (Alt+Backspace) implements word boundary scanning inline. As this narrative adds `move_word_left`, `move_word_right`, `delete_forward_word`, and double-click selection, that inline logic would be duplicated four more times. The first chunk resolves this by extracting the scan logic into shared helpers and canonicalising the word definition in the trunk spec — so every subsequent feature is a thin layer on top of proven primitives rather than a fresh reimplementation of the same rule.

## Chunks

0. **Word boundary primitives** — Extract `word_boundary_left` and `word_boundary_right` from `delete_backward_word` into private helpers in `text_buffer.rs`. Refactor `delete_backward_word` to use them. All subsequent chunks depend on this one.

1. **Alt+Left / Alt+Right word navigation** — Add `move_word_left` and `move_word_right` to `TextBuffer` using the helpers from chunk 0. Wire `Option+Left` / `Option+Right` through the `Command` enum.

2. **Alt+D forward word deletion** — Add `delete_forward_word` to `TextBuffer` using `word_boundary_right` from chunk 0. Wire `Option+'d'` through the `Command` enum.

3. **Double-click word selection** — Extend the mouse event model to carry `click_count`, detect double-click in `handle_mouse`, and select the word or whitespace run under the click using both helpers from chunk 0.

## Completion Criteria

When complete, a user can:
- Press Alt+Right to jump to the end of the word under the cursor (or past whitespace to the end of the next word)
- Press Alt+Left to jump to the start of the word under the cursor (or past whitespace to the start of the preceding word)
- Press Alt+D to delete from the cursor forward through the current non-whitespace or whitespace run, stopping at the line boundary
- Double-click any word to select its full non-whitespace extent; double-click on whitespace to select the whitespace run
- All behaviours derive from a single word model (`docs/trunk/SPEC.md#word-model`) and shared scanning helpers, so a future change to the word definition (e.g. treating `_` as a word separator) is a one-line edit in one place
