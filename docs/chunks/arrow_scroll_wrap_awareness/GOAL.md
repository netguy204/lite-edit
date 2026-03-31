---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
code_references:
- ref: crates/editor/src/editor_state.rs#EditorState::handle_key_buffer
  implements: "Wrap-aware cursor snap-back: replaces ensure_visible with ensure_visible_wrapped when cursor appears off-screen due to line-vs-screen-row confusion"
- ref: crates/editor/src/editor_state.rs#EditorState::goto_definition
  implements: "Wrap-aware gotodef scroll (same-file): replaces ensure_visible with ensure_visible_wrapped for correct position with wrapped lines"
- ref: crates/editor/src/editor_state.rs#EditorState::goto_cross_file_definition
  implements: "Wrap-aware gotodef scroll (cross-file): replaces ensure_visible with ensure_visible_wrapped for correct position with wrapped lines"
- ref: crates/editor/src/editor_state.rs#EditorState::handle_file_drop
  implements: "Wrap-aware scroll after file drop insertion: replaces ensure_visible with ensure_visible_wrapped"
- ref: crates/editor/src/editor_state.rs#EditorState::handle_insert_text
  implements: "Wrap-aware scroll after text insertion: replaces ensure_visible with ensure_visible_wrapped"
- ref: crates/editor/src/editor_state.rs#EditorState::handle_set_marked_text
  implements: "Wrap-aware scroll after IME marked text: replaces ensure_visible with ensure_visible_wrapped"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on:
- find_scroll_wrap_awareness
created_after:
- find_scroll_wrap_awareness
---

# Chunk Goal

## Minor Goal

Fix viewport jumping during arrow key navigation when soft wrapping is active.

Arrow key cursor movement correctly uses the wrap-aware `ensure_visible_wrapped()`
path (via `context.rs`). However, there is a **cursor snap-back** at
`editor_state.rs:~2644` that uses the unwrapped `ensure_visible()`. When the
cursor is near the viewport edge with wrapped lines above it, this snap-back
fires and scrolls to the wrong position, then the wrap-aware path corrects it —
causing a visible two-step jump.

This chunk also audits and fixes the remaining non-wrap-aware `ensure_visible()`
call sites in `editor_state.rs` that can cause similar viewport jumps:

- **Line ~2644** — cursor snap-back when cursor is off-screen (primary culprit
  for arrow key jumping)
- **Line ~1504** — go-to-definition (same file, initial scroll)
- **Line ~1617** — go-to-definition (cross-file, initial scroll)
- **Line ~3639** — file drop insertion
- **Line ~3774** — internal source text insertion
- **Line ~3821** — IME marked text

All of these should use `ensure_visible_wrapped()` (or the new
`ensure_visible_wrapped_with_margin()` introduced by `find_scroll_wrap_awareness`)
instead of the logical-line-based `ensure_visible()`.

## Success Criteria

- Arrow key navigation does not cause viewport jumping when soft wrapping is
  active and wrapped lines are present above the cursor
- All `ensure_visible()` call sites in `editor_state.rs` that operate on editor
  buffer content use wrap-aware scrolling
- Go-to-definition scrolls to the correct position with wrapped lines
- File drop and IME insertion scroll to the correct position with wrapped lines
- No regressions when wrapping is disabled