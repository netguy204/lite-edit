// Chunk: docs/chunks/mini_buffer_model - MiniBuffer single-line editing model
//!
//! MiniBuffer: A reusable single-line editing model.
//!
//! `MiniBuffer` provides the full affordance set of the main editor buffer
//! (word-jump, kill-line, shift-selection, clipboard, Emacs-style Ctrl bindings)
//! while enforcing a single-line invariant. It is the shared primitive that the
//! file picker query field and the find-in-file strip will both build on.
//!
//! # Design
//!
//! MiniBuffer is a thin composition wrapper around existing primitives:
//! - [`TextBuffer`](lite_edit_buffer::TextBuffer): Provides all text editing operations
//! - [`Viewport`]: Tracks viewport state (needed by `BufferFocusTarget`)
//! - [`BufferFocusTarget`]: Handles key event → command resolution and execution
//!
//! Rather than reimplementing any editing logic, MiniBuffer:
//! 1. Owns a `TextBuffer` and `Viewport`
//! 2. Delegates all key handling to a `BufferFocusTarget` via `EditorContext`
//! 3. Filters only the events that would violate the single-line invariant:
//!    - `Key::Return` → no-op (would insert newline)
//!    - `Key::Up` / `Key::Down` → no-op (no multi-line cursor movement)
//!    - All other keys pass through unchanged
//!
//! This ensures MiniBuffer gets all affordances for free, with minimal code to maintain.

use crate::buffer_target::BufferFocusTarget;
use crate::context::EditorContext;
use crate::dirty_region::DirtyRegion;
use crate::focus::FocusTarget;
use crate::font::FontMetrics;
use crate::input::{Key, KeyEvent};
use crate::viewport::Viewport;
// Chunk: docs/chunks/styled_line_cache - Import DirtyLines for EditorContext
use lite_edit_buffer::{DirtyLines, TextBuffer};

/// A single-line text editing buffer with full editor affordances.
///
/// `MiniBuffer` provides the same editing capabilities as the main buffer
/// (character insertion, word navigation, selection, clipboard operations)
/// while enforcing a single-line invariant. Events that would break this
/// invariant (Return, Up, Down) are filtered out.
///
/// # Example
///
/// ```ignore
/// use crate::mini_buffer::MiniBuffer;
/// use crate::font::FontMetrics;
/// use crate::input::{Key, KeyEvent, Modifiers};
///
/// let metrics = FontMetrics { /* ... */ };
/// let mut mb = MiniBuffer::new(metrics);
///
/// // Type some text
/// mb.handle_key(KeyEvent::char('h'));
/// mb.handle_key(KeyEvent::char('i'));
/// assert_eq!(mb.content(), "hi");
///
/// // Return is a no-op (single-line invariant)
/// mb.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));
/// assert_eq!(mb.content(), "hi"); // No newline inserted
/// ```
pub struct MiniBuffer {
    /// The underlying text buffer
    buffer: TextBuffer,
    /// Viewport for tracking scroll state (single line, but needed for EditorContext)
    viewport: Viewport,
    /// Dirty region accumulator
    dirty_region: DirtyRegion,
    // Chunk: docs/chunks/styled_line_cache - Dirty lines for EditorContext (unused for MiniBuffer)
    /// Dirty lines accumulator (required by EditorContext but not used for cache invalidation
    /// since MiniBuffer doesn't participate in the main styled line cache)
    dirty_lines: DirtyLines,
    /// Font metrics for editor context
    font_metrics: FontMetrics,
}

impl MiniBuffer {
    /// Creates a new empty MiniBuffer.
    ///
    /// # Arguments
    ///
    /// * `font_metrics` - Font metrics used for editor context (character width, line height, etc.)
    ///
    /// # Returns
    ///
    /// A new `MiniBuffer` with an empty buffer, no selection, and cursor at position 0.
    pub fn new(font_metrics: FontMetrics) -> Self {
        let mut viewport = Viewport::new(font_metrics.line_height as f32);
        // Single-line viewport: one line visible
        // MiniBuffer is always single-line, so line_count = 1
        viewport.update_size(font_metrics.line_height as f32, 1);

        Self {
            buffer: TextBuffer::new(),
            viewport,
            dirty_region: DirtyRegion::None,
            // Chunk: docs/chunks/styled_line_cache - Initialize dirty lines
            dirty_lines: DirtyLines::None,
            font_metrics,
        }
    }

    /// Returns the current buffer content as a string.
    ///
    /// The content is always a single line containing no `\n` characters,
    /// as the single-line invariant is enforced by filtering Return key events.
    pub fn content(&self) -> String {
        self.buffer.content()
    }

    /// Returns the cursor's column position.
    ///
    /// This is the zero-indexed character offset within the single line.
    pub fn cursor_col(&self) -> usize {
        self.buffer.cursor_position().col
    }

