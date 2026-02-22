// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/terminal_input_encoding - Re-export from shared crate
//!
//! Input event types for keyboard, mouse, and scroll handling.
//!
//! These types abstract over macOS NSEvent details and provide a clean
//! Rust-native interface for input handling. The types are defined in
//! the `lite-edit-input` crate and re-exported here for convenience.

// Re-export all types from the shared input crate
pub use lite_edit_input::*;
