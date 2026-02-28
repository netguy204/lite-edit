---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/pty.rs
code_references:
  - ref: crates/terminal/src/pty.rs#get_utf8_lang
    implements: "UTF-8 locale detection with fallback for child process environment"
  - ref: crates/terminal/src/pty.rs#PtyHandle::spawn
    implements: "Sets LANG and TERM_PROGRAM environment variables for spawned PTY processes"
  - ref: crates/terminal/src/pty.rs#PtyHandle::spawn_with_wakeup
    implements: "Sets LANG and TERM_PROGRAM environment variables for PTY with wakeup support"
  - ref: crates/terminal/src/pty.rs#tests::test_unicode_environment_variables
    implements: "Verifies Unicode environment variables are propagated to child processes"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- app_nap_activity_assertions
- app_nap_blink_timer
- app_nap_file_watcher_pause
- highlight_text_source
- merge_conflict_render
- minibuffer_input
- terminal_single_pane_refresh
---

# Chunk Goal

## Minor Goal

Fix Unicode glyph rendering in the terminal by ensuring child processes receive
proper UTF-8 locale and terminal identification environment variables. Currently,
TUI applications (e.g., Claude Code) running inside lite-edit's terminal render
Unicode symbols (`●`, `□`, `└`, `├`, `✱`) as literal underscores `_` because
they detect insufficient Unicode support and fall back to ASCII output.

The PTY spawn code (`crates/terminal/src/pty.rs`) only sets `TERM` and
`COLORTERM` but omits `LANG`, `LC_ALL`, and `TERM_PROGRAM`. Many applications
use these to decide whether to emit Unicode characters. This is especially
problematic when lite-edit is launched from macOS Finder/Dock where the parent
environment is minimal.

## Success Criteria

- `pty.rs` spawn functions set `LANG=en_US.UTF-8` (or inherit/detect the
  system locale and ensure it includes `.UTF-8`) in the child process environment
- `TERM_PROGRAM=lite-edit` is set so applications can identify the terminal
- A TUI application that outputs Unicode glyphs (box-drawing characters, geometric
  shapes, dingbats) renders them correctly in lite-edit's terminal — no underscore
  fallback
- Verify the fix works both when lite-edit is launched from a terminal (inherits
  locale) and when launched from macOS GUI (minimal parent environment)
- Existing terminal behavior (ANSI colors, cursor positioning, scrollback) is not
  regressed

## Investigation Notes

The following was determined by tracing the rendering pipeline:

1. **Rendering pipeline is NOT the cause.** The glyph atlas fallback chain
   (`glyph_atlas.rs:596-643`) properly handles missing glyphs: primary font →
   Core Text fallback → U+FFFD → solid block. It never produces underscores.

2. **The underscores are literal `_` characters** output by the child application.
   They are too uniform (identical shape, size, position) to be rendering artifacts.

3. **Key evidence:** `└` (U+2514) is confirmed present in Menlo by font tests
   (`font.rs:574-594`), yet it still appears as `_`. This proves the character
   never reaches the renderer — the application chose not to emit it.

4. **Root cause:** `pty.rs:97-99` only sets `TERM=xterm-256color` and
   `COLORTERM=truecolor`. Node.js applications like Claude Code use libraries
   (e.g., `is-unicode-supported`, `figures`) that check locale environment
   variables to decide whether to output Unicode. Without `LANG=*.UTF-8`,
   these libraries may fall back to ASCII.