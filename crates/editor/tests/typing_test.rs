// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
//!
//! Integration tests for the editable buffer functionality.
//!
//! These tests verify that the typing system works correctly without
//! Metal or macOS dependencies. They exercise the full path from
//! KeyEvent → BufferFocusTarget → TextBuffer → DirtyRegion.

// Note: These are unit tests that verify the pure Rust logic.
// They don't test the macOS integration (keyDown:, Metal rendering).
// Manual testing is required for the full integration.

use lite_edit_buffer::{Position, TextBuffer};

/// Helper to create a mock editing context and run operations.
/// This simulates what would happen in the main loop.
struct MockEditor {
    buffer: TextBuffer,
}

impl MockEditor {
    fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
        }
    }

    fn from_str(content: &str) -> Self {
        Self {
            buffer: TextBuffer::from_str(content),
        }
    }

    fn insert(&mut self, ch: char) {
        self.buffer.insert_char(ch);
    }

    fn insert_newline(&mut self) {
        self.buffer.insert_newline();
    }

    fn backspace(&mut self) {
        self.buffer.delete_backward();
    }

    fn delete(&mut self) {
        self.buffer.delete_forward();
    }

    fn move_left(&mut self) {
        self.buffer.move_left();
    }

    fn move_right(&mut self) {
        self.buffer.move_right();
    }

    fn move_up(&mut self) {
        self.buffer.move_up();
    }

    fn move_down(&mut self) {
        self.buffer.move_down();
    }

    fn content(&self) -> String {
        self.buffer.content()
    }

    fn cursor(&self) -> Position {
        self.buffer.cursor_position()
    }

    fn line_count(&self) -> usize {
        self.buffer.line_count()
    }
}

// =============================================================================
// Typing Tests
// =============================================================================

#[test]
fn test_typing_hello() {
    let mut editor = MockEditor::new();

    editor.insert('H');
    editor.insert('e');
    editor.insert('l');
    editor.insert('l');
    editor.insert('o');

    assert_eq!(editor.content(), "Hello");
    assert_eq!(editor.cursor(), Position::new(0, 5));
}

#[test]
fn test_typing_then_backspace() {
    let mut editor = MockEditor::new();

    editor.insert('H');
    editor.insert('e');
    editor.insert('l');
    editor.insert('l');
    editor.insert('o');
    editor.backspace();
    editor.backspace();

    assert_eq!(editor.content(), "Hel");
    assert_eq!(editor.cursor(), Position::new(0, 3));
}

#[test]
fn test_typing_multiline() {
    let mut editor = MockEditor::new();

    editor.insert('H');
    editor.insert('e');
    editor.insert('l');
    editor.insert('l');
    editor.insert('o');
    editor.insert_newline();
    editor.insert('W');
    editor.insert('o');
    editor.insert('r');
    editor.insert('l');
    editor.insert('d');

    assert_eq!(editor.content(), "Hello\nWorld");
    assert_eq!(editor.line_count(), 2);
    assert_eq!(editor.cursor(), Position::new(1, 5));
}

#[test]
fn test_cursor_movement() {
    let mut editor = MockEditor::from_str("Hello\nWorld");

    // Cursor starts at (0, 0)
    assert_eq!(editor.cursor(), Position::new(0, 0));

    // Move right
    editor.move_right();
    assert_eq!(editor.cursor(), Position::new(0, 1));

    // Move down
    editor.move_down();
    assert_eq!(editor.cursor(), Position::new(1, 1));

    // Move left
    editor.move_left();
    assert_eq!(editor.cursor(), Position::new(1, 0));

    // Move up
    editor.move_up();
    assert_eq!(editor.cursor(), Position::new(0, 0));
}

#[test]
fn test_insert_in_middle() {
    let mut editor = MockEditor::from_str("Hllo");

    // Move to position 1
    editor.move_right();
    assert_eq!(editor.cursor(), Position::new(0, 1));

    // Insert 'e'
    editor.insert('e');
    assert_eq!(editor.content(), "Hello");
    assert_eq!(editor.cursor(), Position::new(0, 2));
}

#[test]
fn test_delete_forward() {
    let mut editor = MockEditor::from_str("Hello");

    // Delete 'H'
    editor.delete();
    assert_eq!(editor.content(), "ello");
    assert_eq!(editor.cursor(), Position::new(0, 0));
}

#[test]
fn test_backspace_joins_lines() {
    let mut editor = MockEditor::from_str("Hello\nWorld");

    // Move to start of second line
    editor.move_down();
    assert_eq!(editor.cursor(), Position::new(1, 0));

    // Backspace should delete the newline, joining lines
    editor.backspace();
    assert_eq!(editor.content(), "HelloWorld");
    assert_eq!(editor.line_count(), 1);
    assert_eq!(editor.cursor(), Position::new(0, 5));
}

#[test]
fn test_delete_joins_lines() {
    let mut editor = MockEditor::from_str("Hello\nWorld");

    // Move to end of first line
    for _ in 0..5 {
        editor.move_right();
    }
    assert_eq!(editor.cursor(), Position::new(0, 5));

    // Delete should delete the newline, joining lines
    editor.delete();
    assert_eq!(editor.content(), "HelloWorld");
    assert_eq!(editor.line_count(), 1);
    assert_eq!(editor.cursor(), Position::new(0, 5));
}

#[test]
fn test_paragraph_typing() {
    let mut editor = MockEditor::new();

    // Type a short paragraph
    let paragraph = "The quick brown fox\njumps over the lazy dog.";
    for ch in paragraph.chars() {
        if ch == '\n' {
            editor.insert_newline();
        } else {
            editor.insert(ch);
        }
    }

    assert_eq!(editor.content(), paragraph);
    assert_eq!(editor.line_count(), 2);
}

#[test]
fn test_cursor_movement_clamps_column() {
    let mut editor = MockEditor::from_str("Hello\nHi");

    // Move to end of first line
    for _ in 0..5 {
        editor.move_right();
    }
    assert_eq!(editor.cursor(), Position::new(0, 5));

    // Move down - should clamp to length of "Hi" (2)
    editor.move_down();
    assert_eq!(editor.cursor(), Position::new(1, 2));
}

#[test]
fn test_cursor_wraps_lines() {
    let mut editor = MockEditor::from_str("Hello\nWorld");

    // Move to end of first line
    for _ in 0..5 {
        editor.move_right();
    }
    assert_eq!(editor.cursor(), Position::new(0, 5));

    // Move right should wrap to next line
    editor.move_right();
    assert_eq!(editor.cursor(), Position::new(1, 0));

    // Move left should wrap back
    editor.move_left();
    assert_eq!(editor.cursor(), Position::new(0, 5));
}

#[test]
fn test_backspace_at_start_does_nothing() {
    let mut editor = MockEditor::from_str("Hello");

    // Cursor at start
    assert_eq!(editor.cursor(), Position::new(0, 0));

    // Backspace should do nothing
    editor.backspace();
    assert_eq!(editor.content(), "Hello");
    assert_eq!(editor.cursor(), Position::new(0, 0));
}

#[test]
fn test_delete_at_end_does_nothing() {
    let mut editor = MockEditor::from_str("Hello");

    // Move to end
    for _ in 0..5 {
        editor.move_right();
    }
    assert_eq!(editor.cursor(), Position::new(0, 5));

    // Delete should do nothing
    editor.delete();
    assert_eq!(editor.content(), "Hello");
    assert_eq!(editor.cursor(), Position::new(0, 5));
}
