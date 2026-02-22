// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
//!
//! Terminal focus target implementation.
//!
//! This module provides `TerminalFocusTarget`, which handles keyboard and mouse
//! input for terminal tabs. It encodes input events into terminal escape sequences
//! and writes them to the PTY stdin.
//!
//! Unlike the text buffer's focus target, the terminal target doesn't mutate a
//! text buffer — it sends bytes to a subprocess via the PTY.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::term::TermMode;

use lite_edit_input::{Key, KeyEvent, MouseEvent, ScrollDelta};

use crate::input_encoder::InputEncoder;
use crate::terminal_buffer::TerminalBuffer;

/// Focus target for terminal tabs.
///
/// When a terminal tab is focused, this target receives keyboard and mouse
/// events, encodes them into terminal escape sequences, and writes them to
/// the PTY stdin.
///
/// # Ownership
///
/// The terminal buffer is shared via `Rc<RefCell<>>` because the buffer may
/// also be accessed by the rendering system and event polling.
pub struct TerminalFocusTarget {
    /// Reference to the terminal buffer for mode queries and writing.
    terminal: Rc<RefCell<TerminalBuffer>>,
    /// Font metrics for converting pixel positions to cell coordinates.
    /// (cell_width, cell_height)
    cell_size: (f32, f32),
}

impl TerminalFocusTarget {
    /// Creates a new terminal focus target.
    ///
    /// # Arguments
    ///
    /// * `terminal` - Shared reference to the terminal buffer
    /// * `cell_width` - Width of a terminal cell in pixels
    /// * `cell_height` - Height of a terminal cell in pixels
    pub fn new(terminal: Rc<RefCell<TerminalBuffer>>, cell_width: f32, cell_height: f32) -> Self {
        Self {
            terminal,
            cell_size: (cell_width, cell_height),
        }
    }

