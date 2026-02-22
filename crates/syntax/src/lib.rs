// Chunk: docs/chunks/syntax_highlighting - Tree-sitter syntax highlighting

//! lite-edit-syntax: Tree-sitter-based syntax highlighting for lite-edit.
//!
//! This crate provides incremental syntax highlighting using tree-sitter parsers.
//! It is designed to maintain the <8ms P99 keypress-to-glyph latency by using:
//!
//! - **Incremental parsing**: ~120µs per single-character edit
//! - **Viewport-scoped highlighting**: ~170µs for a 60-line viewport
//!
//! # Overview
//!
//! The main types are:
//!
//! - [`SyntaxHighlighter`]: Owns a tree-sitter `Parser` and `Tree`, provides
//!   `edit()` for incremental updates and `highlight_line()` for styled output.
//!
//! - [`SyntaxTheme`]: Maps tree-sitter capture names to Catppuccin Mocha styles.
//!
//! - [`LanguageRegistry`]: Maps file extensions to language configurations.
//!
//! # Example
//!
//! ```ignore
//! use lite_edit_syntax::{SyntaxHighlighter, SyntaxTheme, LanguageRegistry};
//!
//! let registry = LanguageRegistry::new();
//! let theme = SyntaxTheme::catppuccin_mocha();
//!
//! if let Some(config) = registry.config_for_extension("rs") {
//!     let source = "fn main() { println!(\"Hello\"); }";
//!     let highlighter = SyntaxHighlighter::new(config, source, theme);
//!     let styled_line = highlighter.highlight_line(0, source);
//!     // styled_line.spans contains colored spans for keywords, strings, etc.
//! }
//! ```

mod edit;
mod highlighter;
mod registry;
mod theme;

pub use edit::{byte_offset_to_position, delete_event, insert_event, position_to_byte_offset, EditEvent};
pub use highlighter::SyntaxHighlighter;
pub use registry::{LanguageConfig, LanguageRegistry};
pub use theme::SyntaxTheme;
