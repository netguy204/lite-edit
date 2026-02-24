// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
// Chunk: docs/chunks/agent_lifecycle - Agent lifecycle tracking for Composer-like workflows
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
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
//! # Example: Terminal Buffer
//!
//! ```no_run
//! use lite_edit_terminal::{TerminalBuffer, BufferView};
//! use std::path::Path;
//!
//! let mut term = TerminalBuffer::new(80, 24, 5000);
//! // spawn_shell() spawns the user's login shell (determined from passwd database)
//! term.spawn_shell(Path::new("/home/user")).unwrap();
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
mod cold_scrollback;
mod event;
// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
mod input_encoder;
mod pty;
// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
mod pty_wakeup;
mod style_convert;
mod terminal_buffer;
// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
mod terminal_target;

pub use agent::{AgentConfig, AgentHandle, AgentState, AgentStateMachine};
// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
pub use input_encoder::InputEncoder;
// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
// Chunk: docs/chunks/pty_wakeup_reentrant - WakeupSignal trait re-export
pub use pty_wakeup::{set_global_wakeup_callback, PtyWakeup};
// Re-export WakeupSignal trait for use by editor crate
pub use lite_edit_input::WakeupSignal;
// Chunk: docs/chunks/terminal_flood_starvation - Byte-budgeted VTE processing
pub use terminal_buffer::{PollResult, TerminalBuffer};
// Chunk: docs/chunks/terminal_scrollback_viewport - Terminal scroll action result
pub use terminal_target::{ScrollAction, TerminalFocusTarget};

// Re-export BufferView and related types for convenience
pub use lite_edit_buffer::{BufferView, CursorInfo, CursorShape, DirtyLines, Position, StyledLine};

// Chunk: docs/chunks/terminal_active_tab_safety - Re-export TermMode for input encoding
pub use alacritty_terminal::term::TermMode;