    /// Returns the active selection as a column range, if any.
    ///
    /// # Returns
    ///
    /// - `Some((start_col, end_col))` - The selection range in byte columns, where `start_col < end_col`
    /// - `None` - No selection is active
    ///
    /// Since MiniBuffer is single-line, both positions are on line 0 and we only
    /// return the column components.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.buffer.selection_range().map(|(start, end)| {
            // Both positions are on line 0 for a single-line buffer
            (start.col, end.col)
        })
    }

    /// Returns true if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.buffer.has_selection()
    }

    /// Handles a key event, delegating to BufferFocusTarget after filtering.
    ///
    /// Events that would break the single-line invariant are filtered:
    /// - `Key::Return` → no-op (would insert newline)
    /// - `Key::Up` / `Key::Down` → no-op (no multi-line cursor movement)
    ///
    /// All other keys pass through unchanged to get the full affordance set:
    /// character insertion, backspace, word navigation, selection, clipboard, etc.
    pub fn handle_key(&mut self, event: KeyEvent) {
        // Filter events that would break single-line invariant
        match &event.key {
            Key::Return => return, // No newlines
            Key::Up | Key::Down => return, // No vertical movement
            _ => {}
        }

        // Create EditorContext and delegate to BufferFocusTarget
        // Chunk: docs/chunks/styled_line_cache - Pass dirty_lines for EditorContext
        let mut target = BufferFocusTarget::new();
        let mut ctx = EditorContext::new(
            &mut self.buffer,
            &mut self.viewport,
            &mut self.dirty_region,
            &mut self.dirty_lines,
            self.font_metrics,
            self.font_metrics.line_height as f32, // view_height (single line)
            f32::MAX,                              // view_width (no wrapping needed)
        );
        target.handle_key(event, &mut ctx);
    }

    /// Clears the buffer, resetting it to an empty state.
    ///
    /// After calling `clear()`:
    /// - `content()` returns an empty string
    /// - `cursor_col()` returns 0
    /// - `has_selection()` returns false
    pub fn clear(&mut self) {
        self.buffer = TextBuffer::new();
        self.dirty_region = DirtyRegion::None;
    }

    // Chunk: docs/chunks/minibuffer_input - Text input support for MiniBuffer
    /// Handles text input (from IME, keyboard, paste).
    ///
    /// Converts the text string to character key events and inserts them.
    /// This reuses the existing key handling logic for cursor management,
    /// selection replacement, and dirty tracking.
    pub fn handle_text_input(&mut self, text: &str) {
        for ch in text.chars() {
            self.handle_key(KeyEvent::char(ch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    /// Creates test font metrics with known values
    fn test_font_metrics() -> FontMetrics {
        FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        }
    }

    // ==================== new() tests ====================

    #[test]
    fn test_new_creates_empty_buffer() {
        let mb = MiniBuffer::new(test_font_metrics());
        assert_eq!(mb.content(), "");
    }

    #[test]
    fn test_new_cursor_at_zero() {
        let mb = MiniBuffer::new(test_font_metrics());
        assert_eq!(mb.cursor_col(), 0);
    }

    #[test]
    fn test_new_no_selection() {
        let mb = MiniBuffer::new(test_font_metrics());
        assert!(!mb.has_selection());
        assert_eq!(mb.selection_range(), None);
    }

    // ==================== Typing characters ====================

    #[test]
    fn test_typing_builds_content() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_key(KeyEvent::char('h'));
        mb.handle_key(KeyEvent::char('e'));
        mb.handle_key(KeyEvent::char('l'));
        mb.handle_key(KeyEvent::char('l'));
        mb.handle_key(KeyEvent::char('o'));
        assert_eq!(mb.content(), "hello");
    }

    #[test]
    fn test_cursor_position_after_typing() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_key(KeyEvent::char('a'));
        mb.handle_key(KeyEvent::char('b'));
        mb.handle_key(KeyEvent::char('c'));
        assert_eq!(mb.cursor_col(), 3);
    }

    // ==================== Backspace ====================

    #[test]
    fn test_backspace_removes_last_character() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_key(KeyEvent::char('a'));
        mb.handle_key(KeyEvent::char('b'));
        mb.handle_key(KeyEvent::char('c'));
        mb.handle_key(KeyEvent::new(Key::Backspace, Modifiers::default()));
        assert_eq!(mb.content(), "ab");
        assert_eq!(mb.cursor_col(), 2);
    }

    #[test]
    fn test_backspace_on_empty_is_noop() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_key(KeyEvent::new(Key::Backspace, Modifiers::default()));
        assert_eq!(mb.content(), "");
        assert_eq!(mb.cursor_col(), 0);
    }

    // ==================== Alt+Backspace (delete word backward) ====================

    #[test]
    fn test_alt_backspace_deletes_word_backward() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        // Type "hello world"
        for ch in "hello world".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        assert_eq!(mb.content(), "hello world");

        // Alt+Backspace should delete "world"
        mb.handle_key(KeyEvent::new(
            Key::Backspace,
            Modifiers {
                option: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.content(), "hello ");
    }

    // ==================== Ctrl+K (kill line) ====================

    #[test]
    fn test_ctrl_k_kills_to_end_of_line() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        // Type "hello world"
        for ch in "hello world".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        // Move cursor to position 6 (after "hello ")
        for _ in 0..5 {
            mb.handle_key(KeyEvent::new(Key::Left, Modifiers::default()));
        }
        assert_eq!(mb.cursor_col(), 6);

        // Ctrl+K should delete from cursor to end
        mb.handle_key(KeyEvent::new(
            Key::Char('k'),
            Modifiers {
                control: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.content(), "hello ");
    }

    // ==================== Option+Left/Right (word movement) ====================

    #[test]
    fn test_option_left_moves_by_word() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello world".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        assert_eq!(mb.cursor_col(), 11);

        // Option+Left should move to start of "world"
        mb.handle_key(KeyEvent::new(
            Key::Left,
            Modifiers {
                option: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.cursor_col(), 6);

        // Another Option+Left should move to start of "hello"
        mb.handle_key(KeyEvent::new(
            Key::Left,
            Modifiers {
                option: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.cursor_col(), 0);
    }

    #[test]
    fn test_option_right_moves_by_word() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello world".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        // Move to start
        mb.handle_key(KeyEvent::new(
            Key::Left,
            Modifiers {
                command: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.cursor_col(), 0);

        // Option+Right should move to end of "hello"
        mb.handle_key(KeyEvent::new(
            Key::Right,
            Modifiers {
                option: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.cursor_col(), 5);
    }

    // ==================== Shift+Right (selection) ====================

    #[test]
    fn test_shift_right_extends_selection() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        // Move to start
        mb.handle_key(KeyEvent::new(
            Key::Left,
            Modifiers {
                command: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.cursor_col(), 0);

        // Shift+Right should select one character
        mb.handle_key(KeyEvent::new(
            Key::Right,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        ));
        assert!(mb.has_selection());
        assert_eq!(mb.selection_range(), Some((0, 1)));

        // Another Shift+Right should extend selection
        mb.handle_key(KeyEvent::new(
            Key::Right,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        ));
        assert_eq!(mb.selection_range(), Some((0, 2)));
    }

    // ==================== Return is no-op ====================

    #[test]
    fn test_return_is_noop() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }

        // Return should not insert a newline
        mb.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));
        assert_eq!(mb.content(), "hello");
        assert!(!mb.content().contains('\n'));
    }

    // ==================== Up/Down are no-ops ====================

    #[test]
    fn test_up_is_noop() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        let original_col = mb.cursor_col();

        mb.handle_key(KeyEvent::new(Key::Up, Modifiers::default()));
        assert_eq!(mb.cursor_col(), original_col);
    }

    #[test]
    fn test_down_is_noop() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        let original_col = mb.cursor_col();

        mb.handle_key(KeyEvent::new(Key::Down, Modifiers::default()));
        assert_eq!(mb.cursor_col(), original_col);
    }

    // ==================== Cmd+A (select all) ====================

    #[test]
    fn test_cmd_a_selects_all() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }

        // Cmd+A should select all
        mb.handle_key(KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        ));
        assert!(mb.has_selection());
        assert_eq!(mb.selection_range(), Some((0, 5)));
    }

    // ==================== clear() ====================

    #[test]
    fn test_clear_empties_content() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        assert_eq!(mb.content(), "hello");

        mb.clear();
        assert_eq!(mb.content(), "");
    }

    #[test]
    fn test_clear_removes_selection() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        // Select all
        mb.handle_key(KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        ));
        assert!(mb.has_selection());

        mb.clear();
        assert!(!mb.has_selection());
    }

    #[test]
    fn test_clear_resets_cursor() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        for ch in "hello".chars() {
            mb.handle_key(KeyEvent::char(ch));
        }
        assert_eq!(mb.cursor_col(), 5);

        mb.clear();
        assert_eq!(mb.cursor_col(), 0);
    }

    // ==================== handle_text_input() tests ====================
    // Chunk: docs/chunks/minibuffer_input - Text input tests

    #[test]
    fn test_handle_text_input_single_char() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_text_input("a");
        assert_eq!(mb.content(), "a");
    }

    #[test]
    fn test_handle_text_input_string() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_text_input("hello");
        assert_eq!(mb.content(), "hello");
    }

    #[test]
    fn test_handle_text_input_unicode() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_text_input("日本語");
        assert_eq!(mb.content(), "日本語");
    }

    #[test]
    fn test_handle_text_input_replaces_selection() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        // Type "hello"
        mb.handle_text_input("hello");
        assert_eq!(mb.content(), "hello");

        // Select all (Cmd+A)
        mb.handle_key(KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        ));
        assert!(mb.has_selection());

        // Insert "x" via text input - should replace selection
        mb.handle_text_input("x");
        assert_eq!(mb.content(), "x");
    }

    #[test]
    fn test_handle_text_input_empty_string() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_text_input("hello");
        mb.handle_text_input("");
        // Empty input should be a no-op
        assert_eq!(mb.content(), "hello");
    }

    #[test]
    fn test_handle_text_input_cursor_position() {
        let mut mb = MiniBuffer::new(test_font_metrics());
        mb.handle_text_input("abc");
        assert_eq!(mb.cursor_col(), 3);
    }
}
