<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk addresses a bug in the macOS NSEvent-to-KeyEvent conversion pipeline and verifies the existing Home/End keybinding support works end-to-end.

**The Core Problem:**
In `MetalView::convert_key` (crates/editor/src/metal_view.rs), when the Control modifier is held, macOS's `event.characters()` returns the *interpreted* control character rather than the underlying key. For example:
- Ctrl+A → `characters()` returns `\x01` (SOH control character)
- Ctrl+E → `characters()` returns `\x05` (ENQ control character)

The current code filters out control characters with `ch.is_control()` returning `None`, so **Ctrl+A and Ctrl+E are silently dropped** before reaching `resolve_command` in `buffer_target.rs`.

**The Fix:**
When the Control modifier is active, use `charactersIgnoringModifiers` instead of `characters`. This returns the unmodified base character ('a', 'e', etc.) regardless of what control character macOS would normally produce. This ensures we produce `KeyEvent { key: Key::Char('a'), modifiers: Modifiers { control: true, .. } }` which `resolve_command` already maps to `MoveToLineStart`.

**What Already Works:**
- Home/End keys use key codes (`0x73`, `0x77`) which are handled in the keycode match before we ever call `characters()` — these should work today.
- `resolve_command` correctly maps:
  - `Key::Home` → `MoveToLineStart`
  - `Key::End` → `MoveToLineEnd`
  - `Key::Char('a') + control` → `MoveToLineStart`
  - `Key::Char('e') + control` → `MoveToLineEnd`
- `TextBuffer::move_to_line_start` and `move_to_line_end` are implemented.

**Testing Strategy:**
Per TESTING_PHILOSOPHY.md, we test behavior at the `BufferFocusTarget::handle_key` level with synthetic `KeyEvent` inputs — this is the "update" function in our humble view architecture and is pure Rust without platform dependencies. The existing tests `test_ctrl_a_moves_to_line_start` and `test_ctrl_e_moves_to_line_end` verify this level. The fix in `convert_key` is platform code (humble object) that we verify works manually since it requires real NSEvent objects.

<!-- No subsystems exist yet in this project. -->

## Sequence

### Step 1: Fix `convert_key` to use `charactersIgnoringModifiers` for Control-modified keys

Location: `crates/editor/src/metal_view.rs`, in `MetalView::convert_key`

Modify the character-key handling logic:
1. After checking key codes for special keys (Return, Tab, etc.), before calling `event.characters()`:
2. Check if the Control modifier is active using `event.modifierFlags().contains(NSEventModifierFlags::Control)`
3. If Control is active, call `event.charactersIgnoringModifiers()` instead of `event.characters()`
4. This returns the base character ('a', 'e', etc.) instead of the control character (`\x01`, `\x05`)

The change is localized to the character-key handling section (after the keycode match, in the fallback to `characters()`).

**Implementation detail:** `charactersIgnoringModifiers` is available on NSEvent. We may need to add a binding if objc2-app-kit doesn't expose it — check the objc2 crate documentation. Worst case, use `msg_send!` to call it directly.

### Step 2: Verify existing tests pass for Ctrl+A and Ctrl+E

Location: `crates/editor/src/buffer_target.rs`

The tests `test_ctrl_a_moves_to_line_start` and `test_ctrl_e_moves_to_line_end` already exist and test at the `BufferFocusTarget::handle_key` level with synthetic `KeyEvent` inputs. Run these tests to confirm:
- They construct `KeyEvent::new(Key::Char('a'), Modifiers { control: true, .. })`
- They call `target.handle_key(event, &mut ctx)`
- They assert the cursor moved to line start/end

Run: `cargo test --package lite-edit-editor -p lite-edit-editor`

### Step 3: Add tests for Home and End keys at the BufferFocusTarget level

Location: `crates/editor/src/buffer_target.rs`

Add two new tests to verify the command resolver correctly maps Home/End keys:

```rust
#[test]
fn test_home_moves_to_line_start() {
    // Similar setup to test_ctrl_a_moves_to_line_start
    // Send KeyEvent::new(Key::Home, Modifiers::default())
    // Assert cursor is at column 0
}

#[test]
fn test_end_moves_to_line_end() {
    // Similar setup to test_ctrl_e_moves_to_line_end
    // Send KeyEvent::new(Key::End, Modifiers::default())
    // Assert cursor is at line end
}
```

These tests verify the resolve_command → execute_command pipeline for Home/End.

### Step 4: Verify no regressions in regular typing

Run the full test suite to ensure the change to `convert_key` doesn't break normal typing (characters without Control held should still work):

```bash
cargo test --package lite-edit-editor
cargo test --package lite-edit-buffer
```

Key tests that exercise typing: `test_typing_hello`, `test_typing_then_backspace`, `test_insert_at_empty_buffer`, etc.

### Step 5: Manual verification (platform code)

Since the `convert_key` fix is platform code (humble object), verify manually:
1. Build and run the editor: `cargo run`
2. Type some text to verify normal typing works
3. Press Home → cursor should jump to line start
4. Press End → cursor should jump to line end
5. Press Ctrl+A → cursor should jump to line start
6. Press Ctrl+E → cursor should jump to line end
7. Verify typing still works after using these keybindings (no regression)

## Dependencies

- **editable_buffer** (complete): Provides `TextBuffer::move_to_line_start`, `move_to_line_end`, and the `BufferFocusTarget` focus target
- **metal_surface** (complete): Provides the `MetalView` with `convert_key` that we're fixing
- **objc2-app-kit**: External crate for NSEvent API — need to verify `charactersIgnoringModifiers` is available or use `msg_send!`

## Risks and Open Questions

- **`charactersIgnoringModifiers` availability**: The objc2-app-kit crate may or may not expose this method. If not, we'll need to use `msg_send!` to call it directly. This is a minor implementation detail.

- **Other Control-modified keys**: This fix will also enable other Ctrl+key combinations to reach `resolve_command`. Currently, only Ctrl+A and Ctrl+E are mapped — other combinations will return `None` from `resolve_command` and be ignored (via `Handled::No`). This is the correct behavior.

- **Option key interactions**: The Option key on macOS also modifies `characters()` output (e.g., Option+E produces `´` for accent input). This chunk doesn't address Option-key handling — that's a separate concern for dead-key/accent input.

## Deviations

<!-- Populate during implementation -->