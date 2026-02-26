<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a minimal-touch keybinding addition following the existing pattern in `resolve_command()`. The strategy is straightforward:

1. Add three new match arms in `resolve_command()` that map Ctrl+D, Ctrl+N, and Ctrl+P to their respective existing commands
2. Follow the established pattern: `Key::Char(ch) if mods.control && !mods.command => Some(Command::*)`
3. Add tests following the TDD approach from docs/trunk/TESTING_PHILOSOPHY.md

The commands (`DeleteForward`, `MoveDown`, `MoveUp`) already exist and are fully implemented. The execution path in `execute_command()` already handles them. This chunk only adds alternative key triggers.

The implementation mirrors how the existing Emacs bindings were added:
- Ctrl+F → `MoveRight` (same as Right arrow)
- Ctrl+B → `MoveLeft` (same as Left arrow)
- Ctrl+V → `PageDown` (same as Page Down key)

Now we're adding:
- Ctrl+D → `DeleteForward` (same as Delete key)
- Ctrl+N → `MoveDown` (same as Down arrow)
- Ctrl+P → `MoveUp` (same as Up arrow)

## Subsystem Considerations

No subsystems are directly relevant to this change. The keybinding resolution is self-contained in `buffer_target.rs` and doesn't interact with cross-cutting patterns. The `viewport_scroll` subsystem is used by the existing `MoveDown`/`MoveUp` command execution, but this chunk doesn't modify that behavior—it only adds new key triggers.

## Sequence

### Step 1: Write failing tests for the new keybindings

Following TDD, add tests in `buffer_target.rs` that verify:

1. `test_ctrl_d_resolves_to_delete_forward` - Ctrl+D maps to `Command::DeleteForward`
2. `test_ctrl_n_resolves_to_move_down` - Ctrl+N maps to `Command::MoveDown`
3. `test_ctrl_p_resolves_to_move_up` - Ctrl+P maps to `Command::MoveUp`

These tests should call `resolve_command()` with synthetic `KeyEvent` objects and assert the expected `Command` variant is returned. This matches the existing test pattern used for Ctrl+F, Ctrl+B, and Ctrl+V (see lines 4743-4770).

Location: `crates/editor/src/buffer_target.rs`, in the `#[cfg(test)]` module

### Step 2: Add the three keybinding match arms in resolve_command

Add match arms for Ctrl+D, Ctrl+N, and Ctrl+P in the `resolve_command()` function. Place them after the existing Ctrl+B binding (line 247) and before the `_` catch-all.

Pattern to follow (from existing code):
```rust
// Ctrl+F → forward-char (move cursor right)
Key::Char('f') if mods.control && !mods.command => Some(Command::MoveRight),

// Ctrl+B → backward-char (move cursor left)
Key::Char('b') if mods.control && !mods.command => Some(Command::MoveLeft),
```

New bindings to add:
```rust
// Chunk: docs/chunks/emacs_keybindings - Ctrl+D/N/P Emacs bindings
// Ctrl+D → delete-char (delete character under cursor)
Key::Char('d') if mods.control && !mods.command => Some(Command::DeleteForward),

// Ctrl+N → next-line (move cursor down)
Key::Char('n') if mods.control && !mods.command => Some(Command::MoveDown),

// Ctrl+P → previous-line (move cursor up)
Key::Char('p') if mods.control && !mods.command => Some(Command::MoveUp),
```

Location: `crates/editor/src/buffer_target.rs`, in `resolve_command()` around line 248

### Step 3: Run tests and verify all pass

Run the test suite to verify:
1. The new tests pass (Ctrl+D/N/P resolve to correct commands)
2. Existing tests still pass (no regressions to Delete, Down, Up arrow keys)

Command: `cargo test -p editor`

### Step 4: Update the chunk GOAL.md code_paths

Add `crates/editor/src/buffer_target.rs` to the `code_paths` frontmatter field.

---

**BACKREFERENCE COMMENTS**

Add a backreference comment before the new keybinding block:
```rust
// Chunk: docs/chunks/emacs_keybindings - Ctrl+D/N/P Emacs bindings
```

This links the code back to this chunk for future archaeology.

## Dependencies

None. All required commands and their execution logic already exist:
- `Command::DeleteForward` - bound to `Key::Delete`, calls `ctx.buffer.delete_forward()`
- `Command::MoveDown` - bound to `Key::Down`, calls `ctx.buffer.move_down()`
- `Command::MoveUp` - bound to `Key::Up`, calls `ctx.buffer.move_up()`

The macOS event handling for Ctrl+letter keys was fixed in the `line_nav_keybindings` chunk (using `charactersIgnoringModifiers` when Control is held).

## Risks and Open Questions

**Ctrl+D conflict with Option+D**: Option+D is already bound to `DeleteForwardWord`. These use different modifiers (Control vs Option) so there's no conflict. However, verify the guard conditions are mutually exclusive:
- Option+D: `Key::Char('d') if mods.option && !mods.command`
- Ctrl+D: `Key::Char('d') if mods.control && !mods.command`

**Ctrl+N/P in other contexts**: These bindings apply only to the buffer focus target. Terminal tabs pass all keys through to the PTY. The file picker and mini-buffer have their own key handling. No changes needed there—the Emacs bindings should only affect buffer editing.

**Match arm ordering**: The new match arms must come after more specific guards (like Shift+Ctrl combinations) but before the catch-all. The existing pattern places Ctrl+letter bindings in the "Movement commands" section after selection commands.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->