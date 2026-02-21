# Implementation Plan

## Approach

This chunk adds Cmd+A (select-all), Cmd+C (copy), and Cmd+V (paste) operations that integrate with the macOS system clipboard. The implementation follows the existing command resolution pattern established in `buffer_target.rs` and creates a small, focused clipboard module for NSPasteboard integration.

**High-level strategy:**

1. **Clipboard module**: Create `crates/editor/src/clipboard.rs` with two functions that wrap NSPasteboard Objective-C calls using the `objc2-app-kit` bindings already present in Cargo.toml.

2. **Command enum extension**: Add `SelectAll`, `Copy`, and `Paste` variants to the `Command` enum in `buffer_target.rs`.

3. **Key resolution**: Add match arms in `resolve_command` to map:
   - `Cmd+A` → `SelectAll`
   - `Cmd+C` → `Copy`
   - `Cmd+V` → `Paste`

4. **Command execution**: Implement the commands in `execute_command`:
   - `SelectAll` calls `buffer.select_all()` and marks full viewport dirty
   - `Copy` calls `buffer.selected_text()` and writes to clipboard (no-op if no selection)
   - `Paste` reads from clipboard and calls `buffer.insert_str()` (which handles selection replacement)

**Key design decisions:**
- Clipboard operations are **side effects** called directly from `execute_command`, not mediated through `EditorContext`. This matches the GOAL.md guidance.
- The clipboard module is a thin FFI wrapper with no business logic - all complexity stays in `BufferFocusTarget`.
- Tests verify command resolution and buffer state changes; clipboard FFI is not unit-tested per the testing philosophy's "humble object" pattern.

## Subsystem Considerations

No subsystems exist yet in this project. This chunk does not introduce cross-cutting patterns that would warrant a new subsystem - the clipboard module is a small, isolated FFI wrapper.

## Sequence

### Step 1: Create clipboard module with NSPasteboard FFI

Create `crates/editor/src/clipboard.rs` with two public functions:

```rust
/// Writes text to the macOS general pasteboard.
pub fn copy_to_clipboard(text: &str)

/// Reads text from the macOS general pasteboard.
pub fn paste_from_clipboard() -> Option<String>
```

**Implementation details:**
- Use `objc2_app_kit::NSPasteboard::generalPasteboard()` to get the shared pasteboard
- For `copy_to_clipboard`:
  1. Call `clearContents()` to clear existing content
  2. Create an `NSString` from the Rust `&str`
  3. Call `setString_forType(string, NSPasteboardTypeString)` to write the string
- For `paste_from_clipboard`:
  1. Call `stringForType(NSPasteboardTypeString)` to read
  2. Convert the `Option<Retained<NSString>>` to `Option<String>`

**Reference patterns:** See `metal_view.rs` for how `objc2` crates are used in this project.

Location: `crates/editor/src/clipboard.rs`

Add `pub mod clipboard;` to `crates/editor/src/main.rs` (or lib.rs if it exists).

### Step 2: Add SelectAll, Copy, Paste commands to Command enum

In `crates/editor/src/buffer_target.rs`, add three new variants to the `Command` enum:

```rust
enum Command {
    // ... existing variants ...
    /// Select the entire buffer
    SelectAll,
    /// Copy selection to clipboard
    Copy,
    /// Paste from clipboard at cursor
    Paste,
}
```

Location: `crates/editor/src/buffer_target.rs`

### Step 3: Add key resolution for Cmd+A, Cmd+C, Cmd+V

In the `resolve_command` function in `buffer_target.rs`, add match arms for the new commands. These must be placed **before** the Ctrl+A match arm to ensure correct precedence:

```rust
// Cmd+A → select all (must come before Ctrl+A)
Key::Char('a') if mods.command && !mods.control => Some(Command::SelectAll),

// Cmd+C → copy
Key::Char('c') if mods.command && !mods.control => Some(Command::Copy),

// Cmd+V → paste
Key::Char('v') if mods.command && !mods.control => Some(Command::Paste),
```

Note: The existing `Ctrl+A` match arm (`Key::Char('a') if mods.control && !mods.command`) already excludes `Cmd+A`, so no conflict exists. However, placing `Cmd` variants first is clearer and future-proof.

Location: `crates/editor/src/buffer_target.rs`

### Step 4: Implement SelectAll command execution

In the `execute_command` method of `BufferFocusTarget`, add handling for `Command::SelectAll`:

```rust
Command::SelectAll => {
    ctx.buffer.select_all();
    // Mark full viewport dirty since all visible lines now have selection highlight
    ctx.dirty_region.merge(DirtyRegion::FullViewport);
    return;
}
```

The `buffer.select_all()` method already exists from the `text_selection_model` chunk.

Location: `crates/editor/src/buffer_target.rs`

### Step 5: Implement Copy command execution

Add handling for `Command::Copy`:

```rust
Command::Copy => {
    // Get selected text; no-op if no selection
    if let Some(text) = ctx.buffer.selected_text() {
        crate::clipboard::copy_to_clipboard(&text);
    }
    // Do not modify buffer or clear selection (standard copy behavior)
    return;
}
```

Note: Copy does not return dirty lines - it's a read-only operation on the buffer.

Location: `crates/editor/src/buffer_target.rs`

