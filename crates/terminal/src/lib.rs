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

mod event;
// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
mod input_encoder;
mod pty;
mod style_convert;
mod terminal_buffer;
// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
mod terminal_target;

// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
pub use input_encoder::InputEncoder;
pub use terminal_buffer::TerminalBuffer;
pub use terminal_target::TerminalFocusTarget;

// Re-export BufferView and related types for convenience
pub use lite_edit_buffer::{BufferView, CursorInfo, CursorShape, DirtyLines, Position, StyledLine};
