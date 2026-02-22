<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk creates a new `crates/terminal` crate that implements `TerminalBuffer` — a full-featured terminal emulator backed by `alacritty_terminal` that implements the `BufferView` trait. The design follows the architecture validated in the investigation (`docs/investigations/hierarchical_terminal_tabs/`):

**Core architecture:**
1. **`TerminalBuffer`** wraps `alacritty_terminal::Term<T>` and manages PTY I/O
2. **PTY management** runs on a background thread, feeding bytes to the terminal emulator
3. **Cell-to-Style conversion** maps alacritty's `Cell` type to our `Style`/`Span`/`StyledLine` types
4. **Scrollback integration** exposes both live viewport and scrollback through `BufferView`
5. **Damage tracking** bridges alacritty's `TermDamage` to our `DirtyLines`

**Testing strategy:** Per TESTING_PHILOSOPHY.md, the terminal state machine and cell conversion are testable pure logic. PTY spawning and actual process I/O are "humble" platform code verified visually and through smoke tests. We test:
- Cell flag/color → Style attribute mapping (unit tests)
- Scrollback line indexing (unit tests)
- Damage → DirtyLines conversion (unit tests)
- `BufferView` contract compliance (unit tests with mock terminal state)
- Integration: spawn shell, verify output appears in `styled_line()` (integration test)

**Key design decisions:**
- alacritty_terminal handles all VT100/xterm escape sequence parsing (proven in investigation benchmark)
- The grid-to-StyledLine conversion is cheap (0.24% of 60fps frame budget for 40×120 grid)
- Initial scrollback configured to 2-5K lines (larger scrollback is a future chunk: `file_backed_scrollback`)
- Single-threaded main loop reads from shared state; background thread writes (channel-based communication)

## Sequence

### Step 1: Create the `crates/terminal` crate structure

Create a new crate with the basic module structure:

```
crates/terminal/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Module re-exports
│   ├── terminal_buffer.rs  # TerminalBuffer struct + BufferView impl
│   ├── style_convert.rs    # Cell → Style conversion
│   ├── pty.rs              # PTY spawning and I/O thread
│   └── event.rs            # TerminalEvent enum for channel messages
```

Add to workspace `Cargo.toml`:
```toml
members = ["crates/buffer", "crates/editor", "crates/terminal"]
```

Crate dependencies:
- `alacritty_terminal = "0.25"` (terminal emulator core)
- `portable-pty = "0.8"` (cross-platform PTY)
- `lite-edit-buffer` (for `BufferView`, `Style`, etc.)

Location: `crates/terminal/Cargo.toml`, `crates/terminal/src/lib.rs`

### Step 2: Implement Cell → Style conversion

Create `style_convert.rs` with functions to convert alacritty's types to our types:

```rust
// Cell::fg (alacritty vte::ansi::Color) → buffer_view::Color
// Cell::bg (alacritty vte::ansi::Color) → buffer_view::Color
// Cell::flags → Style attributes (bold, italic, dim, underline, strikethrough, inverse, hidden)
// Cell::underline_color() → Style::underline_color
```

Key mappings:
- `alacritty_terminal::vte::ansi::Color::Named(n)` → `Color::Named(NamedColor::*)`
- `alacritty_terminal::vte::ansi::Color::Indexed(i)` → `Color::Indexed(i)`
- `alacritty_terminal::vte::ansi::Color::Spec(rgb)` → `Color::Rgb { r, g, b }`
- `Flags::BOLD` → `Style::bold`
- `Flags::ITALIC` → `Style::italic`
- `Flags::DIM` → `Style::dim`
- `Flags::UNDERLINE` → `Style::underline = UnderlineStyle::Single`
- `Flags::DOUBLE_UNDERLINE` → `Style::underline = UnderlineStyle::Double`
- `Flags::UNDERCURL` → `Style::underline = UnderlineStyle::Curly`
- `Flags::DOTTED_UNDERLINE` → `Style::underline = UnderlineStyle::Dotted`
- `Flags::DASHED_UNDERLINE` → `Style::underline = UnderlineStyle::Dashed`
- `Flags::STRIKEOUT` → `Style::strikethrough`
- `Flags::INVERSE` → `Style::inverse`
- `Flags::HIDDEN` → `Style::hidden`

