// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
//! Terminal emulator crate for lite-edit.
//!
//! This crate provides `TerminalBuffer`, a full-featured terminal emulator
//! that implements the `BufferView` trait. It wraps `alacritty_terminal` for
//! escape sequence interpretation and manages PTY I/O for process communication.
//!
//! ## Scrollback
//!
//! `TerminalBuffer` supports unlimited scrollback history with bounded memory:
//! - Recent lines stay in memory (hot scrollback)
//! - Older lines are persisted to a temp file (cold scrollback)
//! - The `BufferView::styled_line()` API is transparent — callers don't
//!   need to know where the data comes from
//!
//! This enables 10+ concurrent terminals with 100K+ line histories while
//! keeping memory usage under ~7MB per terminal.
//!
//! # Example
//!
//! ```no_run
//! use lite_edit_terminal::{TerminalBuffer, BufferView};
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

mod cold_scrollback;
mod event;
mod pty;
mod style_convert;
mod terminal_buffer;

pub use terminal_buffer::TerminalBuffer;

// Re-export BufferView and related types for convenience
pub use lite_edit_buffer::{BufferView, CursorInfo, CursorShape, DirtyLines, Position, StyledLine};