### Step 6: Implement Paste command execution

Add handling for `Command::Paste`:

```rust
Command::Paste => {
    if let Some(text) = crate::clipboard::paste_from_clipboard() {
        let dirty = ctx.buffer.insert_str(&text);
        ctx.mark_dirty(dirty);
        ctx.ensure_cursor_visible();
    }
    return;
}
```

The `buffer.insert_str()` method (from `text_selection_model` chunk) already:
1. Deletes any active selection first
2. Inserts the string at the cursor
3. Returns appropriate `DirtyLines`

Location: `crates/editor/src/buffer_target.rs`

### Step 7: Write unit tests for command resolution

Add tests to verify that `resolve_command` correctly maps the new key combinations:

```rust
#[test]
fn test_cmd_a_resolves_to_select_all() {
    let event = KeyEvent::new(Key::Char('a'), Modifiers { command: true, ..Default::default() });
    assert_eq!(resolve_command(&event), Some(Command::SelectAll));
}

#[test]
fn test_cmd_c_resolves_to_copy() {
    let event = KeyEvent::new(Key::Char('c'), Modifiers { command: true, ..Default::default() });
    assert_eq!(resolve_command(&event), Some(Command::Copy));
}

#[test]
fn test_cmd_v_resolves_to_paste() {
    let event = KeyEvent::new(Key::Char('v'), Modifiers { command: true, ..Default::default() });
    assert_eq!(resolve_command(&event), Some(Command::Paste));
}

#[test]
fn test_cmd_a_vs_ctrl_a_precedence() {
    // Cmd+A should be SelectAll, not MoveToLineStart
    let cmd_a = KeyEvent::new(Key::Char('a'), Modifiers { command: true, ..Default::default() });
    assert_eq!(resolve_command(&cmd_a), Some(Command::SelectAll));

    // Ctrl+A should still be MoveToLineStart
    let ctrl_a = KeyEvent::new(Key::Char('a'), Modifiers { control: true, ..Default::default() });
    assert_eq!(resolve_command(&ctrl_a), Some(Command::MoveToLineStart));
}
```

Location: `crates/editor/src/buffer_target.rs` (tests module)

### Step 8: Write integration tests for clipboard operations through BufferFocusTarget

Add tests that verify the full pipeline from key event to buffer state:

```rust
#[test]
fn test_cmd_a_selects_entire_buffer() {
    let mut buffer = TextBuffer::from_str("hello\nworld");
    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('a'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    assert!(buffer.has_selection());
    assert_eq!(buffer.selected_text(), Some("hello\nworld".to_string()));
    assert_eq!(dirty, DirtyRegion::FullViewport);
}

#[test]
fn test_cmd_c_with_no_selection_is_noop() {
    let mut buffer = TextBuffer::from_str("hello");
    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    // Ensure no selection
    assert!(!buffer.has_selection());

    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('c'), Modifiers { command: true, ..Default::default() });
        let handled = target.handle_key(event, &mut ctx);
        assert_eq!(handled, Handled::Yes); // Command was recognized
    }

    // Buffer unchanged, no dirty region
    assert_eq!(buffer.content(), "hello");
    assert_eq!(dirty, DirtyRegion::None);
}
```

Note: Testing actual clipboard content would require calling the clipboard FFI functions, which is not appropriate for unit tests. The clipboard module is a "humble object" and is tested only via manual verification.

Location: `crates/editor/src/buffer_target.rs` (tests module)

### Step 9: Update GOAL.md code_paths

Update the `code_paths` field in `docs/chunks/clipboard_operations/GOAL.md` to reference the files touched:

```yaml
code_paths:
  - crates/editor/src/clipboard.rs
  - crates/editor/src/buffer_target.rs
  - crates/editor/src/main.rs
```

Location: `docs/chunks/clipboard_operations/GOAL.md`

## Dependencies

**Chunk dependencies:**
- `text_selection_model` (ACTIVE): Provides `buffer.select_all()`, `buffer.selected_text()`, and `buffer.insert_str()` with selection-aware behavior. This chunk depends on these methods being implemented.

**External dependencies:**
- `objc2-app-kit` crate: Already in `Cargo.toml` - provides `NSPasteboard` bindings
- `objc2-foundation` crate: Already in `Cargo.toml` - provides `NSString` bindings

## Risks and Open Questions

**Risks:**
1. **NSPasteboard API usage**: The `objc2-app-kit` bindings for NSPasteboard may not exactly match the expected API. If `NSPasteboard::generalPasteboard()` or `setString_forType` methods don't exist with those exact names, we'll need to check the crate documentation or use lower-level `msg_send!` macros.

2. **Thread safety**: NSPasteboard must be accessed from the main thread (which is guaranteed since this is called from key event handlers), but the API may have additional requirements around pasteboard change counts or ownership.

3. **Empty paste behavior**: If the clipboard contains non-text data (e.g., an image), `paste_from_clipboard()` should return `None` and the paste command should be a no-op. Need to verify `stringForType` returns `None` in this case.

**Mitigations:**
- Step 1 (clipboard module) should be implemented first and tested manually before integrating with the command system.
- If the bindings are problematic, fall back to raw `msg_send!` calls as used in `metal_view.rs`.

## Deviations

*To be populated during implementation.*