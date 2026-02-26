---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/mini_buffer.rs
  - crates/editor/src/selector.rs
  - crates/editor/src/selector_target.rs
  - crates/editor/src/find_target.rs
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/mini_buffer.rs#MiniBuffer::handle_text_input
    implements: "Core text input handling for MiniBuffer - converts text to synthetic KeyEvent::char() calls"
  - ref: crates/editor/src/selector.rs#SelectorWidget::handle_text_input
    implements: "Text input for selector widget - delegates to MiniBuffer and resets selection index on query change"
  - ref: crates/editor/src/find_target.rs#FindFocusTarget::handle_text_input
    implements: "Text input for find strip - delegates to MiniBuffer and tracks query_changed flag for live search"
  - ref: crates/editor/src/selector_target.rs#SelectorFocusTarget::handle_text_input
    implements: "Text input delegation through FocusTarget interface to SelectorWidget"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_insert_text
    implements: "Focus-aware text input routing - dispatches TextInputEvent to appropriate focus target by EditorFocus"
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

Route `TextInputEvent` through the focus stack so that minibuffer-backed
overlays (fuzzy file opener, find-in-file strip) receive typed characters.

On macOS, regular keyboard typing flows through `interpretKeyEvents` →
`insertText:` → `TextInputEvent`, **not** through `KeyEvent`. The current
`EditorState::handle_insert_text()` has an early return
(`if self.focus != EditorFocus::Buffer { return }`) that silently discards
all text input when the selector or find strip has focus. This means typed
characters never reach the minibuffer, while modifier-based keys (Escape,
arrows) still work because they go through the `KeyEvent` path.

The fix should route `TextInputEvent` through the focus stack (or an
equivalent dispatch mechanism) so that the active focus target—whether
`SelectorFocusTarget`, `FindFocusTarget`, or the buffer itself—receives
the text input.

## Success Criteria

- Typing characters in the fuzzy file opener populates the query field and filters results
- Typing characters in the find-in-file strip populates the search query and triggers live search
- Escape still dismisses both overlays
- Regular buffer typing continues to work as before
- Terminal tab text input continues to work as before (raw bytes, no bracketed paste for regular typing)
- IME composition and paste work in all three contexts (buffer, selector, find strip)

