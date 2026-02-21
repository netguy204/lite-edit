---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/clipboard.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/clipboard.rs#copy_to_clipboard
    implements: "Writes text to macOS system clipboard via NSPasteboard"
  - ref: crates/editor/src/clipboard.rs#paste_from_clipboard
    implements: "Reads text from macOS system clipboard via NSPasteboard"
  - ref: crates/editor/src/buffer_target.rs#Command::SelectAll
    implements: "SelectAll command enum variant for Cmd+A"
  - ref: crates/editor/src/buffer_target.rs#Command::Copy
    implements: "Copy command enum variant for Cmd+C"
  - ref: crates/editor/src/buffer_target.rs#Command::Paste
    implements: "Paste command enum variant for Cmd+V"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Key binding resolution for Cmd+A/C/V to clipboard commands"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Clipboard command execution (select_all, copy, paste)"
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- text_selection_model
created_after:
- editable_buffer
- glyph_rendering
- metal_surface
- viewport_rendering
---
# Clipboard Operations: Cmd+A, Cmd+C, Cmd+V

## Minor Goal

Add select-all, copy, and paste operations that integrate with the macOS system clipboard. These are among the most fundamental editor interactions — without them, users cannot move text into or out of the editor. This chunk depends on the text selection model for selection state.

## Success Criteria

- **Cmd+A selects entire buffer**: Add a `SelectAll` command to the `Command` enum in `buffer_target.rs`. Map `Key::Char('a')` with `mods.command && !mods.control` to `SelectAll` in `resolve_command`. Execute by calling `buffer.select_all()`. Mark the full viewport dirty (since all visible lines now have selection highlight).

- **Cmd+C copies selection to clipboard**: Add a `Copy` command. Map `Key::Char('c')` with `mods.command && !mods.control` to `Copy`. Execute by:
  1. Calling `buffer.selected_text()` to get the selected content
  2. If `Some(text)`, write it to the macOS pasteboard via `NSPasteboard`
  3. Do not modify the buffer or clear the selection (standard copy behavior)
  - If no selection is active, the command is a no-op.

- **Cmd+V pastes from clipboard**: Add a `Paste` command. Map `Key::Char('v')` with `mods.command && !mods.control` to `Paste`. Execute by:
  1. Reading the string content from `NSPasteboard::generalPasteboard`
  2. If the pasteboard contains a string, call `buffer.insert_str(&text)` (which will delete any active selection first, per the selection model)
  3. Mark appropriate dirty lines

- **NSPasteboard integration**: Create a small clipboard module (e.g., `crates/editor/src/clipboard.rs`) with two functions:
  - `pub fn copy_to_clipboard(text: &str)` — writes text to `NSPasteboard.generalPasteboard` with `NSPasteboardTypeString`
  - `pub fn paste_from_clipboard() -> Option<String>` — reads string from the general pasteboard
  - These wrap the Objective-C calls using the `objc2-app-kit` crate's `NSPasteboard` bindings.

- **Clipboard access from BufferFocusTarget**: The focus target needs to call clipboard functions during command execution. Since clipboard access is a side effect (not buffer mutation), the `Copy` and `Paste` commands call the clipboard module directly from `execute_command`, not through `EditorContext`.

- **Cmd+A then Cmd+C copies entire buffer**: The combined sequence should work: Cmd+A selects all, then Cmd+C copies the full buffer content to the system clipboard. Verify this works by pasting into another app.

- **Cmd+V replaces selection**: If text is selected when Cmd+V is pressed, the pasted text replaces the selection (handled automatically by the selection model's "mutations delete selection first" behavior in `insert_str`).

- **Modifier conflict resolution**: Ensure `Cmd+A` (select-all) takes priority over `Ctrl+A` (move to line start). The `resolve_command` match order must check `mods.command` cases before `mods.control` cases for the `'a'` key. Currently `Ctrl+A` is matched as `Key::Char('a') if mods.control && !mods.command`, which correctly excludes `Cmd+A` — just verify `Cmd+A` has its own match arm.

- **Unit tests**:
  - `resolve_command` maps Cmd+A → SelectAll, Cmd+C → Copy, Cmd+V → Paste
  - Cmd+A through BufferFocusTarget results in full buffer selection
  - Cmd+V inserts clipboard content at cursor
  - Cmd+V with active selection replaces the selection
  - Cmd+C with no selection is a no-op
