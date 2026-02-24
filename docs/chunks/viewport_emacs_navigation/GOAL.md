---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/editor/src/buffer_target.rs#Command
    implements: "PageUp and PageDown command variants for viewport navigation"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Keybinding resolution for Page Up/Down, Ctrl+V, Ctrl+F, Ctrl+B"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Page Up/Down command execution with cursor movement and viewport scrolling"
narrative: null
investigation: null
subsystems:
  - subsystem_id: "viewport_scroll"
    relationship: uses
friction_entries: []
bug_type: null
depends_on: []
created_after:
- pty_wakeup_reentrant
- terminal_shell_env
---

# Chunk Goal

## Minor Goal

Add Emacs-style navigation keybindings and page up/down support to buffer navigation. This brings the editor closer to the familiar keyboard navigation model that many developers expect, improving the editing experience described in the project goal.

Specifically:

- **Page Up / Page Down**: Scroll the buffer by one viewport height (the number of visible lines). The cursor should move with the viewport.
- **Ctrl+V**: Page down (Emacs `scroll-up-command` — scrolls content up, moving view down).
- **Ctrl+F**: Move cursor forward by one character (Emacs `forward-char`).
- **Ctrl+B**: Move cursor backward by one character (Emacs `backward-char`).

These bindings apply to file buffer focus only (not terminal tabs, which pass keys through to the PTY).

## Success Criteria

- Pressing Page Up scrolls the buffer up by the number of visible lines in the viewport and moves the cursor accordingly.
- Pressing Page Down scrolls the buffer down by the number of visible lines in the viewport and moves the cursor accordingly.
- Pressing Ctrl+V behaves identically to Page Down.
- Pressing Ctrl+F moves the cursor forward by one character (same as right arrow).
- Pressing Ctrl+B moves the cursor backward by one character (same as left arrow).
- All new bindings work correctly with line wrapping enabled.
- Existing arrow key and other keybindings remain unaffected.