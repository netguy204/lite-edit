---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/main.rs#AppDelegate::window_did_resign_key
    implements: "Stop blink timer when window loses key status for App Nap eligibility"
  - ref: crates/editor/src/main.rs#AppDelegate::window_did_become_key
    implements: "Restart blink timer and reset cursor visibility when window gains key status"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_cursor_blink_timer
    implements: "Timer creation with 0.1s tolerance for macOS timer coalescing"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- buffer_file_watching
- highlight_injection
---

# Chunk Goal

## Minor Goal

Stop the cursor blink timer when the app is not the key window, and add timer
tolerance so macOS can coalesce wakeups. This is the single highest-impact
change for enabling App Nap: the 0.5s repeating `NSTimer` currently fires
unconditionally, preventing macOS from napping the process when it's in the
background. Supports GOAL.md's "minimal footprint" property by reducing idle
CPU and battery usage to near zero when the editor is backgrounded.

## Success Criteria

- When the window resigns key status (`windowDidResignKey:`), the cursor blink
  timer is invalidated and the `blink_timer` ivar is set to `None`.
- When the window becomes key again (`windowDidBecomeKey:`), a new blink timer
  is created and stored, and the cursor is reset to visible.
- The blink timer has `setTolerance:` set to 0.1s (allowing macOS timer
  coalescing even while the app is in the foreground).
- Existing cursor blink behavior is unchanged when the window is focused:
  blink interval, keystroke reset, focus-aware overlay/buffer distinction all
  work as before.
- No regressions in the existing cursor blink tests.