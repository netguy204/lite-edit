// Chunk: docs/chunks/syntax_highlighting - Edit event translation for tree-sitter

//! Edit event translation between buffer coordinates and tree-sitter byte offsets.
//!
//! Tree-sitter requires edits expressed as byte offsets, while the buffer uses
//! (row, col) character positions. This module provides helpers for the translation.

/// An edit event in tree-sitter format.
///
/// Contains both byte offsets and (row, col) positions needed by
/// `tree_sitter::InputEdit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditEvent {
    /// Byte offset where the edit starts
    pub start_byte: usize,
    /// Byte offset where the old content ends
    pub old_end_byte: usize,
    /// Byte offset where the new content ends
    pub new_end_byte: usize,
    /// Row of the edit start position
    pub start_row: usize,
    /// Column of the edit start position
    pub start_col: usize,
    /// Row where the old content ends
    pub old_end_row: usize,
    /// Column where the old content ends
    pub old_end_col: usize,
    /// Row where the new content ends
    pub new_end_row: usize,
    /// Column where the new content ends
    pub new_end_col: usize,
}

impl EditEvent {
    /// Converts this edit event to a tree-sitter `InputEdit`.
    pub fn to_input_edit(&self) -> tree_sitter::InputEdit {
        tree_sitter::InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte: self.new_end_byte,
            start_position: tree_sitter::Point {
                row: self.start_row,
                column: self.start_col,
            },
            old_end_position: tree_sitter::Point {
                row: self.old_end_row,
                column: self.old_end_col,
            },
            new_end_position: tree_sitter::Point {
                row: self.new_end_row,
                column: self.new_end_col,
            },
        }
    }
}

/// Calculates the byte offset for a (row, col) position in a source string.
///
/// Positions are 0-indexed. Column is in characters, not bytes.
/// If the position is past the end of the source, returns the source length.
///
/// # Example
///
/// ```
/// use lite_edit_syntax::position_to_byte_offset;
///
/// let source = "hello\nworld";
/// assert_eq!(position_to_byte_offset(source, 0, 0), 0);
/// assert_eq!(position_to_byte_offset(source, 0, 5), 5);
/// assert_eq!(position_to_byte_offset(source, 1, 0), 6); // after newline
/// assert_eq!(position_to_byte_offset(source, 1, 5), 11);
/// ```
pub fn position_to_byte_offset(source: &str, row: usize, col: usize) -> usize {
    let mut current_row = 0;

    for (idx, ch) in source.char_indices() {
        if current_row == row {
            // We're on the target row, count characters until col
            let mut char_col = 0;
            let line_start = idx;
            for (char_idx, c) in source[line_start..].char_indices() {
                if c == '\n' || char_col >= col {
                    return line_start + char_idx;
                }
                char_col += 1;
            }
            // col is past end of line/source
            return source.len();
        }

        if ch == '\n' {
            current_row += 1;
        }
    }

    // Past end of source
    source.len()
}

/// Calculates the (row, col) position for a byte offset in a source string.
///
/// Returns (row, col) where both are 0-indexed. Column is in characters, not bytes.
/// If the byte offset is past the end of the source, returns the position at the end.
///
/// # Example
///
/// ```
/// use lite_edit_syntax::byte_offset_to_position;
///
/// let source = "hello\nworld";
/// assert_eq!(byte_offset_to_position(source, 0), (0, 0));
/// assert_eq!(byte_offset_to_position(source, 5), (0, 5));
/// assert_eq!(byte_offset_to_position(source, 6), (1, 0)); // after newline
/// assert_eq!(byte_offset_to_position(source, 11), (1, 5));
/// ```
pub fn byte_offset_to_position(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;
    let mut current_byte = 0;

    for ch in source.chars() {
        if current_byte >= byte_offset {
            return (row, col);
        }

        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_byte += ch.len_utf8();
    }

    (row, col)
}

