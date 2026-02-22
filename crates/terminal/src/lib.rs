// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
// Chunk: docs/chunks/agent_lifecycle - Agent lifecycle tracking for Composer-like workflows
//! Terminal emulator crate for lite-edit.
//!
//! This crate provides `TerminalBuffer`, a full-featured terminal emulator
//! that implements the `BufferView` trait. It wraps `alacritty_terminal` for
//! escape sequence interpretation and manages PTY I/O for process communication.
//!
//! Additionally, this crate provides `AgentHandle`, a wrapper around `TerminalBuffer`
//! that infers agent lifecycle state (Running, NeedsInput, Stale, Exited) from PTY
//! behavior. This enables Composer-like multi-agent workflows where multiple AI
//! coding agents run in parallel and the UI shows their status.
//!
//! # Example: Terminal Buffer
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
//!
//! # Example: Agent Handle
//!
//! ```no_run
//! use lite_edit_terminal::{AgentHandle, AgentConfig, AgentState};
//! use std::path::PathBuf;
//! use std::time::Duration;
//!
//! let config = AgentConfig::new("claude")
//!     .with_cwd(PathBuf::from("/home/user/project"))
//!     .with_needs_input_timeout(Duration::from_secs(5));
//!
//! let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();
//!
//! // Poll each frame to update state
//! loop {
//!     agent.poll();
//!     match agent.state() {
//!         AgentState::Running => { /* green indicator */ }
//!         AgentState::NeedsInput { .. } => { /* yellow indicator */ }
//!         AgentState::Exited { code: 0 } => { /* success */ break; }
//!         AgentState::Exited { .. } => { /* error */ break; }
//!         _ => {}
//!     }
//! }
//! ```

mod agent;
mod event;
mod pty;
mod style_convert;
mod terminal_buffer;

pub use agent::{AgentConfig, AgentHandle, AgentState, AgentStateMachine};
pub use terminal_buffer::TerminalBuffer;

// Re-export BufferView and related types for convenience
pub use lite_edit_buffer::{BufferView, CursorInfo, CursorShape, DirtyLines, Position, StyledLine};
