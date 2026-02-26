---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
  - crates/editor/tests/typing_test.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView
    implements: "NSTextInputClient protocol conformance declaration enabling macOS text input routing"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- focus_stack
- grapheme_cluster_awareness
- quad_buffer_prealloc
- renderer_decomposition
- styled_line_cache
- unicode_ime_input
---

# Chunk Goal

## Minor Goal

Fix a regression where character keystrokes sent to a buffer produce a system
chime (NSBeep) and are silently discarded. Hotkeys (open tab, close tab, quit)
continue to work. The last known-good commit is `7a494aaf`, meaning the
regression was introduced somewhere in the subsequent commit range which includes
the `focus_stack`, `unicode_ime_input`, `styled_line_cache`,
`renderer_decomposition`, `grapheme_cluster_awareness`, and
`quad_buffer_prealloc` chunks.

The most likely culprits are:
- **`unicode_ime_input`** — introduced `NSTextInputClient` conformance which
  changes how macOS routes keystrokes to the view. If `insertText(_:)` or
  `doCommandBySelector(_:)` do not properly forward to the buffer, characters
  would be swallowed and the system would beep.
- **`focus_stack`** — introduced composable focus handling. If the active
  buffer's view is no longer the first responder or the focus stack does not
  route key events to it, input would be lost.

## Success Criteria

- Typing printable characters into an open buffer inserts them at the cursor
  position (no system chime, no dropped keystrokes)
- All existing hotkey bindings continue to function
- `git bisect` or manual inspection identifies the exact commit that introduced
  the regression
- The fix is verified against commit `7a494aaf` behavior as the baseline