Also implement row-to-StyledLine conversion:
```rust
fn row_to_styled_line(row: &[Cell], num_cols: usize) -> StyledLine
```

This iterates cells, coalesces adjacent cells with identical styles into spans, handles WIDE_CHAR and WIDE_CHAR_SPACER flags.

**Tests:**
- `test_color_named_conversion` - all 16 ANSI colors map correctly
- `test_color_indexed_conversion` - 256-color indices preserved
- `test_color_rgb_conversion` - RGB values preserved
- `test_flags_to_style` - each flag maps to correct Style attribute
- `test_style_coalescing` - adjacent same-style cells become one span
- `test_wide_char_handling` - WIDE_CHAR emits 2-column span, WIDE_CHAR_SPACER skipped

Location: `crates/terminal/src/style_convert.rs`

### Step 3: Implement the PTY event channel

Create `event.rs` with the communication types between the PTY reader thread and the main thread:

```rust
pub enum TerminalEvent {
    /// New data from PTY stdout - bytes to feed to terminal
    PtyOutput(Vec<u8>),
    /// PTY process exited with given code
    PtyExited(i32),
    /// PTY error occurred
    PtyError(std::io::Error),
}
```

The channel will be `crossbeam_channel::unbounded()` (low latency, handles bursts).

Location: `crates/terminal/src/event.rs`

### Step 4: Implement PTY spawning and I/O thread

Create `pty.rs` with:

```rust
pub struct PtyHandle {
    /// Writer to send input to PTY stdin
    writer: Box<dyn Write + Send>,
    /// Handle to kill the process
    child: Box<dyn portable_pty::Child>,
    /// Receiver for terminal events
    event_rx: Receiver<TerminalEvent>,
    /// Handle to the reader thread (for join on drop)
    reader_thread: Option<JoinHandle<()>>,
}
```

Functions:
- `spawn_pty(cmd: &str, args: &[&str], cwd: &Path, size: PtySize) -> Result<PtyHandle>`
- Spawns the command in a new PTY
- Creates a background thread that reads from PTY stdout and sends `TerminalEvent::PtyOutput` messages
- Returns the handle with writer and event receiver

The reader thread:
```rust
loop {
    let mut buf = [0u8; 4096];
    match reader.read(&mut buf) {
        Ok(0) => { /* EOF - wait for process exit */ }
        Ok(n) => { tx.send(TerminalEvent::PtyOutput(buf[..n].to_vec())); }
        Err(e) => { tx.send(TerminalEvent::PtyError(e)); break; }
    }
}
// Wait for child exit
let status = child.wait()?;
tx.send(TerminalEvent::PtyExited(status.exit_code()));
```

Implement `PtyHandle::resize(rows, cols)` to send SIGWINCH via PTY ioctl.

**Tests:** Integration test only - spawn `echo hello`, verify output arrives, verify exit code.

Location: `crates/terminal/src/pty.rs`

### Step 5: Implement TerminalBuffer core structure

Create `terminal_buffer.rs` with:

```rust
pub struct TerminalBuffer {
    /// The alacritty terminal emulator
    term: Term<EventProxy>,
    /// VTE processor for feeding bytes
    processor: vte::ansi::Processor,
    /// PTY handle (None if no process attached)
    pty: Option<PtyHandle>,
    /// Accumulated dirty lines since last take_dirty()
    dirty: DirtyLines,
    /// Terminal size (cols, rows)
    size: (usize, usize),
}
```

Where `EventProxy` is a simple struct implementing `alacritty_terminal::event::EventListener` that captures events we care about (title changes, bell, etc.).

