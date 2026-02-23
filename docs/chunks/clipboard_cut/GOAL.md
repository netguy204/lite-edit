---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/editor/src/buffer_target.rs#Command::Cut
    implements: "Cut command enum variant for Cmd+X"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Key binding mapping - Cmd+X resolves to Command::Cut"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Cut execution logic - copies selection to clipboard and deletes it"
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- pty_wakeup_reentrant
- terminal_shell_env
---

# Clipboard Cut: Cmd+X

## Minor Goal

Add Cmd+X (cut) to the editor buffer, completing the standard macOS clipboard triad (copy/cut/paste). Cut copies the selected text to the system clipboard and then deletes it from the buffer. This is one of the most fundamental editing operations and its absence is immediately noticeable to any macOS user. Builds directly on the clipboard infrastructure established in `clipboard_operations`.

## Success Criteria

- **Cmd+X cuts selection to clipboard**: Add a `Cut` variant to the `Command` enum in `buffer_target.rs`. Map `Key::Char('x')` with `mods.command && !mods.control` to `Cut` in `resolve_command`. Execute by:
  1. Calling `buffer.selected_text()` to get the selected content
  2. If `Some(text)`, write it to the macOS pasteboard via `copy_to_clipboard(&text)`
  3. Delete the selected text from the buffer (the selection model's mutation behavior handles this — call `buffer.delete_backward()` or `buffer.delete_selection()` as appropriate)
  4. Mark affected lines dirty

- **Cmd+X with no selection is a no-op**: If no selection is active when Cmd+X is pressed, nothing happens — no clipboard modification, no buffer mutation. This matches standard macOS behavior.

- **Cmd+X then Cmd+V round-trips**: Cut text with Cmd+X, then paste it elsewhere with Cmd+V. The pasted content must exactly match what was cut.

- **Cmd+A then Cmd+X cuts entire buffer**: Select all followed by cut should copy the full buffer to clipboard and leave the buffer empty (single empty line).

- **Undo integration**: If undo is supported, Cmd+X should be undoable as a single operation (the deletion half). The clipboard write is a side effect and is not undone.

- **Unit tests**:
  - `resolve_command` maps Cmd+X → `Cut`
  - Cmd+X with active selection copies to mock clipboard and deletes from buffer
  - Cmd+X with no selection leaves buffer and clipboard unchanged
  - Cut then paste round-trip preserves content exactly
  - Cut multiline selection produces correct clipboard content and leaves buffer with lines joined