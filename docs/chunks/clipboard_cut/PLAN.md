# Implementation Plan

## Approach

This chunk adds `Cmd+X` (cut) to complete the macOS clipboard triad. Cut is semantically "copy + delete", which means it combines the existing clipboard infrastructure from `clipboard_operations` with the existing selection deletion from `text_selection_model`.

**High-level strategy:**

1. **Add `Cut` variant to Command enum**: A single new variant in `buffer_target.rs`.

2. **Add key resolution for Cmd+X**: Map `Key::Char('x')` with `mods.command && !mods.control` to `Cut` in `resolve_command`. Place it adjacent to the existing `Cmd+C` and `Cmd+V` bindings.

3. **Implement Cut execution**: The `execute_command` handler for `Cut` will:
   - Call `buffer.selected_text()` to get selection (returns `None` if no selection)
   - If `Some(text)`, copy to clipboard via `copy_to_clipboard(&text)`
   - If `Some(text)`, delete the selection via `buffer.delete_selection()`
   - If no selection, do nothing (standard macOS behavior)

**Why this is minimal**: All building blocks already exist:
- `copy_to_clipboard()` from `clipboard.rs` (with test mock)
- `buffer.selected_text()` from `text_selection_model`
- `buffer.delete_selection()` from `text_selection_model`
- Key resolution pattern in `resolve_command`
- Command execution pattern in `execute_command`

No new APIs are needed. This is pure composition of existing primitives.

**Testing strategy per TESTING_PHILOSOPHY.md**:
- Write failing unit tests first for `resolve_command` mapping
- Write failing integration tests for Cut behavior through `BufferFocusTarget`
- The mock clipboard from `clipboard_operations` handles clipboard isolation
- Tests verify boundary cases: no selection, single-line selection, multiline selection

## Sequence

### Step 1: Add Cut command variant (RED phase)

First, write a failing test for the key resolution:

```rust
#[test]
fn test_cmd_x_resolves_to_cut() {
    let event = KeyEvent::new(Key::Char('x'), Modifiers { command: true, ..Default::default() });
    assert_eq!(resolve_command(&event), Some(Command::Cut));
}
```

This test will fail because `Command::Cut` doesn't exist.

Then add the `Cut` variant to the `Command` enum in `buffer_target.rs`:

```rust
// Chunk: docs/chunks/clipboard_cut - Cut command variant
/// Cut selection to clipboard (Cmd+X)
Cut,
```

Location: `crates/editor/src/buffer_target.rs`, in the `Command` enum, adjacent to `Copy` and `Paste`.

### Step 2: Add Cmd+X key resolution (GREEN phase)

Add the key binding in `resolve_command`, adjacent to the existing clipboard bindings:

```rust
// Chunk: docs/chunks/clipboard_cut - Cmd+X key binding
// Cmd+X → cut selection to clipboard
Key::Char('x') if mods.command && !mods.control => Some(Command::Cut),
```

Location: `crates/editor/src/buffer_target.rs`, in `resolve_command`, alongside `Cmd+C` and `Cmd+V`.

The test from Step 1 should now pass.

### Step 3: Add Cut execution tests (RED phase for behavior)

Write failing tests for Cut behavior before implementing:

