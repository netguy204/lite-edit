// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
//! TerminalBuffer - a terminal emulator implementing BufferView.
//!
//! This is the main type exported by this crate. It wraps alacritty_terminal's
//! Term struct and provides the BufferView trait implementation for rendering.

use std::path::Path;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape as VteCursorShape, Processor};

use lite_edit_buffer::{
    BufferView, CursorInfo, CursorShape, DirtyLines, Position, StyledLine,
};

use crate::event::TerminalEvent;
use crate::pty::PtyHandle;
use crate::style_convert::row_to_styled_line;

/// Event listener that captures terminal events.
///
/// Currently we don't process these events, but the alacritty_terminal
/// Term requires an event listener.
#[derive(Clone)]
struct EventProxy;

impl EventListener for EventProxy {
    fn send_event(&self, _event: Event) {
        // We could capture title changes, bell events, etc. here
        // For now, we ignore them
    }
}

/// A terminal emulator buffer implementing BufferView.
///
/// This struct wraps alacritty_terminal's Term and manages PTY I/O.
/// It converts the terminal's cell grid to StyledLines for rendering
/// through the same pipeline as text editing buffers.
pub struct TerminalBuffer {
    /// The alacritty terminal emulator.
    term: Term<EventProxy>,
    /// VTE processor for feeding bytes to the terminal.
    processor: Processor,
    /// PTY handle (None if no process attached).
    pty: Option<PtyHandle>,
    /// Accumulated dirty lines since last take_dirty().
    dirty: DirtyLines,
    /// Terminal size (cols, rows).
    size: (usize, usize),
    /// Scrollback capacity (reserved for future configuration).
    #[allow(dead_code)]
    scrollback: usize,
}

impl TerminalBuffer {
    /// Creates a new terminal buffer with the given dimensions.
    ///
    /// # Arguments
    ///
    /// * `cols` - Number of columns (characters per line)
    /// * `rows` - Number of rows (lines in viewport)
    /// * `scrollback` - Number of scrollback lines to keep
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self {
        let size = TermSize::new(cols, rows);
        let config = Config::default();

        let term = Term::new(config, &size, EventProxy);
        let processor = Processor::new();

        Self {
            term,
            processor,
            pty: None,
            dirty: DirtyLines::FromLineToEnd(0), // Initial state: everything dirty
            size: (cols, rows),
            scrollback,
        }
    }

    /// Spawns a shell process in this terminal.
    ///
    /// # Arguments
    ///
    /// * `shell` - Path to the shell (e.g., "/bin/zsh")
    /// * `cwd` - Working directory for the shell
    pub fn spawn_shell(&mut self, shell: &str, cwd: &Path) -> std::io::Result<()> {
        self.spawn_command(shell, &[], cwd)
    }

    /// Spawns a command in this terminal.
    ///
    /// # Arguments
    ///
    /// * `cmd` - Command to run
    /// * `args` - Arguments for the command
    /// * `cwd` - Working directory
    pub fn spawn_command(
        &mut self,
        cmd: &str,
        args: &[&str],
        cwd: &Path,
    ) -> std::io::Result<()> {
        let (cols, rows) = self.size;
        let handle = PtyHandle::spawn(cmd, args, cwd, rows as u16, cols as u16)?;
        self.pty = Some(handle);
        Ok(())
    }

    /// Polls for and processes PTY events.
    ///
    /// This should be called regularly (e.g., each frame) to process
    /// PTY output and update the terminal state.
    ///
    /// Returns true if any events were processed.
    pub fn poll_events(&mut self) -> bool {
        let Some(ref pty) = self.pty else {
            return false;
        };

        let mut processed_any = false;

        // Drain all available events
        while let Some(event) = pty.try_recv() {
            match event {
                TerminalEvent::PtyOutput(data) => {
                    // Feed bytes to the terminal emulator
                    self.processor.advance(&mut self.term, &data);
                    processed_any = true;
                }
                TerminalEvent::PtyExited(_code) => {
                    // Process could handle this differently
                    // For now, we just note that it happened
                    processed_any = true;
                }
                TerminalEvent::PtyError(_) => {
                    // Handle error - could log or propagate
                    processed_any = true;
                }
            }
        }

        if processed_any {
            // Update dirty tracking based on terminal damage
            self.update_damage();
        }

        processed_any
    }

    /// Writes input data to the PTY stdin.
    pub fn write_input(&mut self, data: &[u8]) -> std::io::Result<()> {
        if let Some(ref mut pty) = self.pty {
            pty.write(data)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "No PTY attached",
            ))
        }
    }

    /// Resizes the terminal.
    ///
    /// This updates both the terminal emulator and the PTY (if attached).
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.size = (cols, rows);

        // Create new terminal size
        let size = TermSize::new(cols, rows);

        // Resize the terminal emulator
        self.term.resize(size);

        // Resize the PTY
        if let Some(ref pty) = self.pty {
            let _ = pty.resize(rows as u16, cols as u16);
        }

        // Mark everything dirty
        self.dirty = DirtyLines::FromLineToEnd(0);
    }

    /// Returns the terminal size as (cols, rows).
    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    /// Returns true if in alternate screen mode.
    pub fn is_alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    // Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
    /// Returns the current terminal mode flags.
    ///
    /// This is used by the input encoder to determine how to encode
    /// keys and mouse events (e.g., APP_CURSOR mode, SGR_MOUSE mode).
    pub fn term_mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Returns the number of lines in scrollback.
    fn history_size(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Returns the number of screen lines (viewport height).
    fn screen_lines(&self) -> usize {
        self.term.grid().screen_lines()
    }

    /// Updates dirty state based on terminal damage.
    fn update_damage(&mut self) {
        let history_len = self.history_size();

        // For now, we'll use a simplified damage tracking approach.
        // The terminal could have changed anywhere, so mark from the
        // start of the viewport as dirty.
        //
        // A more sophisticated implementation would use term.damage()
        // to track exactly which lines changed, but that requires
        // careful handling of the damage borrow.
        let new_dirty = DirtyLines::FromLineToEnd(history_len);
        self.dirty.merge(new_dirty);

        // Reset the terminal's internal damage tracking
        self.term.reset_damage();
    }

    /// Checks if the PTY process has exited.
    ///
    /// Returns `Some(exit_code)` if exited, `None` if still running.
    pub fn try_wait(&mut self) -> Option<i32> {
        self.pty.as_mut()?.try_wait()
    }
}

