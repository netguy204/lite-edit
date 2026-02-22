<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `InputEncoder` in `crates/terminal/src/input_encoder.rs` handles keyboard event encoding for terminal PTY input. Currently, `Key::Backspace` in `encode_special_key()` returns `vec![0x7f]` (DEL) unconditionally, ignoring modifiers.

The fix is minimal: when `Key::Backspace` is pressed with `modifiers.option == true`, we return `\x1b\x7f` (ESC + DEL) instead. This matches what real terminal emulators like iTerm2 and Terminal.app send, and is the standard escape sequence that readline/zsh line editor interpret as "delete word backward."

**Pattern**: This follows the existing pattern in `encode_char()` where `modifiers.option` causes an ESC prefix to be prepended. We apply the same logic to the Backspace special key.

**TDD Approach**: Per TESTING_PHILOSOPHY.md, we write the unit test first (test for `\x1b\x7f` encoding when Alt+Backspace is pressed), see it fail, then implement the fix, and confirm the test passes.

## Subsystem Considerations

No subsystems are relevant to this chunk. This is a targeted fix within the existing `InputEncoder` module.

## Sequence

### Step 1: Add unit test for Alt+Backspace encoding (TDD - red phase)

Add a unit test in `crates/terminal/src/input_encoder.rs` that verifies Alt+Backspace produces `\x1b\x7f` (ESC + DEL). This test should:

1. Create a `KeyEvent` with `Key::Backspace` and `modifiers.option = true`
2. Call `InputEncoder::encode_key(&event, TermMode::NONE)`
3. Assert the result equals `b"\x1b\x7f"`

Location: `crates/terminal/src/input_encoder.rs` in the `#[cfg(test)]` module, near the existing `test_encode_backspace` test.

Run `cargo test -p lite-edit-terminal test_encode_alt_backspace` — the test should **fail** initially.

### Step 2: Implement Alt+Backspace encoding (TDD - green phase)

Modify `encode_special_key()` in `crates/terminal/src/input_encoder.rs` to handle `Key::Backspace` with `modifiers.option`:

```rust
Key::Backspace => {
    // Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace sends ESC+DEL
    if modifiers.option {
        vec![0x1b, 0x7f]  // ESC + DEL for backward word delete
    } else {
        vec![0x7f]  // DEL
    }
}
```

Run `cargo test -p lite-edit-terminal test_encode_alt_backspace` — the test should now **pass**.

### Step 3: Add regression test to verify plain Backspace still works

Ensure the existing `test_encode_backspace` test still passes. This test already exists and asserts `Key::Backspace` without modifiers returns `vec![0x7f]`.

Run `cargo test -p lite-edit-terminal test_encode_backspace` to confirm no regression.

### Step 4: Add integration test for Alt+Backspace in shell context

Add an integration test in `crates/terminal/tests/input_integration.rs` that:

1. Spawns a shell
2. Types `"echo hello world"` (without pressing Enter)
3. Sends Alt+Backspace
4. Asserts `"world"` was deleted (the command line should now be `"echo hello "`)
5. Sends Enter
6. Verifies output is `"hello "` (just "hello " with trailing space)

This test verifies the full pipeline: Alt+Backspace → `\x1b\x7f` → PTY → readline → word deletion.

Location: `crates/terminal/tests/input_integration.rs`

### Step 5: Run full test suite and verify

Run `cargo test -p lite-edit-terminal` to ensure all tests pass.

---

**BACKREFERENCE COMMENTS**

The implementation adds a backreference comment at the modified code site:
```rust
// Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace sends ESC+DEL
```

## Dependencies

- **terminal_input_encoding chunk** (ACTIVE): The `InputEncoder` module that this chunk modifies. Already merged.

## Risks and Open Questions

- **Shell compatibility**: Different shells (bash, zsh, fish, sh) may handle `\x1b\x7f` slightly differently. The escape sequence `\x1b\x7f` is standard for "delete word backward" in readline-compatible shells, but the exact word boundary behavior may vary. This is acceptable — we're encoding the key correctly; how the shell interprets it is the shell's concern.

- **Integration test flakiness**: The integration test in Step 4 depends on shell timing and readline behavior. If the test proves flaky:
  - Consider adding a longer sleep before checking output
  - Or rely on the unit test (Step 1) for CI and treat the integration test as a manual verification

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->