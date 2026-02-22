---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/Cargo.toml
  - crates/terminal/src/lib.rs
  - crates/terminal/src/terminal_buffer.rs
  - crates/terminal/src/style_convert.rs
  - crates/terminal/src/pty.rs
  - crates/terminal/src/event.rs
  - crates/terminal/tests/integration.rs
  - Cargo.toml
code_references:
  - ref: crates/terminal/src/lib.rs
    implements: "Module re-exports and crate documentation"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer
    implements: "Core terminal emulator struct wrapping alacritty_terminal::Term"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::new
    implements: "Terminal initialization with cols, rows, scrollback capacity"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::spawn_shell
    implements: "Shell spawning in PTY"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::spawn_command
    implements: "Arbitrary command spawning in PTY"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::poll_events
    implements: "PTY event processing, feeding bytes to terminal emulator"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::resize
    implements: "Terminal resize propagation to emulator and PTY"
  - ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::update_damage
    implements: "Damage tracking bridge from alacritty to DirtyLines"
  - ref: crates/terminal/src/terminal_buffer.rs#impl BufferView for TerminalBuffer
    implements: "BufferView trait implementation for rendering pipeline integration"
  - ref: crates/terminal/src/style_convert.rs#convert_color
    implements: "alacritty vte Color to lite-edit Color conversion"
  - ref: crates/terminal/src/style_convert.rs#cell_to_style
    implements: "Cell flags/colors to Style attribute mapping"
  - ref: crates/terminal/src/style_convert.rs#row_to_styled_line
    implements: "Row-to-StyledLine conversion with span coalescing and wide char handling"
  - ref: crates/terminal/src/pty.rs#PtyHandle
    implements: "PTY management with background reader thread"
  - ref: crates/terminal/src/pty.rs#PtyHandle::spawn
    implements: "Cross-platform PTY spawning via portable-pty"
  - ref: crates/terminal/src/event.rs#TerminalEvent
    implements: "PTY reader thread to main thread communication"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- renderer_styled_content
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Implement `TerminalBuffer` — a full-featured terminal emulator backed by `alacritty_terminal`, implementing the `BufferView` trait. This is not an agent output viewer — it's a real terminal that can run Vim, Emacs, htop, or any TUI application.

Create a new crate (e.g., `crates/terminal`) containing:

1. **`TerminalBuffer`**: Wraps `alacritty_terminal::Term<T>` and a `vte::ansi::Processor`. Implements `BufferView` by converting the terminal's cell grid to `StyledLine`s. Maps `Cell` flags/colors to `Style` attributes (bold, italic, underline variants, fg/bg colors, inverse, dim, hidden, wide char handling).

2. **PTY management**: Spawn a shell process (or arbitrary command) in a PTY. Read PTY output on a background thread and feed bytes through `Processor::advance()` into the `Term`. Handle resize (propagate `SIGWINCH` equivalent via PTY ioctl + `term.resize()`).

3. **Scrollback integration**: The `BufferView::line_count()` returns viewport + scrollback lines. `styled_line(n)` reads from scrollback for `n < scrollback_len`, from the live viewport grid for `n >= scrollback_len`. Alacritty's in-memory scrollback is configured to 2-5K lines initially.

4. **Alternate screen support**: When a TUI app activates alternate screen (`swap_alt()`), the `BufferView` seamlessly shows the alternate grid. `line_count()` returns just `screen_lines`, scrollback disappears. When the TUI exits, primary grid + scrollback reappear.

5. **Damage tracking**: `take_dirty()` uses alacritty's `TermDamage` (collect damaged line indices, drop the borrow, then return `DirtyLines`). Handle the `TermDamage::Full` case (map to `DirtyLines::FromLineToEnd(0)`).

The investigation benchmark (`docs/investigations/hierarchical_terminal_tabs/prototypes/alacritty_bench/`) proved alacritty_terminal processes at ~170 MB/s with grid reads costing 0.24% of a 60fps frame budget.

## Success Criteria

- `TerminalBuffer` implements `BufferView` and is usable as `Box<dyn BufferView>`
- Spawning a shell (e.g., `/bin/zsh`) in a `TerminalBuffer` produces visible styled output through the same rendering pipeline as `TextBuffer`
- ANSI colors render correctly: 16 named colors, 256-color, and RGB truecolor
- Bold, italic, underline, inverse, dim attributes render correctly
- Wide characters (CJK, emoji) occupy 2 cells and render without overlap or gaps
- Alternate screen works: running `vim` or `less` switches to alternate buffer, exiting restores primary
- Scrollback works: output that scrolls off the top is accessible via `styled_line()` for earlier line indices
- Terminal resize propagates correctly: resizing the view updates PTY dimensions and reflows content
- Damage tracking works: `take_dirty()` returns accurate dirty regions matching terminal output changes
- No main-thread blocking: PTY reads happen on a background thread; the main/render thread only reads the grid