impl BufferView for TerminalBuffer {
    fn line_count(&self) -> usize {
        if self.is_alt_screen() {
            // Alternate screen: no scrollback
            self.screen_lines()
        } else {
            // Primary screen: scrollback + viewport
            self.history_size() + self.screen_lines()
        }
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        let grid = self.term.grid();
        let cols = self.size.0;

        if self.is_alt_screen() {
            // Alternate screen: direct viewport access
            let screen_lines = grid.screen_lines();
            if line >= screen_lines {
                return None;
            }
            let row = &grid[Line(line as i32)];
            // Iterate over columns to access cells
            let cells: Vec<_> = (0..cols).map(|col| &row[alacritty_terminal::index::Column(col)]).collect();
            Some(row_to_styled_line(cells.iter().copied(), cols))
        } else {
            // Primary screen: handle scrollback + viewport
            let history_len = grid.history_size();
            let screen_lines = grid.screen_lines();

            if line < history_len {
                // Scrollback region: index from oldest to newest
                // Line 0 = oldest scrollback, line (history_len - 1) = newest scrollback
                let scroll_idx = history_len - 1 - line;
                // Use negative line index to access scrollback
                let row = &grid[Line(-(scroll_idx as i32) - 1)];
                let cells: Vec<_> = (0..cols).map(|col| &row[alacritty_terminal::index::Column(col)]).collect();
                Some(row_to_styled_line(cells.iter().copied(), cols))
            } else {
                // Viewport region
                let viewport_line = line - history_len;
                if viewport_line >= screen_lines {
                    return None;
                }
                let row = &grid[Line(viewport_line as i32)];
                let cells: Vec<_> = (0..cols).map(|col| &row[alacritty_terminal::index::Column(col)]).collect();
                Some(row_to_styled_line(cells.iter().copied(), cols))
            }
        }
    }

    fn line_len(&self, _line: usize) -> usize {
        // Terminal lines are always the terminal width
        self.size.0
    }

    fn take_dirty(&mut self) -> DirtyLines {
        std::mem::take(&mut self.dirty)
    }

    fn is_editable(&self) -> bool {
        // Terminal buffers don't accept direct text input
        // (input goes to PTY stdin instead)
        false
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        let grid = self.term.grid();
        let cursor_point = grid.cursor.point;
        let history_len = self.history_size();

        // In alt screen, history_len is effectively 0 for cursor positioning
        let doc_line = if self.is_alt_screen() {
            cursor_point.line.0 as usize
        } else {
            history_len + cursor_point.line.0 as usize
        };
        let col = cursor_point.column.0;

        // Map cursor shape from alacritty to our CursorShape
        let cursor_style = self.term.cursor_style();
        let shape = match cursor_style.shape {
            VteCursorShape::Block | VteCursorShape::HollowBlock => CursorShape::Block,
            VteCursorShape::Underline => CursorShape::Underline,
            VteCursorShape::Beam => CursorShape::Beam,
            VteCursorShape::Hidden => CursorShape::Hidden,
        };

        // Check if cursor should blink
        let blinking = cursor_style.blinking;

        Some(CursorInfo::new(
            Position::new(doc_line, col),
            shape,
            blinking,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_terminal() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert_eq!(term.size(), (80, 24));
    }

    #[test]
    fn test_line_count_empty() {
        let term = TerminalBuffer::new(80, 24, 1000);
        // New terminal has screen_lines count (no scrollback yet)
        assert_eq!(term.line_count(), 24);
    }

    #[test]
    fn test_styled_line_empty() {
        let term = TerminalBuffer::new(80, 24, 1000);
        // First line should exist and be empty/spaces
        let line = term.styled_line(0);
        assert!(line.is_some());
    }

    #[test]
    fn test_styled_line_out_of_bounds() {
        let term = TerminalBuffer::new(80, 24, 1000);
        // Line 100 should be out of bounds
        let line = term.styled_line(100);
        assert!(line.is_none());
    }

    #[test]
    fn test_is_editable() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert!(!term.is_editable());
    }

    #[test]
    fn test_cursor_info() {
        let term = TerminalBuffer::new(80, 24, 1000);
        let cursor = term.cursor_info();
        assert!(cursor.is_some());
        let cursor = cursor.unwrap();
        // Initial cursor should be at (0, 0)
        assert_eq!(cursor.position.line, 0);
        assert_eq!(cursor.position.col, 0);
    }

    #[test]
    fn test_resize() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        term.resize(120, 40);
        assert_eq!(term.size(), (120, 40));
        assert_eq!(term.line_count(), 40);
    }

    #[test]
    fn test_take_dirty() {
        let mut term = TerminalBuffer::new(80, 24, 1000);

        // Initial state should be dirty
        let dirty = term.take_dirty();
        assert!(!dirty.is_none());

        // After taking, should be none
        let dirty2 = term.take_dirty();
        assert!(dirty2.is_none());
    }
}
