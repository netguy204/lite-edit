---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/terminal_buffer.rs
  - crates/terminal/tests/integration.rs
code_references:
  - ref: crates/terminal/src/terminal_buffer.rs#EventSender
    implements: "Channel-based EventListener that captures terminal events (DSR responses, title changes, etc.) for forwarding"
  - ref: crates/terminal/src/terminal_buffer.rs#EventSender::send_event
    implements: "Forwards Event::PtyWrite through the channel for write-back to PTY"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::poll_events
    implements: "Processes terminal events from alacritty_terminal and writes CPR responses back to PTY"
  - ref: crates/terminal/tests/integration.rs#test_dsr_cursor_position_report
    implements: "Integration test verifying DSR/CPR round-trip works correctly"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- viewport_emacs_navigation
- pane_scroll_isolation
---

# Chunk Goal

## Minor Goal

Programs running inside the integrated terminal (e.g. Claude Code) issue DSR (Device Status Report) escape sequences (`ESC[6n`) to query the cursor's current position. The terminal emulator must respond with a CPR (Cursor Position Report, `ESC[<row>;<col>R`) written back to the PTY. Without this, programs that depend on cursor position — input prompts, TUI layouts, readline-style editing — render their cursor in the wrong location.

Observed symptom: In Claude Code running inside lite-edit's terminal, the text cursor renders one row below the actual input line (on the "bypass permissions" status bar instead of the prompt line). This indicates that either (a) DSR requests are silently dropped and the program falls back to a wrong default, or (b) CPR responses are generated with incorrect row/column values.

This chunk implements correct DSR/CPR round-trip handling so that programs running in the terminal can accurately determine cursor position, which is essential to the project goal of making terminal tabs behave like a normal, fully functional terminal emulator.

## Success Criteria

- The terminal emulator intercepts DSR escape sequences (`ESC[6n`) from the hosted program
- A correct CPR response (`ESC[<row>;<col>R`) is written back to the PTY via `PtyHandle::write()`
- Row and column values in the CPR response are 1-indexed per ANSI standard and account for scrollback offset
- Claude Code (or another readline-based program) running in a lite-edit terminal tab positions its cursor on the correct row — the same row as the prompt caret, not one row below