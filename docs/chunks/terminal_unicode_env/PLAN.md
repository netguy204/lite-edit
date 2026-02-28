<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix involves extending the environment variable setup in `crates/terminal/src/pty.rs`
to ensure child processes receive proper UTF-8 locale information and terminal
identification. The current implementation only sets `TERM` and `COLORTERM`, which is
insufficient for applications that use locale environment variables to decide whether
to emit Unicode characters.

**Strategy:**

1. **Add locale environment variables**: Set `LANG` to ensure UTF-8 encoding is
   signaled to child processes. The approach will:
   - First attempt to inherit the parent's `LANG` if it ends with `.UTF-8`
   - Fall back to `en_US.UTF-8` if the parent environment lacks a UTF-8 locale
   - This handles both terminal-launched scenarios (inherits user's locale) and
     Finder/Dock-launched scenarios (minimal environment, falls back to explicit UTF-8)

2. **Add `TERM_PROGRAM=lite-edit`**: This allows applications to identify the terminal
   emulator, which some programs use for capability detection.

3. **Optionally set `LC_ALL`**: Some applications check `LC_ALL` before `LANG`. Setting
   `LC_ALL` can override other `LC_*` variables, but setting `LANG` alone should be
   sufficient for most use cases since `LC_ALL` is typically not set by default on macOS.

**Existing code to build on:**

- `pty.rs:97-99` (in `spawn()`) and `pty.rs:217-219` (in `spawn_with_wakeup()`)
  already set `TERM` and `COLORTERM`. We extend this pattern.
- Both spawn functions share the same environment setup logic, so both must be updated.

**Testing approach per TESTING_PHILOSOPHY.md:**

- The core behavior (environment variable propagation) can be tested by spawning a
  process that echoes environment variables and verifying the expected values.
- This is testable in a unit test without GUI/platform dependencies.
- Visual Unicode rendering (box-drawing, geometric shapes) is a "humble view" concern
  and cannot be unit-tested; manual verification is appropriate for that aspect.

<!-- No subsystems are relevant to this chunk.
     The renderer and viewport_scroll subsystems do not govern PTY environment setup. -->

## Sequence

### Step 1: Write failing tests for environment variable propagation

**Location:** `crates/terminal/src/pty.rs` (in the `#[cfg(test)]` module)

Create tests that verify environment variables are properly set in child processes:

1. `test_env_lang_utf8` — Spawns `printenv LANG` and verifies the output contains
   `.UTF-8`. This tests the locale environment variable is set.

2. `test_env_term_program` — Spawns `printenv TERM_PROGRAM` and verifies the output
   is `lite-edit`. This tests the terminal identification is set.

3. `test_unicode_output_preserved` — Spawns `echo` with a Unicode character (e.g.,
   `echo "●"`) and verifies the exact character is received (not an underscore or
   replacement). This is a regression test for the original bug.

These tests should fail initially because the environment variables are not yet set.

### Step 2: Extract environment setup into a helper function

**Location:** `crates/terminal/src/pty.rs`

Currently, both `spawn()` and `spawn_with_wakeup()` have identical environment setup
code:

```rust
cmd_builder.env("TERM", "xterm-256color");
cmd_builder.env("COLORTERM", "truecolor");
```

Extract this into a private helper function `configure_pty_environment()` that takes
a mutable `CommandBuilder` reference and sets all required environment variables.
This ensures both spawn functions stay in sync and reduces duplication.

**Signature:**
```rust
// Chunk: docs/chunks/terminal_unicode_env - UTF-8 locale environment setup
fn configure_pty_environment(cmd: &mut CommandBuilder) {
    // Environment setup logic here
}
```

Add a chunk backreference comment to the helper function.

### Step 3: Implement locale detection and UTF-8 environment variables

**Location:** `crates/terminal/src/pty.rs` (in `configure_pty_environment`)

Implement the locale detection logic:

```rust
// 1. Check if parent process has a UTF-8 locale set
let lang = std::env::var("LANG")
    .ok()
    .filter(|v| v.ends_with(".UTF-8") || v.ends_with(".utf8"))
    .unwrap_or_else(|| "en_US.UTF-8".to_string());

// 2. Set LANG for the child process
cmd.env("LANG", &lang);

// 3. Set TERM_PROGRAM to identify lite-edit
cmd.env("TERM_PROGRAM", "lite-edit");
```

This approach:
- Inherits the user's locale if it's UTF-8 (respects user preferences)
- Falls back to `en_US.UTF-8` for Finder/Dock launches or non-UTF-8 parent locales
- Sets `TERM_PROGRAM` for application identification

**Note:** We do NOT set `LC_ALL` because:
- `LC_ALL` overrides all other `LC_*` variables, which could override user preferences
- `LANG` is sufficient for UTF-8 signaling
- macOS does not set `LC_ALL` by default, and we should match that behavior

### Step 4: Update `spawn()` to use the helper

**Location:** `crates/terminal/src/pty.rs`, lines 97-99

Replace:
```rust
// Set up environment
cmd_builder.env("TERM", "xterm-256color");
cmd_builder.env("COLORTERM", "truecolor");
```

With:
```rust
// Set up environment (terminal capabilities + UTF-8 locale)
configure_pty_environment(&mut cmd_builder);
```

### Step 5: Update `spawn_with_wakeup()` to use the helper

**Location:** `crates/terminal/src/pty.rs`, lines 217-219

Replace:
```rust
// Set up environment
cmd_builder.env("TERM", "xterm-256color");
cmd_builder.env("COLORTERM", "truecolor");
```

With:
```rust
// Set up environment (terminal capabilities + UTF-8 locale)
configure_pty_environment(&mut cmd_builder);
```

### Step 6: Run tests and verify they pass

Run the new environment variable tests:
```bash
cargo test -p terminal test_env_lang_utf8
cargo test -p terminal test_env_term_program
cargo test -p terminal test_unicode_output_preserved
```

Also run the existing tests to ensure no regressions:
```bash
cargo test -p terminal
```

### Step 7: Manual verification with a TUI application

After the automated tests pass, manually verify the fix works end-to-end:

1. Build lite-edit
2. Open a terminal tab
3. Run a TUI application that uses Unicode (e.g., Claude Code, or `echo "● □ └ ├ ✱"`)
4. Verify the Unicode characters render correctly (not as underscores)
5. Test both scenarios:
   - Launch lite-edit from a terminal (inherits locale)
   - Launch lite-edit from Finder/Dock (minimal environment, uses fallback)

---

**BACKREFERENCE COMMENTS**

Add a chunk backreference to the `configure_pty_environment()` helper function:

```rust
// Chunk: docs/chunks/terminal_unicode_env - UTF-8 locale environment setup
fn configure_pty_environment(cmd: &mut CommandBuilder) { ... }
```

## Dependencies

None. The `portable-pty` crate's `CommandBuilder::env()` method is already available
and used by the existing code. No new dependencies are required.

## Risks and Open Questions

1. **Locale availability:** `en_US.UTF-8` should be available on all macOS systems,
   but if a system has a minimal locale configuration, the fallback might fail.
   Risk is low because macOS ships with UTF-8 locales by default.

2. **Inheriting non-UTF-8 locales:** If the parent process has `LANG=C` or another
   non-UTF-8 locale, we fall back to `en_US.UTF-8`. This is intentional — the goal
   is to enable Unicode rendering, and `C` locale would defeat that purpose.

3. **`TERM_PROGRAM` conflicts:** Some applications check `TERM_PROGRAM` and may have
   special behavior for known terminals. Setting `lite-edit` is unlikely to trigger
   unintended behavior, but applications won't have lite-edit-specific code paths.
   This is acceptable — we're signaling identity, not requesting special treatment.

4. **Test reliability:** The environment variable tests rely on `printenv` being
   available, which is standard on macOS but worth noting. The tests should be
   platform-appropriate.

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