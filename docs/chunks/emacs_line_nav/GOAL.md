---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView::__key_down
    implements: "Route Ctrl-modified keys through bypass path to restore emacs bindings"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- focus_stack
- grapheme_cluster_awareness
- invalidation_separation
- quad_buffer_prealloc
- renderer_decomposition
- styled_line_cache
- unicode_ime_input
---

# Chunk Goal

## Minor Goal

Restore Ctrl+A and Ctrl+E emacs keybindings for moving to the beginning and end
of the current line. These bindings regressed — likely during the
`unicode_ime_input` or `input_keystroke_regression` work that changed how
keystrokes are routed through macOS's text input system. Cmd+Left and Cmd+Right
(which bypass the text input system) still work for the same operations.

The root cause is the input routing split in `metal_view.rs __key_down`:
- **Cmd-modified keys** bypass `interpretKeyEvents:` and go directly to the key
  handler, where `convert_key_event()` preserves the full key+modifiers. This is
  why Cmd+Left/Right work.
- **Ctrl-modified keys** (like Ctrl+A/E) fall through to `interpretKeyEvents:`,
  which translates them into Cocoa selectors and calls `doCommandBySelector:`.

**Ctrl+V works** — Cocoa sends `pageDown:` which the handler maps to
`Key::PageDown`, confirming the `doCommandBySelector:` path is functional.

For Ctrl+A/E, Cocoa should send `moveToBeginningOfLine:`/`moveToEndOfLine:`
which the handler maps to `Key::Home`/`Key::End`. These resolve to
`MoveToLineStart`/`MoveToLineEnd` in `resolve_command()`. Since Ctrl+V works
through the same path, the issue is either:
1. Cocoa is sending a different selector than expected (e.g.
   `moveToBeginningOfParagraph:` instead of `moveToBeginningOfLine:`), or
2. The event is being consumed by `insertText:` instead of `doCommandBySelector:`
   (Ctrl+A = ASCII SOH = 0x01, which might be passed as text)

Runtime logging in `doCommandBySelector:` and `insertText:` will identify which.

For other Ctrl emacs bindings (Ctrl+F/B/N/P/D/K), the Cocoa selectors they
generate (`moveForward:`, `moveBackward:`, etc.) are NOT in the
`doCommandBySelector:` match table and silently fall to `_ => None`. These need
to be added or the routing strategy needs to change.

## Success Criteria

- Ctrl+A moves the cursor to the beginning of the current line
- Ctrl+E moves the cursor to the end of the current line
- Ctrl+F/B/N/P/D/K all work as expected (forward/back char, next/prev line,
  delete forward, kill line)
- Cmd+Left/Right continue to work (no regression)
- IME input (Japanese, Chinese, etc.) continues to work correctly — the text
  input system path must remain functional for composition
- No system beep on any of the above key combinations