Implement:
- `new(cols: usize, rows: usize, scrollback: usize) -> Self`
- `spawn_shell(shell: &str, cwd: &Path) -> Result<()>`
- `spawn_command(cmd: &str, args: &[&str], cwd: &Path) -> Result<()>`
- `poll_events()` - drains PTY events, feeds bytes through processor, updates dirty
- `write_input(&mut self, data: &[u8])` - writes to PTY stdin
- `resize(&mut self, cols: usize, rows: usize)` - resizes term and PTY

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 6: Implement BufferView trait for TerminalBuffer

Add the `BufferView` implementation:

```rust
impl BufferView for TerminalBuffer {
    fn line_count(&self) -> usize {
        // scrollback_len + screen_lines
        let grid = self.term.grid();
        let history_len = grid.history_size();
        let screen_lines = grid.screen_lines();
        history_len + screen_lines
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        let grid = self.term.grid();
        let history_len = grid.history_size();

        if line < history_len {
            // Scrollback region: index from oldest to newest
            let scroll_line = history_len - 1 - line;
            let row = &grid.scrollback_line(scroll_line);
            Some(row_to_styled_line(row, self.size.0))
        } else {
            // Viewport region
            let viewport_line = line - history_len;
            if viewport_line >= grid.screen_lines() {
                return None;
            }
            let row = &grid[Line(viewport_line as i32)];
            Some(row_to_styled_line(&row[..], self.size.0))
        }
    }

    fn line_len(&self, line: usize) -> usize {
        // For terminals, line length is always the terminal width
        // (trailing spaces exist but are invisible)
        self.size.0
    }

    fn take_dirty(&mut self) -> DirtyLines {
        std::mem::take(&mut self.dirty)
    }

    fn is_editable(&self) -> bool {
        false // Terminal buffers don't accept direct text input
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        let grid = self.term.grid();
        let cursor = grid.cursor();
        let history_len = grid.history_size();

        // Cursor position in document coordinates
        let line = history_len + cursor.point.line.0 as usize;
        let col = cursor.point.column.0;

        // Map cursor shape from alacritty to our CursorShape
        let shape = match self.term.cursor_style() {
            CursorStyle::Block => CursorShape::Block,
            CursorStyle::Underline => CursorShape::Underline,
            CursorStyle::Beam => CursorShape::Beam,
            CursorStyle::Hidden => CursorShape::Hidden,
        };

        Some(CursorInfo::new(
            Position::new(line, col),
            shape,
            true, // blinking
        ))
    }
}
```

**Tests:**
- `test_line_count_empty_terminal` - new terminal has screen_lines count
- `test_line_count_with_scrollback` - after output scrolls, history_size + screen_lines
- `test_styled_line_viewport` - lines in viewport region return content
- `test_styled_line_scrollback` - lines in scrollback region return historical content
- `test_styled_line_out_of_bounds` - returns None for invalid indices
- `test_cursor_info_position` - cursor at correct document position
- `test_is_editable_false` - terminal buffers are not editable

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 7: Implement damage tracking bridge

Connect alacritty's `TermDamage` to our `DirtyLines`:

```rust
impl TerminalBuffer {
    /// Called after processing PTY output to update dirty state
    fn update_damage(&mut self) {
        let damage = self.term.damage();
        let history_len = self.term.grid().history_size();

        let new_dirty = match damage {
            TermDamage::Full => {
                // Full redraw - all visible lines dirty
                DirtyLines::FromLineToEnd(history_len)
            }
            TermDamage::Partial(iter) => {
                // Collect damaged line indices
                let mut min_line = usize::MAX;
                let mut max_line = 0;
                for damage_info in iter {
                    let doc_line = history_len + damage_info.line;
                    min_line = min_line.min(doc_line);
                    max_line = max_line.max(doc_line);
                }
                if min_line == usize::MAX {
                    DirtyLines::None
                } else if min_line == max_line {
                    DirtyLines::Single(min_line)
                } else {
                    DirtyLines::Range { from: min_line, to: max_line + 1 }
                }
            }
        };

        self.dirty.merge(new_dirty);
        self.term.reset_damage();
    }
}
```

