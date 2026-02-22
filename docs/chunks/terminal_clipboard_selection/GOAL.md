---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/terminal_buffer.rs
- crates/terminal/src/terminal_target.rs
- crates/terminal/tests/selection_integration.rs
code_references:
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::selection_anchor
    implements: "Selection anchor state getter"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::set_selection_anchor
    implements: "Set selection anchor on mouse down"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::selection_head
    implements: "Selection head state getter"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::set_selection_head
    implements: "Extend selection on drag with dirty tracking"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::clear_selection
    implements: "Clear selection and mark dirty"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::selected_text
    implements: "Extract selected text from terminal grid as string"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::selection_range
    implements: "BufferView selection_range for rendering"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::poll_events
    implements: "Clear selection when new PTY output arrives"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::handle_mouse
    implements: "Click-and-drag selection and double-click word selection"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::select_word_at
    implements: "Word boundary detection for double-click selection"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::handle_key
    implements: "Cmd+C/Cmd+V passthrough to editor"
  - ref: crates/terminal/src/terminal_target.rs#TerminalFocusTarget::write_paste
    implements: "Paste with bracketed paste mode support"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_emulator
- clipboard_operations
- mouse_drag_selection
- word_double_click_select
created_after:
- scroll_bottom_deadzone_v3
- terminal_pty_wakeup
- terminal_styling_fidelity
---

# Terminal Clipboard Selection: Copy/Paste with Mouse and Keyboard

## Minor Goal

Enable copy/paste in the terminal tab using mouse selection and standard macOS keyboard shortcuts. Users need to be able to select text in terminal output by clicking and dragging (or double-clicking to select a word), then copy with Cmd+C, and paste with Cmd+V. Terminal history (scrollback and already-rendered output) must not be editable — selection is read-only over the rendered terminal grid.

This is a fundamental usability feature: without it, users cannot extract text from terminal output or paste commands into the terminal. The text editor already has mouse drag selection (`mouse_drag_selection`), word double-click selection (`word_double_click_select`), and clipboard operations (`clipboard_operations`). This chunk brings equivalent capabilities to the terminal, adapted for the terminal's read-only grid model.

## Success Criteria

- **Click-and-drag selects text in terminal output**: When the user clicks and drags over terminal content (both live viewport and scrollback), the dragged region is visually highlighted as a selection. Selection operates over the terminal's character grid — it selects rendered cell content, not raw escape sequences. Selection coordinates are in terminal grid positions (column, row), not buffer byte offsets.

- **Double-click selects a word**: Double-clicking on a word in the terminal grid selects the entire word (using word boundary detection on the terminal cell content). This mirrors the existing `word_double_click_select` behavior adapted for the terminal's cell grid rather than a `TextBuffer`.

- **Cmd+C copies selection to system clipboard**: When terminal text is selected and the user presses Cmd+C, the selected text is copied to the macOS system clipboard via the existing `clipboard::copy_to_clipboard()`. The selection should be converted from grid cells to a string, joining rows with newlines. After copying, the selection may optionally be cleared (standard terminal emulator behavior).

- **Cmd+C without selection is a no-op**: When no text is selected in the terminal and the user presses Cmd+C, nothing happens. The interrupt signal is sent by Ctrl+C (which is already handled by `TerminalFocusTarget::handle_key()` as a normal key event encoded to `\x03`). Cmd+C is exclusively for clipboard copy.

- **Cmd+V pastes into terminal**: When the user presses Cmd+V, the system clipboard content is read via `clipboard::paste_from_clipboard()` and written to the terminal's PTY using `TerminalFocusTarget::write_paste()`, which already handles bracketed paste mode. The pasted text goes to the running process as input — it does not modify the terminal grid directly.

- **History is not editable**: Mouse selection and clipboard operations are strictly read-only over the terminal grid. Clicking or dragging does not move a cursor or modify cell content. The only way to "write" to the terminal is via Cmd+V paste, which sends input to the PTY (the running process decides what to do with it). There is no insert cursor in the terminal — the terminal's cursor position is controlled entirely by the running process.

- **Selection state lives on TerminalBuffer or TerminalFocusTarget**: A selection model (anchor + head grid coordinates) is maintained for the terminal. This is separate from the text editor's `TextBuffer` selection model. The selection is purely visual — it highlights cells in the rendered output for copy purposes.

- **Selection renders with highlight**: Selected cells in the terminal are rendered with a visible highlight (e.g., inverted colors or a selection background color), consistent with how the text editor renders selections. The renderer must handle terminal selection highlighting through the existing `BufferView` / styled line pipeline.

- **Selection clears on terminal output**: When new output arrives from the PTY (the terminal content changes), any active selection should be cleared to avoid stale/misaligned highlights. This is standard terminal emulator behavior.

- **Mouse events not consumed when TUI app requests mouse**: When the terminal is running a TUI app that has enabled mouse reporting (e.g., htop, vim with mouse mode), mouse events should be forwarded to the app via the PTY rather than interpreted as selection. The existing `TerminalFocusTarget::handle_mouse()` already checks for active mouse modes — selection should only engage when no mouse mode is active.

- **Unit tests**:
  - Click-and-drag over terminal grid produces correct selection range
  - Double-click selects the word at the clicked position
  - Cmd+C with selection copies the correct text to the mock clipboard
  - Cmd+C without selection is a no-op (does not write to clipboard, does not send interrupt)
  - Cmd+V reads from clipboard and calls `write_paste()`
  - Selection coordinates correctly map between pixel positions and grid cells
  - Selection is cleared when new PTY output arrives