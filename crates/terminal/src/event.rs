// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
//! Event types for PTY communication.
//!
//! This module defines the events that flow from the PTY reader thread
//! to the main thread via a crossbeam channel.

use std::io;

/// Events sent from the PTY reader thread to the main terminal buffer.
#[derive(Debug)]
#[allow(dead_code)]
pub enum TerminalEvent {
    /// New data from PTY stdout - bytes to feed to terminal emulator.
    PtyOutput(Vec<u8>),
    /// PTY process exited with given exit code.
    PtyExited(i32),
    /// PTY error occurred during reading.
    PtyError(io::Error),
}