    /// Handles a keyboard event.
    ///
    /// Returns `true` if the event was handled.
    pub fn handle_key(&mut self, event: KeyEvent) -> bool {
        // Check for special Cmd+key combinations that should be handled by the editor
        // rather than sent to the terminal
        if event.modifiers.command {
            match event.key {
                // Cmd+V is paste - handle specially with bracketed paste
                Key::Char('v') | Key::Char('V') => {
                    // Paste would be handled by clipboard integration
                    // For now, return false to let the editor handle it
                    // The actual paste text would be sent via write_paste()
                    return false;
                }
                // Cmd+C with no selection should send interrupt (Ctrl+C behavior)
                // Cmd+C with selection should copy to clipboard (handled by editor)
                Key::Char('c') | Key::Char('C') => {
                    // Let the editor decide based on selection state
                    return false;
                }
                // Other Cmd+key combinations are handled by the editor
                _ => return false,
            }
        }

        let modes = self.terminal.borrow().term_mode();
        let bytes = InputEncoder::encode_key(&event, modes);

        if bytes.is_empty() {
            return false;
        }

        match self.terminal.borrow_mut().write_input(&bytes) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Handles a scroll event.
    ///
    /// When scrolled up in the terminal, we show scrollback history.
    /// When the user types, we should snap back to the live terminal.
    /// For now, scroll events don't go to the PTY — they control the viewport.
    pub fn handle_scroll(&mut self, _delta: ScrollDelta) {
        // Terminal scrollback viewport handling would go here.
        // This is separate from the PTY — scrolling doesn't send data to the shell.
        //
        // In the future, we might implement:
        // 1. Scroll up: show older scrollback
        // 2. Scroll down: show newer scrollback / snap to live terminal
        // 3. If in alternate screen mode (e.g., vim), send scroll events as mouse wheel
        //
        // For MVP, scrolling is a no-op.
    }

    /// Handles a mouse event.
    ///
    /// Mouse events are only sent to the PTY if the terminal has enabled
    /// mouse reporting mode (e.g., TUI apps like htop, less with mouse mode).
    pub fn handle_mouse(&mut self, event: MouseEvent, view_origin: (f32, f32)) -> bool {
        let modes = self.terminal.borrow().term_mode();

        // Check if any mouse mode is active
        if !modes.intersects(TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) {
            return false;
        }

        // Convert pixel position to cell coordinates
        let (col, row) = self.pixel_to_cell(event.position, view_origin);

        let bytes = InputEncoder::encode_mouse(&event, col, row, modes);

        if bytes.is_empty() {
            return false;
        }

        match self.terminal.borrow_mut().write_input(&bytes) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Writes pasted text to the terminal, respecting bracketed paste mode.
    ///
    /// This should be called by the editor when the user pastes (Cmd+V) into
    /// a focused terminal tab.
    pub fn write_paste(&mut self, text: &str) -> bool {
        let modes = self.terminal.borrow().term_mode();
        let bytes = InputEncoder::encode_paste(text, modes);

        match self.terminal.borrow_mut().write_input(&bytes) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Converts a pixel position to terminal cell coordinates.
    ///
    /// # Arguments
    ///
    /// * `pixel_pos` - Position in pixels (x, y) from top-left of view
    /// * `view_origin` - Origin of the terminal view in the overall window
    ///
    /// # Returns
    ///
    /// (column, row) in terminal cell coordinates (0-indexed)
    fn pixel_to_cell(&self, pixel_pos: (f64, f64), view_origin: (f32, f32)) -> (usize, usize) {
        let (cell_width, cell_height) = self.cell_size;

        // Adjust for view origin
        let x = (pixel_pos.0 as f32 - view_origin.0).max(0.0);
        let y = (pixel_pos.1 as f32 - view_origin.1).max(0.0);

        let col = (x / cell_width) as usize;
        let row = (y / cell_height) as usize;

        (col, row)
    }

    /// Updates the cell size (e.g., after font change or resize).
    pub fn set_cell_size(&mut self, width: f32, height: f32) {
        self.cell_size = (width, height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lite_edit_input::{Modifiers, MouseEventKind};

    fn create_test_terminal() -> Rc<RefCell<TerminalBuffer>> {
        Rc::new(RefCell::new(TerminalBuffer::new(80, 24, 1000)))
    }

    #[test]
    fn test_new_terminal_target() {
        let terminal = create_test_terminal();
        let target = TerminalFocusTarget::new(terminal, 8.0, 16.0);
        assert_eq!(target.cell_size, (8.0, 16.0));
    }

    #[test]
    fn test_pixel_to_cell() {
        let terminal = create_test_terminal();
        let target = TerminalFocusTarget::new(terminal, 10.0, 20.0);

        // Position (25, 45) with cell size (10, 20) should be column 2, row 2
        let (col, row) = target.pixel_to_cell((25.0, 45.0), (0.0, 0.0));
        assert_eq!(col, 2);
        assert_eq!(row, 2);
    }

    #[test]
    fn test_pixel_to_cell_with_offset() {
        let terminal = create_test_terminal();
        let target = TerminalFocusTarget::new(terminal, 10.0, 20.0);

        // Position (125, 145) with origin (100, 100) should be column 2, row 2
        let (col, row) = target.pixel_to_cell((125.0, 145.0), (100.0, 100.0));
        assert_eq!(col, 2);
        assert_eq!(row, 2);
    }

    #[test]
    fn test_handle_key_without_pty() {
        let terminal = create_test_terminal();
        let mut target = TerminalFocusTarget::new(terminal, 8.0, 16.0);

        // Without a PTY attached, write_input will fail
        let event = KeyEvent::char('a');
        let result = target.handle_key(event);
        // Should return false because no PTY is attached
        assert!(!result);
    }

    #[test]
    fn test_handle_mouse_no_mode() {
        let terminal = create_test_terminal();
        let mut target = TerminalFocusTarget::new(terminal, 8.0, 16.0);

        // Without mouse mode active, mouse events should not be handled
        let event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (100.0, 200.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        let result = target.handle_mouse(event, (0.0, 0.0));
        assert!(!result);
    }

    #[test]
    fn test_cmd_v_returns_false() {
        let terminal = create_test_terminal();
        let mut target = TerminalFocusTarget::new(terminal, 8.0, 16.0);

        // Cmd+V should return false to let the editor handle paste
        let event = KeyEvent {
            key: Key::Char('v'),
            modifiers: Modifiers {
                command: true,
                ..Default::default()
            },
        };
        let result = target.handle_key(event);
        assert!(!result);
    }

    #[test]
    fn test_write_paste() {
        let terminal = create_test_terminal();
        let mut target = TerminalFocusTarget::new(terminal, 8.0, 16.0);

        // Without a PTY attached, write_paste will fail
        let result = target.write_paste("hello");
        assert!(!result);
    }
}
