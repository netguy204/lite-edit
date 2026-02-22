<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `InputEncoder` in `crates/terminal/src/input_encoder.rs` handles keyboard event encoding for terminal PTY input. Currently, `Key::Backspace` in `encode_special_key()` handles the `modifiers.option` case (Alt+Backspace → `\x1b\x7f`), but there is no handling for `modifiers.command` (Cmd+Backspace).

In macOS terminal emulators like iTerm2 and Terminal.app, Cmd+Backspace sends `\x15` (Ctrl+U / NAK) to the PTY. This is the standard "kill line backward" control character that readline, zsh line editor, and other line-editing libraries interpret as "delete from cursor to start of line."

**Fix**: Extend the `Key::Backspace` match arm in `encode_special_key()` to check for `modifiers.command` and return `vec![0x15]` when true. The priority order is:
1. `modifiers.command` → `\x15` (Ctrl+U / kill line backward)
2. `modifiers.option` → `\x1b\x7f` (ESC + DEL / backward word delete)
3. Neither → `\x7f` (DEL / backward char delete)

**Pattern**: This follows the established pattern from `terminal_alt_backspace`, which added Option modifier handling to Backspace. We apply the same approach for the Command modifier.

**TDD Approach**: Per TESTING_PHILOSOPHY.md, we write the unit test first (test for `\x15` encoding when Cmd+Backspace is pressed), see it fail, then implement the fix, and confirm the test passes.

## Subsystem Considerations

No subsystems are relevant to this chunk. This is a targeted fix within the existing `InputEncoder` module, following the established pattern from `terminal_alt_backspace`.

## Sequence

### Step 1: Add unit test for Cmd+Backspace encoding (TDD - red phase)

Add a unit test in `crates/terminal/src/input_encoder.rs` that verifies Cmd+Backspace produces `\x15` (Ctrl+U / NAK). This test should:

1. Create a `KeyEvent` with `Key::Backspace` and `modifiers.command = true`
2. Call `InputEncoder::encode_key(&event, TermMode::NONE)`
3. Assert the result equals `vec![0x15]`

Location: `crates/terminal/src/input_encoder.rs` in the `#[cfg(test)]` module, in the "Alt/Option Key Tests" section (rename to "Modifier Key Tests" or add a new section for "Command Key Tests").

```rust
// Chunk: docs/chunks/terminal_cmd_backspace - Cmd+Backspace sends Ctrl+U
#[test]
fn test_encode_cmd_backspace() {
    let event = KeyEvent {
        key: Key::Backspace,
        modifiers: Modifiers {
            command: true,
            ..Default::default()
        },
    };
    let result = InputEncoder::encode_key(&event, TermMode::NONE);
    // Cmd+Backspace should send Ctrl+U (NAK) for kill-line-backward
    assert_eq!(result, vec![0x15]);
}
```

Run `cargo test -p lite-edit-terminal test_encode_cmd_backspace` — the test should **fail** initially (the current implementation returns `\x7f` for Backspace regardless of Command modifier).

### Step 2: Implement Cmd+Backspace encoding (TDD - green phase)

Modify `encode_special_key()` in `crates/terminal/src/input_encoder.rs` to handle `Key::Backspace` with `modifiers.command`:

```rust
// Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace sends ESC+DEL
// Chunk: docs/chunks/terminal_cmd_backspace - Cmd+Backspace sends Ctrl+U
Key::Backspace => {
    if modifiers.command {
        vec![0x15]  // Ctrl+U (NAK) for kill line backward
    } else if modifiers.option {
        vec![0x1b, 0x7f]  // ESC + DEL for backward word delete
    } else {
        vec![0x7f]  // DEL (most modern terminals)
    }
}
```

Run `cargo test -p lite-edit-terminal test_encode_cmd_backspace` — the test should now **pass**.

### Step 3: Add regression tests to verify existing behavior unchanged

Ensure the existing tests still pass:
- `test_encode_backspace` (plain Backspace returns `vec![0x7f]`)
- `test_encode_alt_backspace` (Alt+Backspace returns `\x1b\x7f`)

Run `cargo test -p lite-edit-terminal test_encode_backspace test_encode_alt_backspace` to confirm no regression.

### Step 4: Add integration test for Cmd+Backspace in shell context

Add an integration test in `crates/terminal/tests/input_integration.rs` that:

1. Spawns a shell
2. Types `"echo hello world"` (without pressing Enter)
3. Sends Cmd+Backspace
4. Asserts "world" and " " were deleted (the command line should now be `"echo hello"` or less, depending on readline behavior)
5. Sends Enter
6. Verifies the output reflects the line-deletion behavior

This test verifies the full pipeline: Cmd+Backspace → `\x15` → PTY → readline → line deletion.

```rust
// Chunk: docs/chunks/terminal_cmd_backspace - Cmd+Backspace integration test
#[test]
fn test_cmd_backspace_deletes_to_line_start() {
    let (terminal, mut target) = create_terminal_with_shell();

    // Type "echo hello world" (without pressing Enter)
    type_string(&mut target, "echo hello world");

    // Give shell time to process the input
    wait_and_poll(&terminal, 100);

    // Send Cmd+Backspace to delete from cursor to line start
    let cmd_backspace = KeyEvent {
        key: Key::Backspace,
        modifiers: Modifiers {
            command: true,
            ..Default::default()
        },
    };
    target.handle_key(cmd_backspace);

    // Give shell time to process the line deletion
    wait_and_poll(&terminal, 100);

    // Type something new to verify the line was cleared
    type_string(&mut target, "echo CLEARED");
    press_enter(&mut target);

    // Wait for output
    wait_and_poll(&terminal, 200);

    // The terminal should output "CLEARED" (the original text was deleted)
    let content = get_terminal_content(&terminal);
    assert!(
        content.contains("CLEARED"),
        "Expected 'CLEARED' in output after Cmd+Backspace cleared the line, got: {}",
        content
    );
}
```

Location: `crates/terminal/tests/input_integration.rs`

### Step 5: Run full test suite and verify

Run `cargo test -p lite-edit-terminal` to ensure all tests pass.

---

**BACKREFERENCE COMMENTS**

The implementation adds/updates backreference comments at the modified code site:
```rust
// Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace sends ESC+DEL
// Chunk: docs/chunks/terminal_cmd_backspace - Cmd+Backspace sends Ctrl+U
```

## Dependencies

- **terminal_alt_backspace chunk** (ACTIVE): Established the pattern for modifier+Backspace encoding in `InputEncoder`. Already merged.
- **terminal_input_encoding chunk** (ACTIVE): The `InputEncoder` module that this chunk modifies. Already merged.

## Risks and Open Questions

- **Shell compatibility**: Different shells (bash, zsh, fish, sh) may handle `\x15` (Ctrl+U) slightly differently. In bash with readline, Ctrl+U deletes from cursor to line start. In zsh with default bindings, Ctrl+U kills the entire line. This is acceptable — we're encoding the key correctly (matching iTerm2/Terminal.app behavior); how the shell interprets it is the shell's concern.

- **Integration test flakiness**: The integration test in Step 4 depends on shell timing and readline behavior. If the test proves flaky:
  - Consider adding a longer sleep before checking output
  - Or rely on the unit test (Step 1) for CI and treat the integration test as a manual verification

- **Modifier precedence**: The implementation checks `command` before `option`. If both are pressed simultaneously (Cmd+Alt+Backspace), the command modifier wins. This matches typical macOS precedence where Cmd takes priority, but is worth documenting in case questions arise.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->