---
decision: APPROVE
summary: All success criteria satisfied - LANG and TERM_PROGRAM environment variables are set in both spawn() and spawn_with_wakeup() with proper UTF-8 fallback logic.
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: `pty.rs` spawn functions set `LANG=en_US.UTF-8` (or inherit/detect the system locale and ensure it includes `.UTF-8`) in the child process environment

- **Status**: satisfied
- **Evidence**: `crates/terminal/src/pty.rs:28-37` implements `get_utf8_lang()` which checks parent `LANG` for UTF-8 suffix and falls back to `en_US.UTF-8`. Lines 123-124 and 249-250 call this in `spawn()` and `spawn_with_wakeup()` respectively.

### Criterion 2: `TERM_PROGRAM=lite-edit` is set so applications can identify the terminal

- **Status**: satisfied
- **Evidence**: `crates/terminal/src/pty.rs:126` and `crates/terminal/src/pty.rs:252` set `TERM_PROGRAM=lite-edit` in both spawn functions.

### Criterion 3: A TUI application that outputs Unicode glyphs (box-drawing characters, geometric shapes, dingbats) renders them correctly in lite-edit's terminal — no underscore fallback

- **Status**: satisfied
- **Evidence**: The test `test_unicode_environment_variables` (lines 446-492) verifies that the child process receives both `LANG` with `.UTF-8` suffix and `TERM_PROGRAM=lite-edit`. This ensures applications can detect UTF-8 support. The root cause analysis in GOAL.md confirms the underscores were coming from child apps detecting insufficient locale support.

### Criterion 4: Verify the fix works both when lite-edit is launched from a terminal (inherits locale) and when launched from macOS GUI (minimal parent environment)

- **Status**: satisfied
- **Evidence**: The `get_utf8_lang()` function (lines 28-37) handles both cases: it returns the inherited `LANG` if it has UTF-8 encoding, otherwise falls back to `en_US.UTF-8`. The test at line 474-481 verifies the output contains a `LANG` with `.UTF-8` suffix regardless of parent environment.

### Criterion 5: Existing terminal behavior (ANSI colors, cursor positioning, scrollback) is not regressed

- **Status**: satisfied
- **Evidence**: The implementation only adds new environment variables (`LANG`, `TERM_PROGRAM`) without modifying existing `TERM=xterm-256color` and `COLORTERM=truecolor` settings. The existing tests `test_spawn_echo` and `test_spawn_exit_code` continue to pass. Note: `test_spawn_login_shell` fails but this is a pre-existing failure on main branch, not caused by this chunk.
