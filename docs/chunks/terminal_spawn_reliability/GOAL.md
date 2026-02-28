---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/workspace.rs
- crates/editor/src/editor_state.rs
- crates/terminal/src/pty.rs
code_references:
  - ref: crates/editor/src/workspace.rs#ErrorBuffer
    implements: "Read-only buffer displaying error message and retry hint for failed terminal spawns"
  - ref: crates/editor/src/workspace.rs#TabBuffer::Error
    implements: "TabBuffer variant wrapping ErrorBuffer for failed terminal spawns"
  - ref: crates/editor/src/workspace.rs#Tab::new_error
    implements: "Constructor for error tabs with TabKind::Terminal visual treatment"
  - ref: crates/editor/src/workspace.rs#Tab::is_error_tab
    implements: "Detection method for error tabs to enable retry handling"
  - ref: crates/editor/src/editor_state.rs#EditorState::new_terminal_tab
    implements: "Terminal spawn with error state fallback instead of dead tab"
  - ref: crates/editor/src/editor_state.rs#EditorState::retry_terminal_spawn
    implements: "Replace error tab with new terminal spawn attempt"
  - ref: crates/terminal/src/pty.rs#PtyHandle
    implements: "PTY handle with reader_done_rx channel for timed join on Drop"
  - ref: crates/terminal/src/pty.rs#PtyHandle::Drop
    implements: "Timed join (100ms) of reader thread to prevent PTY fd leaks"
narrative: null
investigation: terminal_shell_flakiness
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_unicode_env
- incremental_parse
- tab_rendering
---

# Chunk Goal

## Minor Goal

Fix two bugs that cause terminal tabs to occasionally be non-functional:

1. **`new_terminal_tab` swallows spawn errors**: When `spawn_shell()` fails (e.g., `openpty` returns `ENXIO`), the error is logged to stderr and a dead tab is created with `pty: None`. The user sees a "Terminal" tab that accepts no input and shows no output, with no indication of failure. The tab should instead enter an error state — analogous to Chrome's "something went wrong" tab — that displays the error message and offers a way to retry.

2. **`PtyHandle::Drop` leaks PTY file descriptors**: The Drop impl kills the child process but detaches the reader thread without joining it (`self.reader_thread.take()`). The reader thread may still hold the PTY master fd, keeping the PTY device allocated in the kernel. This can cause subsequent `openpty` calls to fail with `ENXIO` if old PTYs haven't been fully released.

Together these create a feedback loop: leaked PTY fds from closed terminals cause the next spawn to fail, and the swallowed error creates a dead tab instead of surfacing the problem.

## Success Criteria

- When `spawn_shell` fails, the tab enters an error state that renders an error message (e.g., "Failed to create terminal: {error}") and offers a retry action
- A new `TabBuffer` variant (e.g., `Error { message, retry }`) or equivalent mechanism supports this state
- `PtyHandle::Drop` joins the reader thread with a brief timeout (e.g., 100ms) before detaching, ensuring PTY fds are released promptly in the common case
- Existing terminal tests continue to pass
- The contention experiment from the investigation (spawning 10 shells simultaneously) should not regress

## Root Cause Evidence

See `docs/investigations/terminal_shell_flakiness/OVERVIEW.md` for the full investigation, including:
- Contention experiment showing `openpty` fails with `ENXIO` at 25+ concurrent PTYs
- The error-swallowing code at `crates/editor/src/editor_state.rs:4442-4445`
- The reader-thread-detach code at `crates/terminal/src/pty.rs:371-386`
