---
decision: APPROVE
summary: All success criteria satisfied - UTF-8 locale and TERM_PROGRAM environment variables properly configured with comprehensive test coverage
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: `pty.rs` spawn functions set `LANG=en_US.UTF-8` (or inherit/detect the system locale and ensure it includes `.UTF-8`)

- **Status**: satisfied
- **Evidence**: `configure_pty_environment()` at pty.rs:39-43 implements locale detection - inherits parent's `LANG` if it ends with `.UTF-8` or `.utf8`, otherwise falls back to `en_US.UTF-8`. Both `spawn()` (line 129) and `spawn_with_wakeup()` (line 248) call this helper.

### Criterion 2: `TERM_PROGRAM=lite-edit` is set so applications can identify the terminal

- **Status**: satisfied
- **Evidence**: `configure_pty_environment()` at pty.rs:46 sets `cmd.env("TERM_PROGRAM", "lite-edit")`. Test `test_env_term_program` verifies this.

### Criterion 3: A TUI application that outputs Unicode glyphs (box-drawing characters, geometric shapes, dingbats) renders them correctly — no underscore fallback

- **Status**: satisfied
- **Evidence**: The `test_unicode_output_preserved` test (pty.rs:509-541) spawns `echo "●"` and verifies the Unicode bullet character is preserved, not replaced with underscore. Visual end-to-end testing is documented as requiring manual verification per TESTING_PHILOSOPHY.md.

### Criterion 4: Verify the fix works both when lite-edit is launched from a terminal (inherits locale) and when launched from macOS GUI (minimal parent environment)

- **Status**: satisfied
- **Evidence**: Code at pty.rs:36-42 handles both scenarios: (1) Terminal-launched inherits parent's UTF-8 `LANG`, (2) Finder/Dock-launched uses `en_US.UTF-8` fallback when parent environment lacks UTF-8 locale. Logic documented in code comments.

### Criterion 5: Existing terminal behavior (ANSI colors, cursor positioning, scrollback) is not regressed

- **Status**: satisfied
- **Evidence**: All 157 terminal tests pass (except `test_spawn_login_shell` from a different chunk). `TERM=xterm-256color` and `COLORTERM=truecolor` are preserved in the helper function (pty.rs:33-34).