**Tests:**
- `test_damage_full_to_dirty` - TermDamage::Full → DirtyLines::FromLineToEnd
- `test_damage_partial_single` - single damaged line → DirtyLines::Single
- `test_damage_partial_range` - multiple lines → DirtyLines::Range
- `test_damage_merge` - multiple updates merge correctly

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 8: Implement alternate screen support

Handle alternate screen buffer (`\e[?1049h` / `\e[?1049l`):

The key insight: when in alternate screen mode, `styled_line()` should:
- Return only the alternate screen content (no scrollback)
- `line_count()` returns just `screen_lines`, not `history + screen_lines`

Check `self.term.mode().contains(TermMode::ALT_SCREEN)`:

```rust
fn line_count(&self) -> usize {
    if self.term.mode().contains(TermMode::ALT_SCREEN) {
        // Alternate screen: no scrollback
        self.term.grid().screen_lines()
    } else {
        // Primary screen: scrollback + viewport
        let grid = self.term.grid();
        grid.history_size() + grid.screen_lines()
    }
}

fn styled_line(&self, line: usize) -> Option<StyledLine> {
    let grid = self.term.grid();

    if self.term.mode().contains(TermMode::ALT_SCREEN) {
        // Alternate screen: direct viewport access
        if line >= grid.screen_lines() {
            return None;
        }
        let row = &grid[Line(line as i32)];
        Some(row_to_styled_line(&row[..], self.size.0))
    } else {
        // Primary screen: handle scrollback + viewport
        // ... existing logic
    }
}
```

**Tests:**
- `test_alt_screen_line_count` - when in alt screen, only viewport lines counted
- `test_alt_screen_no_scrollback` - styled_line() returns viewport only in alt mode
- `test_alt_screen_exit_restores` - exiting alt screen restores primary + scrollback

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 9: Handle wide characters in styled_line conversion

In `row_to_styled_line`, properly handle wide characters (CJK, emoji):

```rust
fn row_to_styled_line(row: &[Cell], num_cols: usize) -> StyledLine {
    let mut spans: Vec<Span> = Vec::new();
    let mut col = 0;

    while col < num_cols {
        let cell = &row[col];

        // Skip spacer cells that follow wide characters
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            col += 1;
            continue;
        }

        let style = cell_to_style(cell);
        let text = if cell.flags.contains(Flags::WIDE_CHAR) {
            // Wide character - occupies 2 cells
            cell.c.to_string()
        } else {
            cell.c.to_string()
        };

        // Coalesce with previous span if same style
        if let Some(last) = spans.last_mut() {
            if last.style == style {
                last.text.push_str(&text);
                col += 1;
                continue;
            }
        }

        spans.push(Span::new(text, style));
        col += 1;
    }

    StyledLine::new(spans)
}
```

**Tests:**
- `test_wide_char_basic` - CJK character appears once, not duplicated
- `test_wide_char_spacer_skipped` - spacer cell doesn't create extra content
- `test_emoji_handling` - emoji renders as single character

Location: `crates/terminal/src/style_convert.rs`

### Step 10: Integration test - shell output rendering

Create an integration test that:
1. Spawns a `TerminalBuffer` with a shell
2. Sends a simple command (`echo hello`)
3. Polls for output
4. Verifies `styled_line()` returns the expected content

```rust
#[test]
fn test_shell_output_renders() {
    let mut term = TerminalBuffer::new(80, 24, 1000);
    term.spawn_shell("/bin/sh", Path::new("/tmp")).unwrap();

    // Give shell time to start and produce prompt
    std::thread::sleep(Duration::from_millis(100));
    term.poll_events();

    // Send echo command
    term.write_input(b"echo hello\n");

    // Wait for output
    std::thread::sleep(Duration::from_millis(100));
    term.poll_events();

    // Find "hello" in output
    let mut found = false;
    for line in 0..term.line_count() {
        if let Some(styled) = term.styled_line(line) {
            let text: String = styled.spans.iter().map(|s| &s.text).collect();
            if text.contains("hello") {
                found = true;
                break;
            }
        }
    }
    assert!(found, "Expected 'hello' in terminal output");
}
```

