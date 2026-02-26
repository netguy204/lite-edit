---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/Cargo.toml
- crates/editor/src/activity_assertion.rs
- crates/editor/src/main.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/editor_event.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/event_channel.rs
code_references:
  - ref: crates/editor/src/activity_assertion.rs#ActivityAssertion
    implements: "NSProcessInfo activity assertion wrapper for App Nap prevention"
  - ref: crates/editor/src/activity_assertion.rs#ActivityAssertion::hold
    implements: "Acquires activity assertion when terminal activity begins"
  - ref: crates/editor/src/activity_assertion.rs#ActivityAssertion::release
    implements: "Releases activity assertion to allow App Nap when idle"
  - ref: crates/editor/src/editor_state.rs#EditorState::poll_agents
    implements: "Terminal activity tracking - updates last_terminal_activity and holds assertion"
  - ref: crates/editor/src/editor_state.rs#EditorState::toggle_cursor_blink
    implements: "2-second quiescence timeout check that releases assertion when terminals are idle"
  - ref: crates/editor/src/editor_state.rs#EditorState::release_activity_assertion
    implements: "Immediate assertion release on window resign key"
  - ref: crates/editor/src/editor_event.rs#EditorEvent::WindowResignKey
    implements: "Event variant for window backgrounding notification"
  - ref: crates/editor/src/event_channel.rs#EventSender::send_window_resign_key
    implements: "Sends WindowResignKey event from delegate to drain loop"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::handle_window_resign_key
    implements: "Handles WindowResignKey by releasing activity assertion"
  - ref: crates/editor/src/main.rs#WindowDelegate::window_did_resign_key
    implements: "windowDidResignKey delegate that sends WindowResignKey event"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- app_nap_blink_timer
created_after:
- buffer_file_watching
- highlight_injection
---

# Chunk Goal

## Minor Goal

Use `NSProcessInfo` activity assertions (`beginActivity` / `endActivity`) to
explicitly communicate to macOS when lite-edit is doing latency-sensitive work
vs. idle. This gives the OS the clearest possible signal for App Nap
scheduling, complementing the blink timer work from `app_nap_blink_timer`.

Active terminal output (PTY data arriving) should hold a "user-initiated"
activity assertion to prevent napping during active shell sessions. When
all terminals are quiescent and no user input has occurred recently, the
assertion should be released so macOS can nap the process.

Supports GOAL.md's "minimal footprint" property.

## Success Criteria

- An `NSProcessInfo` activity assertion is held while any terminal tab is
  actively receiving PTY output (data arrived within the last ~2 seconds).
- The assertion is released when all terminals are quiescent and no recent
  user input has occurred.
- The assertion uses `NSActivityUserInitiated` (or similar appropriate option)
  to prevent App Nap during active terminal sessions.
- When no assertion is held and the window is not key, macOS is free to nap
  the process (verifiable via Activity Monitor's "App Nap" column).
- No impact on input latency — assertion management happens off the hot path.
- No regressions in existing tests.