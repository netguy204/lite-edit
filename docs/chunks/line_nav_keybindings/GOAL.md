---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/metal_view.rs
- crates/editor/src/buffer_target.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView::convert_key
    implements: "Control-key handling in NSEvent conversion - uses charactersIgnoringModifiers when Control is held to get base character instead of control character"
  - ref: crates/editor/src/buffer_target.rs#resolve_command
    implements: "Maps Home/End keys and Ctrl+A/Ctrl+E to MoveToLineStart/MoveToLineEnd commands"
  - ref: crates/editor/src/buffer_target.rs#BufferFocusTarget::execute_command
    implements: "Executes MoveToLineStart/MoveToLineEnd commands via TextBuffer methods"
narrative: editor_qol_interactions
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- editable_buffer
- glyph_rendering
- metal_surface
- viewport_rendering
---
# Verify Home/End and Ctrl+A/Ctrl+E Line Navigation

## Minor Goal

Verify that Home, End, Ctrl+A, and Ctrl+E keybindings work end-to-end from macOS key event through to cursor movement. The `resolve_command` function in `buffer_target.rs` already maps these correctly:

- `Key::Home` → `MoveToLineStart`
- `Key::End` → `MoveToLineEnd`
- `Ctrl+A` → `MoveToLineStart`
- `Ctrl+E` → `MoveToLineEnd`

And `TextBuffer` already implements `move_to_line_start` and `move_to_line_end`. However, there is a likely bug in the NSEvent conversion pipeline: `MetalView::convert_key` uses `event.characters()` for non-special keys, which on macOS returns the *interpreted* character. When Control is held, `Ctrl+A` produces the control character `\x01` (SOH), not `'a'`. The current code filters out control characters (`ch.is_control()` returns `None`), so **Ctrl+A and Ctrl+E are silently dropped before reaching `resolve_command`**.

This chunk fixes the NSEvent conversion so Ctrl+key combinations reach the command resolver, and adds an integration-style test confirming the full pipeline works.

## Success Criteria

- **Ctrl+A and Ctrl+E produce correct KeyEvents**: Fix `MetalView::convert_key` to handle Control-modified keys. When the Control modifier is held, use `charactersIgnoringModifiers` (the unmodified character) instead of `characters` (which returns the control character). This ensures `Ctrl+A` produces `KeyEvent { key: Key::Char('a'), modifiers: Modifiers { control: true, .. } }` rather than being swallowed.

- **Home and End keys work**: These use key codes (`0x73` and `0x77`) which are already handled in the key-code match. Verify they produce the correct `Key::Home` and `Key::End` variants and that the command resolver maps them correctly. This should already work but needs confirmation via test.

- **Unit tests for convert_key with Control modifier**: Test that Ctrl+A, Ctrl+E, and other Ctrl+letter combinations produce the expected `KeyEvent` with `Key::Char('a')` / `Key::Char('e')` and `control: true`. (This may require refactoring `convert_key` to be testable without a real NSEvent, or testing at the `resolve_command` level with synthetic events.)

- **End-to-end test at the BufferFocusTarget level**: Confirm that sending `KeyEvent::new(Key::Char('a'), Modifiers { control: true, .. })` through `BufferFocusTarget::handle_key` moves the cursor to line start, and similarly for Ctrl+E → line end. (These tests likely already exist in `buffer_target.rs` — verify they pass.)

- **No regressions**: Regular Ctrl-free typing still works. The fix should only change behavior when the Control modifier is active.
