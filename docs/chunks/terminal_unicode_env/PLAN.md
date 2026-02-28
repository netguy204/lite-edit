<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Extend the PTY spawn code to set additional environment variables that signal
UTF-8 support to child processes. The fix is localized to `crates/terminal/src/pty.rs`
where environment setup currently only sets `TERM` and `COLORTERM`.

**Strategy:**

1. **Locale detection with fallback**: First check if the parent environment has a
   `LANG` variable with a UTF-8 encoding. If present, inherit it. If absent or
   non-UTF-8, explicitly set `LANG=en_US.UTF-8` to guarantee UTF-8 output.

2. **Terminal identification**: Set `TERM_PROGRAM=lite-edit` and optionally
   `TERM_PROGRAM_VERSION={workspace version}` so applications can identify the
   terminal emulator. This follows the convention used by iTerm2, Terminal.app,
   VS Code, and other terminal emulators.

3. **Test-driven approach**: Following TESTING_PHILOSOPHY.md, write a test first
   that verifies Unicode environment variables are present in the child process,
   then implement the fix.

**Why this approach works:**

- Applications like Node.js's `is-unicode-supported` check `LC_ALL`, `LC_CTYPE`,
  or `LANG` for `.UTF-8` suffix to decide whether to output Unicode.
- `TERM_PROGRAM` helps applications tailor behavior (e.g., hyperlinks, image
  protocols) and aids debugging.
- Inheriting when available preserves user locale preferences; falling back to
  `en_US.UTF-8` provides a sane default for Finder/Dock launches.

## Sequence

### Step 1: Write failing test for Unicode environment variables

Add a test that spawns a PTY and verifies the child process receives the required
environment variables. The test should:

1. Spawn a PTY with an explicit command (`/bin/sh -c 'env'`) to print environment
2. Verify output contains `LANG` with `.UTF-8` suffix
3. Verify output contains `TERM_PROGRAM=lite-edit`

The test will fail initially because these variables are not yet set.

Location: `crates/terminal/src/pty.rs` in the `#[cfg(test)]` module

### Step 2: Add helper function for UTF-8 locale detection

Create a helper function `get_utf8_lang()` that:

1. Checks `std::env::var("LANG")`
2. If present and ends with `.UTF-8` (case-insensitive), return it
3. Otherwise, return `"en_US.UTF-8"` as the fallback

This isolates the locale detection logic for testability and reuse across both
`spawn()` and `spawn_with_wakeup()`.

Location: `crates/terminal/src/pty.rs` (module-level function, not exported)

### Step 3: Set environment variables in `spawn()`

In the `spawn()` function, after the existing `TERM` and `COLORTERM` setup,
add the new environment variables:

```rust
// Locale for UTF-8 support (inherit or fallback)
cmd_builder.env("LANG", get_utf8_lang());

// Terminal identification
cmd_builder.env("TERM_PROGRAM", "lite-edit");
```

Add a chunk backreference comment to mark this as the fix location.

Location: `crates/terminal/src/pty.rs:97-99`

### Step 4: Set environment variables in `spawn_with_wakeup()`

Apply the same environment variable setup to the `spawn_with_wakeup()` function.
The duplication is acceptable here because both functions have nearly identical
setup code, and extracting a shared builder would increase complexity without
significant benefit.

Location: `crates/terminal/src/pty.rs:217-219`

### Step 5: Run tests and verify fix

1. Run `cargo test -p lite-edit-terminal` to verify the new test passes
2. Manually test by launching lite-edit and running a Unicode-heavy TUI application
   (e.g., `node -e "console.log('●□└├✱')"` or the actual Claude Code app)
3. Test both scenarios:
   - Launch from terminal (inherits locale): should use inherited `LANG`
   - Launch from Finder/Dock (minimal env): should use fallback `en_US.UTF-8`

Location: Terminal

### Step 6: Verify no regression in existing tests

Run the full test suite to ensure the changes don't break existing PTY behavior:

```bash
cargo test
```

Location: Terminal

## Risks and Open Questions

1. **Locale availability**: The fallback `en_US.UTF-8` assumes this locale is
   available on the system. On macOS this is always true. If we ever support
   other platforms, this may need adjustment.

2. **`LC_ALL` override**: Some users may have `LC_ALL` set, which overrides
   `LANG`. We could optionally set `LC_ALL` as well for stronger guarantees.
   However, this could conflict with user preferences. Starting with just
   `LANG` is conservative; we can add `LC_ALL` if needed.

3. **Version injection**: Including `TERM_PROGRAM_VERSION` would require passing
   the version from Cargo.toml at compile time (e.g., via `env!("CARGO_PKG_VERSION")`).
   This is low priority for the initial fix and can be added later if useful.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->