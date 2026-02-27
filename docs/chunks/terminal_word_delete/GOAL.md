---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView::__key_down
    implements: "Routes Option-modified keys through bypass path, skipping macOS text input system"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- app_nap_activity_assertions
- app_nap_blink_timer
- app_nap_file_watcher_pause
- highlight_text_source
- merge_conflict_render
- minibuffer_input
- terminal_single_pane_refresh
---

# Chunk Goal

## Minor Goal

Alt+Backspace and Alt+D do not work as expected. Alt+D inserts `∂` (macOS Option+D character composition) and Alt+Backspace does nothing. Both the editor buffer path and the terminal path are affected.

The downstream handling is already correct — `convert_key()` in `metal_view.rs` uses `charactersIgnoringModifiers` when Option is held, and the terminal's `InputEncoder` correctly encodes Alt+Backspace → `\x1b\x7f` and Alt+D → `\x1b\x64`. The bug is upstream: `__key_down` in `metal_view.rs` only bypasses the macOS text input system (`interpretKeyEvents:`) for Command, Control, Escape, and Function keys — not Option. When Option+D is pressed, it goes through `interpretKeyEvents:` → `__insert_text`, where macOS composes `∂` and sends it as literal text, never reaching `convert_key()`.

The fix is a single change in `__key_down` — add Option to the text-input-system bypass condition so Option-modified keys route through `convert_key()` instead of `interpretKeyEvents:`. This fixes both editor and terminal paths with no duplication.

## Success Criteria

- Alt+D in the terminal sends `\x1b\x64` (ESC+d) to the PTY instead of inserting `∂`
- Alt+Backspace in the terminal sends `\x1b\x7f` (ESC+DEL) to the PTY
- Alt+D in editor buffers triggers `DeleteForwardWord` instead of inserting `∂`
- Alt+Backspace in editor buffers triggers `DeleteBackwardWord`
- Non-Option key input (regular typing, IME composition, dead keys) is unaffected by the bypass change