#![allow(dead_code)]
// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
// Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue architecture
//!
//! lite-edit library interface.
//!
//! This module exposes types that are useful for other crates in the workspace,
//! particularly input types that the terminal crate needs for encoding.
//!
//! Input types are now in the `lite-edit-input` crate and re-exported here
//! for backwards compatibility.

// Re-export the input module for backwards compatibility
pub mod input;

// Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue types
/// Event types for the unified event queue architecture.
pub mod editor_event;
/// Event channel for sending/receiving editor events.
pub mod event_channel;

// Chunk: docs/chunks/row_scroller_extract - Reusable scroll arithmetic
pub mod row_scroller;

// Chunk: docs/chunks/dirty_region - Dirty region tracking
mod dirty_region;

// Chunk: docs/chunks/wrap_layout - Word wrapping layout
mod wrap_layout;

// Chunk: docs/chunks/font_metrics - Font metrics
mod font;

// Chunk: docs/chunks/fuzzy_file_matcher - File index for fuzzy file matching
pub mod file_index;

// Chunk: docs/chunks/workspace_dir_picker - Directory picker for new workspaces
mod dir_picker;

// Chunk: docs/chunks/workspace_model - Workspace model for the editor
pub mod workspace;

// Chunk: docs/chunks/viewport_scrolling - Viewport scroll state
pub mod viewport;

// Chunk: docs/chunks/tiling_tree_model - Binary pane layout tree data model
pub mod pane_layout;

// Chunk: docs/chunks/dragdrop_file_paste - Shell escaping for drag-and-drop paths
pub mod shell_escape;
