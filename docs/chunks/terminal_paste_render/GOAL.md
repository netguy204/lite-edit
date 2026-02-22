---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/terminal/tests/input_integration.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key
    implements: "Paste handler without premature dirty marking - lets PTY echo drive rendering"
  - ref: crates/terminal/tests/input_integration.rs#test_paste_content_appears_after_poll
    implements: "Integration test validating paste content appears after poll"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_alt_backspace
- terminal_background_box_drawing
- terminal_clipboard_selection
- terminal_tab_initial_render
---

# Chunk Goal

## Minor Goal

When text is pasted (Cmd+V) into the terminal tab, the pasted characters appear as blank spaces of the same length as the pasted text. The glyphs only become visible after pressing Enter, at which point the command runs successfully. Paste works correctly in editor buffers — this is a terminal-tab-only issue.

**What we know:**

- The Cmd+V handler (`editor_state.rs:1146`) correctly reads from the clipboard, encodes via `InputEncoder::encode_paste()`, writes all bytes to the PTY, marks `DirtyRegion::FullViewport`, and returns.
- A PTY wakeup path exists (`terminal_pty_wakeup` chunk): the reader thread signals the main thread via GCD dispatch when PTY output arrives, which triggers `handle_pty_wakeup` → `poll_agents()` → `render_if_dirty()`.
- This wakeup path works correctly for normal single-character typing — echoed characters appear promptly.
- Editor buffers don't have this problem because they insert text directly (no PTY roundtrip).
- The pasted text IS reaching the shell (Enter executes the command successfully), so `write_input` works.
- The blank spaces have the **same length** as the pasted text, suggesting partial rendering state.
- **Paste into TUI apps (alternate screen) works correctly.** The issue is specific to the primary screen / shell prompt case.

**Narrowed investigation:** Since TUI apps use `styled_line_alt_screen` and the primary screen uses `styled_line_hot` (with its cold/hot scrollback index mapping), the bug likely lives in the primary-screen rendering path. Possible causes:

1. **Damage tracking in primary screen mode:** After `poll_events` processes the shell's echo, `update_damage()` marks specific lines dirty. If the damage region doesn't cover the right lines (e.g., off-by-one in the cold/hot offset calculation), the renderer shows stale content (spaces) for the echoed line.
2. **`styled_line_hot` index mapping:** The cold_line_count + history_len offset arithmetic in `styled_line_hot` (`terminal_buffer.rs:607`) may return the wrong row when the grid is rapidly updated, causing the renderer to read an empty/stale row instead of the one containing echoed text.
3. **Immediate pre-echo render interference:** The paste handler marks `FullViewport` dirty and renders before the echo arrives (showing spaces). When the wakeup later fires with the echo data, the incremental damage region may not fully overlap what was rendered, leaving stale space glyphs on screen.

The chunk should diagnose which of these explains the gap and fix it. The alternate-screen path working correctly provides a useful comparison baseline.

## Success Criteria

- Pasting text via Cmd+V into a focused terminal tab displays the pasted characters immediately (no blank spaces)
- Pasted multi-line text (e.g., a shell script) renders correctly line by line
- Pasting does not break subsequent keyboard input
- Existing single-character typing behavior is unaffected
- The fix works for both short strings (a word) and longer pastes (a paragraph)