/// Creates an EditEvent for inserting text at a position.
///
/// # Arguments
///
/// * `source` - The source text before the edit
/// * `row` - Row position (0-indexed)
/// * `col` - Column position (0-indexed, in characters)
/// * `text` - The text being inserted
pub fn insert_event(source: &str, row: usize, col: usize, text: &str) -> EditEvent {
    let start_byte = position_to_byte_offset(source, row, col);

    // Calculate new end position
    let mut new_end_row = row;
    let mut new_end_col = col;
    for ch in text.chars() {
        if ch == '\n' {
            new_end_row += 1;
            new_end_col = 0;
        } else {
            new_end_col += 1;
        }
    }

    EditEvent {
        start_byte,
        old_end_byte: start_byte,
        new_end_byte: start_byte + text.len(),
        start_row: row,
        start_col: col,
        old_end_row: row,
        old_end_col: col,
        new_end_row,
        new_end_col,
    }
}

/// Creates an EditEvent for deleting text.
///
/// # Arguments
///
/// * `source` - The source text before the edit
/// * `start_row` - Start row position (0-indexed)
/// * `start_col` - Start column position (0-indexed, in characters)
/// * `end_row` - End row position (0-indexed)
/// * `end_col` - End column position (0-indexed, in characters)
pub fn delete_event(
    source: &str,
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
) -> EditEvent {
    let start_byte = position_to_byte_offset(source, start_row, start_col);
    let old_end_byte = position_to_byte_offset(source, end_row, end_col);

    EditEvent {
        start_byte,
        old_end_byte,
        new_end_byte: start_byte,
        start_row,
        start_col,
        old_end_row: end_row,
        old_end_col: end_col,
        new_end_row: start_row,
        new_end_col: start_col,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== position_to_byte_offset tests ====================

    #[test]
    fn test_position_at_start() {
        let source = "hello\nworld";
        assert_eq!(position_to_byte_offset(source, 0, 0), 0);
    }

    #[test]
    fn test_position_in_first_line() {
        let source = "hello\nworld";
        assert_eq!(position_to_byte_offset(source, 0, 3), 3);
    }

    #[test]
    fn test_position_at_end_of_first_line() {
        let source = "hello\nworld";
        assert_eq!(position_to_byte_offset(source, 0, 5), 5);
    }

    #[test]
    fn test_position_at_start_of_second_line() {
        let source = "hello\nworld";
        assert_eq!(position_to_byte_offset(source, 1, 0), 6);
    }

    #[test]
    fn test_position_in_second_line() {
        let source = "hello\nworld";
        assert_eq!(position_to_byte_offset(source, 1, 3), 9);
    }

    #[test]
    fn test_position_with_multibyte_char() {
        let source = "he\u{1F600}llo"; // emoji is 4 bytes
        assert_eq!(position_to_byte_offset(source, 0, 0), 0);
        assert_eq!(position_to_byte_offset(source, 0, 2), 2);
        assert_eq!(position_to_byte_offset(source, 0, 3), 6); // after emoji
    }

    #[test]
    fn test_position_past_end() {
        let source = "hello";
        assert_eq!(position_to_byte_offset(source, 0, 100), source.len());
        assert_eq!(position_to_byte_offset(source, 10, 0), source.len());
    }

    // ==================== byte_offset_to_position tests ====================

    #[test]
    fn test_offset_at_start() {
        let source = "hello\nworld";
        assert_eq!(byte_offset_to_position(source, 0), (0, 0));
    }

    #[test]
    fn test_offset_in_first_line() {
        let source = "hello\nworld";
        assert_eq!(byte_offset_to_position(source, 3), (0, 3));
    }

    #[test]
    fn test_offset_at_newline() {
        let source = "hello\nworld";
        assert_eq!(byte_offset_to_position(source, 5), (0, 5));
    }

    #[test]
    fn test_offset_after_newline() {
        let source = "hello\nworld";
        assert_eq!(byte_offset_to_position(source, 6), (1, 0));
    }

    #[test]
    fn test_offset_in_second_line() {
        let source = "hello\nworld";
        assert_eq!(byte_offset_to_position(source, 9), (1, 3));
    }

    #[test]
    fn test_offset_with_multibyte_char() {
        let source = "he\u{1F600}llo"; // emoji is 4 bytes
        assert_eq!(byte_offset_to_position(source, 0), (0, 0));
        assert_eq!(byte_offset_to_position(source, 2), (0, 2));
        assert_eq!(byte_offset_to_position(source, 6), (0, 3)); // after emoji
    }

    // ==================== round-trip tests ====================

    #[test]
    fn test_roundtrip_simple() {
        let source = "hello\nworld\ntest";
        for row in 0..3 {
            for col in 0..5 {
                let byte = position_to_byte_offset(source, row, col);
                let (r, c) = byte_offset_to_position(source, byte);
                assert_eq!((r, c), (row, col), "Roundtrip failed for ({}, {})", row, col);
            }
        }
    }

    // ==================== insert_event tests ====================

    #[test]
    fn test_insert_single_char() {
        let source = "hello";
        let event = insert_event(source, 0, 2, "x");

        assert_eq!(event.start_byte, 2);
        assert_eq!(event.old_end_byte, 2);
        assert_eq!(event.new_end_byte, 3);
        assert_eq!(event.start_row, 0);
        assert_eq!(event.start_col, 2);
        assert_eq!(event.old_end_row, 0);
        assert_eq!(event.old_end_col, 2);
        assert_eq!(event.new_end_row, 0);
        assert_eq!(event.new_end_col, 3);
    }

    #[test]
    fn test_insert_newline() {
        let source = "hello";
        let event = insert_event(source, 0, 2, "\n");

        assert_eq!(event.start_byte, 2);
        assert_eq!(event.new_end_byte, 3);
        assert_eq!(event.new_end_row, 1);
        assert_eq!(event.new_end_col, 0);
    }

    #[test]
    fn test_insert_multiline() {
        let source = "hello";
        let event = insert_event(source, 0, 2, "abc\ndef\n");

        assert_eq!(event.new_end_row, 2);
        assert_eq!(event.new_end_col, 0);
    }

    // ==================== delete_event tests ====================

    #[test]
    fn test_delete_single_char() {
        let source = "hello";
        let event = delete_event(source, 0, 2, 0, 3);

        assert_eq!(event.start_byte, 2);
        assert_eq!(event.old_end_byte, 3);
        assert_eq!(event.new_end_byte, 2);
        assert_eq!(event.start_row, 0);
        assert_eq!(event.start_col, 2);
        assert_eq!(event.old_end_row, 0);
        assert_eq!(event.old_end_col, 3);
        assert_eq!(event.new_end_row, 0);
        assert_eq!(event.new_end_col, 2);
    }

    #[test]
    fn test_delete_across_lines() {
        let source = "hello\nworld";
        let event = delete_event(source, 0, 3, 1, 2);

        assert_eq!(event.start_byte, 3);
        assert_eq!(event.old_end_byte, 8); // "lo\nwo" = 5 bytes
        assert_eq!(event.new_end_byte, 3);
    }

    // ==================== EditEvent::to_input_edit tests ====================

    #[test]
    fn test_to_input_edit() {
        let event = EditEvent {
            start_byte: 10,
            old_end_byte: 15,
            new_end_byte: 12,
            start_row: 1,
            start_col: 3,
            old_end_row: 1,
            old_end_col: 8,
            new_end_row: 1,
            new_end_col: 5,
        };

        let input_edit = event.to_input_edit();

        assert_eq!(input_edit.start_byte, 10);
        assert_eq!(input_edit.old_end_byte, 15);
        assert_eq!(input_edit.new_end_byte, 12);
        assert_eq!(input_edit.start_position.row, 1);
        assert_eq!(input_edit.start_position.column, 3);
        assert_eq!(input_edit.old_end_position.row, 1);
        assert_eq!(input_edit.old_end_position.column, 8);
        assert_eq!(input_edit.new_end_position.row, 1);
        assert_eq!(input_edit.new_end_position.column, 5);
    }
}