Location: `crates/terminal/tests/integration.rs`

### Step 11: Implement resize handling

Add resize support that propagates to both the terminal emulator and the PTY:

```rust
impl TerminalBuffer {
    pub fn resize(&mut self, cols: usize, rows: usize) {
        // Update stored size
        self.size = (cols, rows);

        // Create new terminal size
        let size = TermSize { cols, lines: rows };

        // Resize the terminal emulator (handles reflow)
        self.term.resize(size);

        // Resize the PTY (sends SIGWINCH to child)
        if let Some(ref mut pty) = self.pty {
            let _ = pty.resize(rows as u16, cols as u16);
        }

        // Mark everything dirty
        self.dirty = DirtyLines::FromLineToEnd(0);
    }
}
```

**Tests:**
- `test_resize_updates_size` - size stored correctly
- `test_resize_marks_dirty` - dirty set to full refresh

Location: `crates/terminal/src/terminal_buffer.rs`

### Step 12: Export public API and add documentation

Update `lib.rs` with public exports and documentation:

```rust
// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
//! Terminal emulator crate for lite-edit.
//!
//! This crate provides `TerminalBuffer`, a full-featured terminal emulator
//! that implements the `BufferView` trait. It wraps `alacritty_terminal` for
//! escape sequence interpretation and manages PTY I/O for process communication.
//!
//! # Example
//!
//! ```no_run
//! use lite_edit_terminal::TerminalBuffer;
//! use std::path::Path;
//!
//! let mut term = TerminalBuffer::new(80, 24, 5000);
//! term.spawn_shell("/bin/zsh", Path::new("/home/user")).unwrap();
//!
//! // Poll for events and render
//! term.poll_events();
//! for line in 0..term.line_count() {
//!     if let Some(styled) = term.styled_line(line) {
//!         // render styled line...
//!     }
//! }
//! ```

mod event;
mod pty;
mod style_convert;
mod terminal_buffer;

pub use terminal_buffer::TerminalBuffer;
```

Location: `crates/terminal/src/lib.rs`

## Dependencies

### Chunk dependencies (must be complete):
- `renderer_styled_content` (ACTIVE) - Provides the renderer enhancements for per-cell backgrounds, cursor shapes, and per-span foreground colors that terminal output requires

### External crate dependencies:
- `alacritty_terminal = "0.25"` - Terminal emulator core (escape sequence parsing, grid management)
- `portable-pty = "0.8"` - Cross-platform PTY spawning
- `crossbeam-channel = "0.5"` - Low-latency channel for PTY reader thread

### Internal crate dependencies:
- `lite-edit-buffer` - For `BufferView`, `Style`, `Span`, `StyledLine`, `DirtyLines`, `CursorInfo`, etc.

## Risks and Open Questions

1. **alacritty_terminal API stability**: The crate is versioned but internal APIs may shift. We depend on specific types like `Term`, `TermDamage`, `TermMode`. Pin to exact version and monitor for breaking changes.

2. **Scrollback indexing**: alacritty_terminal's grid history is a ring buffer. Need to verify the indexing direction (oldest vs newest first) and ensure our `styled_line(n)` returns lines in the order users expect (line 0 = oldest scrollback, line N = cursor).

3. **Performance of frequent poll_events()**: If the main loop polls too frequently, it may busy-wait. If too infrequently, terminal output feels laggy. Initial implementation uses channel-based polling which blocks until data available; may need tuning.

4. **PTY reader thread shutdown**: When `TerminalBuffer` is dropped, the reader thread must be cleanly shut down. Using channel closure as signal, but need to handle the case where reader is blocked in `read()`.

5. **Wide character metrics**: Wide characters occupy 2 cells in the grid but render as single glyphs. The glyph buffer in `crates/editor` needs to handle this, which is already implemented but untested with real terminal output. May surface edge cases.

6. **Cursor style sync**: alacritty_terminal tracks cursor style changes via escape sequences (`DECSCUSR`). Need to verify `cursor_style()` returns the correct shape after mode changes.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
