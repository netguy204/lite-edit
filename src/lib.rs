// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! lite-edit-buffer: A text buffer implementation for the lite-edit editor.
//!
//! This crate provides a gap buffer-backed text buffer with cursor tracking
//! and dirty line reporting. It is designed for efficient text editing operations
//! with minimal rendering overhead.
//!
//! # Overview
//!
//! The main type is [`TextBuffer`], which provides:
//! - Character insertion and deletion at the cursor position
//! - Line-based access for efficient rendering
//! - Dirty line tracking to minimize redraws
//! - Cursor movement operations
//!
//! # Example
//!
//! ```
//! use lite_edit_buffer::{TextBuffer, DirtyLines, Position};
//!
//! let mut buffer = TextBuffer::new();
//!
//! // Insert some text
//! buffer.insert_str("Hello, world!");
//! assert_eq!(buffer.line_count(), 1);
//! assert_eq!(buffer.line_content(0), "Hello, world!");
//!
//! // Split into multiple lines
//! buffer.set_cursor(Position::new(0, 6));
//! let dirty = buffer.insert_newline();
//! assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
//! assert_eq!(buffer.line_count(), 2);
//! ```
//!
//! # Dirty Line Tracking
//!
//! Each mutation operation returns a [`DirtyLines`] value indicating which lines
//! were affected. This enables downstream rendering to minimize redraws:
//!
//! - `DirtyLines::None` - No visual change (e.g., no-op at buffer boundary)
//! - `DirtyLines::Single(line)` - Only one line changed
//! - `DirtyLines::FromLineToEnd(line)` - All lines from `line` to the end changed
//!   (used when lines are split or joined)

mod gap_buffer;
mod line_index;
mod text_buffer;
mod types;

pub use text_buffer::TextBuffer;
pub use types::{DirtyLines, Position};
