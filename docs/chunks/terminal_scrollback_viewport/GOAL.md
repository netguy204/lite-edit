---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/input_encoder.rs
  - crates/terminal/src/terminal_target.rs
  - crates/editor/src/editor_state.rs
  - crates/terminal/tests/scroll_integration.rs
code_references:
  - ref: crates/terminal/src/input_encoder.rs#InputEncoder::encode_scroll
    implements: "Scroll wheel encoding for alternate screen passthrough (button 64/65 sequences)"
  - ref: crates/terminal/src/terminal_target.rs#ScrollAction
    implements: "Scroll action result enum distinguishing primary/alternate screen handling"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::handle_scroll
    implements: "Terminal scroll event routing - alt screen sends to PTY, primary delegates to viewport"
  - ref: crates/editor/src/viewport.rs#Viewport::is_at_bottom
    implements: "At-bottom detection for auto-follow behavior"
  - ref: crates/editor/src/viewport.rs#Viewport::scroll_to_bottom
    implements: "Snap-to-bottom helper for keypresses and mode transitions"
  - ref: crates/editor/src/workspace.rs#Tab::terminal_and_viewport_mut
    implements: "Joint access to terminal buffer and viewport for scroll handling"
  - ref: crates/editor/src/workspace.rs#Editor::poll_standalone_terminals
    implements: "Auto-follow on new output and alt-to-primary mode transition reset"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: uses
friction_entries: []
bug_type: null
depends_on: null
created_after:
- scroll_bottom_deadzone
- terminal_tab_spawn
- workspace_switching
- cursor_blink_focus
- word_triclass_boundaries
---

# Chunk Goal

## Minor Goal

Wire terminal tabs into the existing `Viewport` scroll infrastructure so users can scroll through terminal scrollback history with the trackpad/mouse wheel.

Today every terminal `Tab` already owns a `Viewport`, and the renderer already uses `viewport.visible_range()` + `BufferView::styled_line()` generically. But `TerminalFocusTarget::handle_scroll` is a no-op stub — scroll events are silently dropped. This chunk connects the plumbing.

The key behavioral distinction is between **primary screen** (shell, build output — scrollback available) and **alternate screen** (vim, htop, less — application owns the viewport). These two modes require different scroll handling:

- **Primary screen**: Scroll events adjust the tab's `Viewport` scroll offset against `TerminalBuffer::line_count()` (cold + hot + screen). New output auto-follows when the user is at the bottom; if the user has scrolled up into history, new output does *not* yank the viewport. Any keypress snaps back to the live bottom.
- **Alternate screen**: Scroll events are encoded as mouse wheel escape sequences and sent to the PTY (the application handles scrolling internally). The `Viewport` stays pinned at offset 0 since `line_count()` equals the screen size with no scrollback.

## Success Criteria

1. **Primary screen scrollback**: Trackpad scroll up in a terminal tab reveals scrollback history (cold and hot). Scroll down returns toward live output. Scrolling respects `Viewport` clamping (can't scroll past oldest line or past bottom).

2. **Auto-follow on new output**: When the viewport is at the bottom (within one screen of the latest line), new PTY output automatically advances the scroll position to keep the latest output visible.

3. **Scroll-away holds position**: When the user has scrolled up into history (more than one screen from bottom), new PTY output does *not* change the scroll position. The user stays where they scrolled to.

4. **Keypress snaps to bottom**: Any keypress that sends data to the PTY (printable characters, Enter, Ctrl+C, etc.) snaps the viewport back to the live bottom before sending the input.

5. **Alternate screen passthrough**: When `TerminalBuffer::is_alt_screen()` is true, scroll events are encoded as mouse wheel sequences via `InputEncoder` and written to the PTY. The `Viewport` offset remains 0. Applications like vim, htop, and less receive the scroll events and handle them internally.

6. **Mode transition reset**: When the terminal transitions from alternate screen back to primary screen, the viewport snaps to the bottom of the primary scrollback (live output).