```rust
#[test]
fn test_cmd_x_with_selection_copies_and_deletes() {
    let mut buffer = TextBuffer::from_str("hello world");
    buffer.set_cursor(0, 0);
    // Select "hello" (chars 0-5)
    buffer.set_selection_anchor(Some(Position::new(0, 0)));
    buffer.set_cursor(0, 5);

    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('x'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Buffer should have "hello" deleted
    assert_eq!(buffer.content(), " world");
    // Clipboard should contain "hello"
    assert_eq!(crate::clipboard::paste_from_clipboard(), Some("hello".to_string()));
}

#[test]
fn test_cmd_x_with_no_selection_is_noop() {
    // Clear mock clipboard
    crate::clipboard::copy_to_clipboard("original");

    let mut buffer = TextBuffer::from_str("hello");
    // No selection
    assert!(!buffer.has_selection());

    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('x'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Buffer unchanged
    assert_eq!(buffer.content(), "hello");
    // Clipboard unchanged (still "original")
    assert_eq!(crate::clipboard::paste_from_clipboard(), Some("original".to_string()));
    // No dirty region
    assert_eq!(dirty, DirtyRegion::None);
}

#[test]
fn test_cut_then_paste_roundtrip() {
    let mut buffer = TextBuffer::from_str("hello world");
    buffer.set_cursor(0, 0);
    // Select "hello"
    buffer.set_selection_anchor(Some(Position::new(0, 0)));
    buffer.set_cursor(0, 5);

    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    // Cut
    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('x'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Move cursor to end and paste
    buffer.move_to_buffer_end();
    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('v'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Buffer should be " worldhello"
    assert_eq!(buffer.content(), " worldhello");
}

#[test]
fn test_select_all_then_cut_empties_buffer() {
    let mut buffer = TextBuffer::from_str("line1\nline2\nline3");
    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    // Cmd+A to select all
    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('a'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Cmd+X to cut
    dirty = DirtyRegion::None;
    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('x'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Buffer should be empty (single empty line)
    assert_eq!(buffer.line_count(), 1);
    assert_eq!(buffer.line_content(0), "");
    // Clipboard should have the full content
    assert_eq!(crate::clipboard::paste_from_clipboard(), Some("line1\nline2\nline3".to_string()));
}

#[test]
fn test_cut_multiline_selection() {
    let mut buffer = TextBuffer::from_str("aaa\nbbb\nccc");
    // Select from middle of line 0 to middle of line 2: "a\nbbb\nc"
    buffer.set_selection_anchor(Some(Position::new(0, 2)));
    buffer.set_cursor(2, 1);

    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    {
        let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
        let event = KeyEvent::new(Key::Char('x'), Modifiers { command: true, ..Default::default() });
        target.handle_key(event, &mut ctx);
    }

    // Remaining: "aa" + "cc" = "aacc"
    assert_eq!(buffer.content(), "aacc");
    // Clipboard: "a\nbbb\nc"
    assert_eq!(crate::clipboard::paste_from_clipboard(), Some("a\nbbb\nc".to_string()));
}
```

### Step 4: Implement Cut execution (GREEN phase)

Add the command execution in `execute_command`:

```rust
// Chunk: docs/chunks/clipboard_cut - Cut command execution
Command::Cut => {
    // Get selected text; if no selection, this is a no-op
    if let Some(text) = ctx.buffer.selected_text() {
        // Copy to clipboard first (before mutation)
        crate::clipboard::copy_to_clipboard(&text);
        // Delete the selection
        let dirty = ctx.buffer.delete_selection();
        ctx.mark_dirty(dirty);
        ctx.ensure_cursor_visible();
    }
    return;
}
```

Location: `crates/editor/src/buffer_target.rs`, in `execute_command`, adjacent to `Copy` and `Paste`.

All tests from Step 3 should now pass.

### Step 5: Update GOAL.md code_paths

Update the `code_paths` field in `docs/chunks/clipboard_cut/GOAL.md`:

```yaml
code_paths:
  - crates/editor/src/buffer_target.rs
```

This chunk only modifies `buffer_target.rs` — it reuses the existing `clipboard.rs` without modification.

## Dependencies

**Chunk dependencies:**
- `clipboard_operations` (ACTIVE): Provides `copy_to_clipboard()` and the mock clipboard infrastructure for tests.
- `text_selection_model` (ACTIVE): Provides `buffer.selected_text()` and `buffer.delete_selection()`.

**External dependencies:**
- None. All required functionality already exists in the codebase.

## Risks and Open Questions

**Low risk implementation**: This chunk is pure composition of existing, tested primitives. The only new code is:
1. One enum variant
2. One key binding match arm
3. ~8 lines of execution logic

**Potential edge case**: If a future implementation adds undo support, Cut should be undoable as a single operation. Currently undo is out of scope (per the GOAL.md note "If undo is supported"), so this is not a blocker.

**Clipboard side effects**: The clipboard write (`copy_to_clipboard`) is intentionally not undoable — this matches standard macOS behavior where Cmd+Z undoes the deletion but doesn't restore the previous clipboard content.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION, not at planning time. -->
