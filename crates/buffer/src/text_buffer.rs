// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! TextBuffer is the main public API for text editing operations.
//!
//! It combines a gap buffer (for efficient character storage) with a line index
//! (for O(1) line access) and tracks cursor position as (line, column).
//!
//! Each mutation operation returns `DirtyLines` indicating which lines changed,
//! enabling downstream rendering to minimize redraws.

use crate::gap_buffer::GapBuffer;
use crate::line_index::LineIndex;
use crate::types::{DirtyLines, Position};

/// A text buffer with cursor tracking and dirty line reporting.
///
/// The buffer maintains:
/// - Content storage via a gap buffer
/// - Line boundary tracking for efficient line-based access
/// - Cursor position as (line, column)
/// - Selection anchor for text selection (anchor-cursor model)
///
/// All mutation operations return `DirtyLines` to enable efficient rendering.
// Chunk: docs/chunks/text_selection_model - Selection anchor and range API
#[derive(Debug)]
pub struct TextBuffer {
    buffer: GapBuffer,
    line_index: LineIndex,
    cursor: Position,
    /// Selection anchor position. When `Some`, the selection spans from anchor to cursor.
    /// The anchor may come before or after the cursor (both directions are valid).
    selection_anchor: Option<Position>,
    /// Mutation counter for sampling debug assertions (debug builds only).
    #[cfg(debug_assertions)]
    debug_mutation_count: u64,
}

impl TextBuffer {
    /// Creates a new empty text buffer.
    pub fn new() -> Self {
        Self {
            buffer: GapBuffer::new(),
            line_index: LineIndex::new(),
            cursor: Position::default(),
            selection_anchor: None,
            #[cfg(debug_assertions)]
            debug_mutation_count: 0,
        }
    }

    /// Creates a text buffer initialized with the given content.
    ///
    /// Note: We don't implement `FromStr` because it requires returning `Result`,
    /// but parsing a string into a TextBuffer cannot fail.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(content: &str) -> Self {
        let buffer = GapBuffer::from_str(content);
        let mut line_index = LineIndex::new();
        line_index.rebuild(content.chars());

        Self {
            buffer,
            line_index,
            cursor: Position::default(),
            selection_anchor: None,
            #[cfg(debug_assertions)]
            debug_mutation_count: 0,
        }
    }

    // ==================== Accessors ====================

    /// Returns the current cursor position.
    pub fn cursor_position(&self) -> Position {
        self.cursor
    }

    /// Returns the number of lines in the buffer.
    ///
    /// Always at least 1 (even for an empty buffer).
    pub fn line_count(&self) -> usize {
        self.line_index.line_count()
    }

    /// Returns the content of the specified line as a String.
    ///
    /// The returned string does not include the trailing newline (if any).
    /// Returns an empty string if the line index is out of bounds.
    pub fn line_content(&self, line: usize) -> String {
        let total_len = self.buffer.len();

        let start = match self.line_index.line_start(line) {
            Some(s) => s,
            None => return String::new(),
        };

        let end = match self.line_index.line_end(line, total_len) {
            Some(e) => e,
            None => return String::new(),
        };

        self.buffer.slice(start, end)
    }

    /// Returns the length of the specified line (excluding newline).
    pub fn line_len(&self, line: usize) -> usize {
        self.line_index
            .line_len(line, self.buffer.len())
            .unwrap_or(0)
    }

    /// Returns the total character count in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Returns the entire buffer content as a String.
    pub fn content(&self) -> String {
        self.buffer.to_string()
    }

    // ==================== Selection ====================
    // Chunk: docs/chunks/text_selection_model - Selection anchor and range API

    /// Sets the selection anchor to the given position.
    ///
    /// The position is clamped to valid bounds.
    pub fn set_selection_anchor(&mut self, pos: Position) {
        let line = pos.line.min(self.line_count().saturating_sub(1));
        let col = pos.col.min(self.line_len(line));
        self.selection_anchor = Some(Position::new(line, col));
    }

    /// Sets the selection anchor to the current cursor position.
    ///
    /// This is a convenience method for starting a selection at the cursor.
    pub fn set_selection_anchor_at_cursor(&mut self) {
        self.selection_anchor = Some(self.cursor);
    }

    /// Clears the selection anchor (no selection).
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Returns the selection anchor position, if any.
    pub fn selection_anchor(&self) -> Option<Position> {
        self.selection_anchor
    }

    /// Returns true if there is an active selection (anchor is set and differs from cursor).
    pub fn has_selection(&self) -> bool {
        match self.selection_anchor {
            Some(anchor) => anchor != self.cursor,
            None => false,
        }
    }

    /// Returns the selection range as (start, end) in document order.
    ///
    /// Returns `None` if there is no active selection.
    pub fn selection_range(&self) -> Option<(Position, Position)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        // Return in document order (start <= end)
        if anchor < self.cursor {
            Some((anchor, self.cursor))
        } else {
            Some((self.cursor, anchor))
        }
    }

    /// Returns the text within the selection range.
    ///
    /// Returns `None` if there is no active selection.
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let start_offset = self.position_to_offset(start);
        let end_offset = self.position_to_offset(end);
        Some(self.buffer.slice(start_offset, end_offset))
    }

    /// Selects all text in the buffer.
    ///
    /// Sets the anchor to the start of the buffer and cursor to the end.
    pub fn select_all(&mut self) {
        self.selection_anchor = Some(Position::new(0, 0));
        // Set cursor to buffer end without clearing selection
        let last_line = self.line_count().saturating_sub(1);
        let last_col = self.line_len(last_line);
        self.cursor = Position::new(last_line, last_col);
    }

    /// Deletes the selected text and places the cursor at the start of the former selection.
    ///
    /// Returns `DirtyLines::None` if there is no active selection.
    pub fn delete_selection(&mut self) -> DirtyLines {
        let (start, end) = match self.selection_range() {
            Some(range) => range,
            None => return DirtyLines::None,
        };

        let is_multiline = start.line != end.line;
        let start_line = start.line;

        // Convert positions to offsets
        let start_offset = self.position_to_offset(start);
        let end_offset = self.position_to_offset(end);
        let chars_to_delete = end_offset - start_offset;

        // Position cursor at end of selection and delete backward
        self.cursor = end;
        self.sync_gap_to_cursor();

        // Delete characters one by one from end to start
        // This is O(n) in selection size but correct
        for _ in 0..chars_to_delete {
            let deleted = self.buffer.delete_backward();
            if let Some(ch) = deleted {
                if ch == '\n' {
                    // Removing a newline joins lines
                    let prev_line = self.cursor.line - 1;
                    let prev_line_len_before = self.line_len(prev_line);
                    self.line_index.remove_newline(prev_line);
                    self.cursor.line = prev_line;
                    self.cursor.col = prev_line_len_before;
                } else {
                    // Regular character deletion
                    self.line_index.remove_char(self.cursor.line);
                    self.cursor.col -= 1;
                }
                self.sync_gap_to_cursor();
            }
        }

        // Clear selection anchor
        self.selection_anchor = None;

        // Cursor should now be at start position
        self.assert_line_index_consistent();

        if is_multiline {
            DirtyLines::FromLineToEnd(start_line)
        } else {
            DirtyLines::Single(start_line)
        }
    }

    // ==================== Cursor Movement ====================

    /// Converts a (line, col) position to a buffer offset.
    fn position_to_offset(&self, pos: Position) -> usize {
        let line_start = self.line_index.line_start(pos.line).unwrap_or(0);
        line_start + pos.col
    }

    /// Moves the cursor left by one character.
    ///
    /// If at the beginning of a line, moves to the end of the previous line.
    /// If at the beginning of the buffer, does nothing.
    /// Clears any active selection.
    pub fn move_left(&mut self) {
        self.clear_selection();
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_len(self.cursor.line);
        }
    }

    /// Moves the cursor right by one character.
    ///
    /// If at the end of a line, moves to the beginning of the next line.
    /// If at the end of the buffer, does nothing.
    /// Clears any active selection.
    pub fn move_right(&mut self) {
        self.clear_selection();
        let line_len = self.line_len(self.cursor.line);

        if self.cursor.col < line_len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    /// Moves the cursor up by one line.
    ///
    /// The column is clamped to the length of the target line.
    /// If at the first line, does nothing.
    /// Clears any active selection.
    pub fn move_up(&mut self) {
        self.clear_selection();
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let line_len = self.line_len(self.cursor.line);
            self.cursor.col = self.cursor.col.min(line_len);
        }
    }

    /// Moves the cursor down by one line.
    ///
    /// The column is clamped to the length of the target line.
    /// If at the last line, does nothing.
    /// Clears any active selection.
    pub fn move_down(&mut self) {
        self.clear_selection();
        if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            let line_len = self.line_len(self.cursor.line);
            self.cursor.col = self.cursor.col.min(line_len);
        }
    }

    /// Moves the cursor to the start of the current line.
    /// Clears any active selection.
    pub fn move_to_line_start(&mut self) {
        self.clear_selection();
        self.cursor.col = 0;
    }

    /// Moves the cursor to the end of the current line.
    /// Clears any active selection.
    pub fn move_to_line_end(&mut self) {
        self.clear_selection();
        self.cursor.col = self.line_len(self.cursor.line);
    }

    /// Moves the cursor to the start of the buffer (line 0, column 0).
    /// Clears any active selection.
    pub fn move_to_buffer_start(&mut self) {
        self.clear_selection();
        self.cursor = Position::new(0, 0);
    }

    /// Moves the cursor to the end of the buffer.
    /// Clears any active selection.
    pub fn move_to_buffer_end(&mut self) {
        self.clear_selection();
        let last_line = self.line_count().saturating_sub(1);
        let last_col = self.line_len(last_line);
        self.cursor = Position::new(last_line, last_col);
    }

    /// Sets the cursor to an arbitrary position.
    ///
    /// The position is clamped to valid bounds.
    /// Clears any active selection.
    pub fn set_cursor(&mut self, pos: Position) {
        self.clear_selection();
        let line = pos.line.min(self.line_count().saturating_sub(1));
        let col = pos.col.min(self.line_len(line));
        self.cursor = Position::new(line, col);
    }

    // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection
    /// Sets the cursor to an arbitrary position without clearing selection.
    ///
    /// This is used during drag operations where we want to extend the selection
    /// from a fixed anchor. The position is clamped to valid bounds.
    pub fn move_cursor_preserving_selection(&mut self, pos: Position) {
        let line = pos.line.min(self.line_count().saturating_sub(1));
        let col = pos.col.min(self.line_len(line));
        self.cursor = Position::new(line, col);
    }

    // ==================== Validation ====================

    /// Debug assertion: verifies that the incremental line_index matches
    /// a fresh rebuild from the buffer content.
    ///
    /// This catches cumulative drift between incremental updates
    /// (insert_char, insert_newline, remove_char, remove_newline) and
    /// the ground truth. Compiled out in release builds.
    ///
    /// Uses a mutation counter so the O(n) rebuild doesn't tank perf
    /// in tight loops — checks every 64th mutation.
    #[cfg(debug_assertions)]
    fn assert_line_index_consistent(&mut self) {
        self.debug_mutation_count += 1;
        if self.debug_mutation_count % 64 != 0 {
            return;
        }
        let mut expected = LineIndex::new();
        expected.rebuild(self.buffer.chars());
        let actual = self.line_index.line_starts();
        let expected_starts = expected.line_starts();
        assert_eq!(
            actual, expected_starts,
            "line_index drift detected after {} mutations!\n  cursor: {:?}\n  buffer len: {}\n  actual line_starts:   {:?}\n  expected line_starts: {:?}",
            self.debug_mutation_count, self.cursor, self.buffer.len(), actual, expected_starts,
        );
    }

    #[cfg(not(debug_assertions))]
    fn assert_line_index_consistent(&mut self) {}

    // ==================== Mutations ====================

    /// Ensures the gap buffer's gap is at the cursor position.
    fn sync_gap_to_cursor(&mut self) {
        let offset = self.position_to_offset(self.cursor);
        self.buffer.move_gap_to(offset);
    }

    /// Inserts a character at the cursor position.
    ///
    /// If there is an active selection, deletes it first before inserting.
    /// Returns the dirty lines affected by this operation.
    pub fn insert_char(&mut self, ch: char) -> DirtyLines {
        if ch == '\n' {
            return self.insert_newline();
        }

        // Delete selection first if present
        let mut dirty = self.delete_selection();

        self.sync_gap_to_cursor();
        self.buffer.insert(ch);
        self.line_index.insert_char(self.cursor.line);

        let dirty_line = self.cursor.line;
        self.cursor.col += 1;

        self.assert_line_index_consistent();
        dirty.merge(DirtyLines::Single(dirty_line));
        dirty
    }

    /// Inserts a newline at the cursor position, splitting the current line.
    ///
    /// If there is an active selection, deletes it first before inserting.
    /// Returns the dirty lines affected by this operation.
    pub fn insert_newline(&mut self) -> DirtyLines {
        // Delete selection first if present
        let mut dirty = self.delete_selection();

        self.sync_gap_to_cursor();
        let offset = self.position_to_offset(self.cursor);

        self.buffer.insert('\n');
        self.line_index.insert_newline(offset);

        let dirty_from = self.cursor.line;

        // Move cursor to the start of the new line
        self.cursor.line += 1;
        self.cursor.col = 0;

        self.assert_line_index_consistent();
        dirty.merge(DirtyLines::FromLineToEnd(dirty_from));
        dirty
    }

    /// Deletes the character before the cursor (Backspace).
    ///
    /// If there is an active selection, deletes the selection instead.
    /// Returns the dirty lines affected by this operation.
    /// If at the beginning of the buffer with no selection, returns `DirtyLines::None`.
    pub fn delete_backward(&mut self) -> DirtyLines {
        // If there's a selection, delete it and return
        if self.has_selection() {
            return self.delete_selection();
        }

        if self.cursor.col == 0 && self.cursor.line == 0 {
            // At the very beginning of the buffer
            return DirtyLines::None;
        }

        self.sync_gap_to_cursor();

        if self.cursor.col == 0 {
            // At the beginning of a line - delete the newline, joining with previous line
            let deleted = self.buffer.delete_backward();
            if deleted != Some('\n') {
                // Should not happen, but handle gracefully
                return DirtyLines::None;
            }

            let prev_line = self.cursor.line - 1;
            let prev_line_len = self.line_len(prev_line);

            self.line_index.remove_newline(prev_line);

            self.cursor.line = prev_line;
            self.cursor.col = prev_line_len;

            self.assert_line_index_consistent();
            DirtyLines::FromLineToEnd(prev_line)
        } else {
            // Delete a regular character within the line
            let deleted = self.buffer.delete_backward();
            if deleted.is_none() {
                return DirtyLines::None;
            }

            self.line_index.remove_char(self.cursor.line);
            self.cursor.col -= 1;

            self.assert_line_index_consistent();
            DirtyLines::Single(self.cursor.line)
        }
    }

    /// Deletes the character after the cursor (Delete key).
    ///
    /// If there is an active selection, deletes the selection instead.
    /// Returns the dirty lines affected by this operation.
    /// If at the end of the buffer with no selection, returns `DirtyLines::None`.
    pub fn delete_forward(&mut self) -> DirtyLines {
        // If there's a selection, delete it and return
        if self.has_selection() {
            return self.delete_selection();
        }

        let line_len = self.line_len(self.cursor.line);
        let is_last_line = self.cursor.line + 1 >= self.line_count();

        if self.cursor.col >= line_len && is_last_line {
            // At the very end of the buffer
            return DirtyLines::None;
        }

        self.sync_gap_to_cursor();

        if self.cursor.col >= line_len {
            // At the end of a line (but not last line) - delete the newline, joining lines
            let deleted = self.buffer.delete_forward();
            if deleted != Some('\n') {
                // Should not happen, but handle gracefully
                return DirtyLines::None;
            }

            self.line_index.remove_newline(self.cursor.line);

            // Cursor stays in place
            self.assert_line_index_consistent();
            DirtyLines::FromLineToEnd(self.cursor.line)
        } else {
            // Delete a regular character within the line
            let deleted = self.buffer.delete_forward();
            if deleted.is_none() {
                return DirtyLines::None;
            }

            self.line_index.remove_char(self.cursor.line);

            // Cursor stays in place
            self.assert_line_index_consistent();
            DirtyLines::Single(self.cursor.line)
        }
    }

    /// Deletes all characters from the cursor to the end of the current line.
    ///
    /// This implements kill-line (Emacs `C-k`) behavior:
    /// - If the cursor is mid-line, deletes all characters from cursor to end of line
    /// - If the cursor is at the end of the line, deletes the newline (joining with next line)
    /// - If the cursor is at the end of the buffer, does nothing (returns `DirtyLines::None`)
    ///
    /// The cursor position does not change after this operation.
    pub fn delete_to_line_end(&mut self) -> DirtyLines {
        let line_len = self.line_len(self.cursor.line);
        let is_last_line = self.cursor.line + 1 >= self.line_count();

        if self.cursor.col >= line_len {
            // Cursor is at end of line
            if is_last_line {
                // At the very end of the buffer - nothing to delete
                return DirtyLines::None;
            }

            // Delete the newline, joining with the next line (same as delete_forward at line end)
            self.sync_gap_to_cursor();
            let deleted = self.buffer.delete_forward();
            if deleted != Some('\n') {
                // Should not happen, but handle gracefully
                return DirtyLines::None;
            }

            self.line_index.remove_newline(self.cursor.line);

            // Cursor stays in place
            self.assert_line_index_consistent();
            DirtyLines::FromLineToEnd(self.cursor.line)
        } else {
            // Cursor is mid-line: delete from cursor to end of line
            let chars_to_delete = line_len - self.cursor.col;

            self.sync_gap_to_cursor();

            for _ in 0..chars_to_delete {
                let _ = self.buffer.delete_forward();
                self.line_index.remove_char(self.cursor.line);
            }

            // Cursor stays in place
            self.assert_line_index_consistent();
            DirtyLines::Single(self.cursor.line)
        }
    }

    /// Deletes all characters from the cursor to the start of the current line.
    ///
    /// This implements Cmd+Backspace (macOS standard) behavior:
    /// - If there is an active selection, deletes the selection (consistent with other delete operations)
    /// - If the cursor is at column 0, does nothing (returns `DirtyLines::None`)
    /// - Otherwise, deletes all characters from cursor position back to column 0
    ///
    /// Unlike `delete_to_line_end`, this operation does NOT join lines - it only affects
    /// content on the current line. The cursor moves to column 0 after the operation.
    pub fn delete_to_line_start(&mut self) -> DirtyLines {
        // If there's a selection, delete it and return
        if self.has_selection() {
            return self.delete_selection();
        }

        // At column 0 - nothing to delete
        if self.cursor.col == 0 {
            return DirtyLines::None;
        }

        let chars_to_delete = self.cursor.col;
        let current_line = self.cursor.line;

        self.sync_gap_to_cursor();

        // Delete backward from cursor to column 0
        for _ in 0..chars_to_delete {
            let _ = self.buffer.delete_backward();
            self.line_index.remove_char(current_line);
        }

        // Move cursor to column 0
        self.cursor.col = 0;

        self.assert_line_index_consistent();
        DirtyLines::Single(current_line)
    }

    /// Inserts a string at the cursor position.
    ///
    /// If there is an active selection, deletes it first before inserting.
    /// This is a convenience method that inserts each character in sequence.
    /// Returns the combined dirty region.
    pub fn insert_str(&mut self, s: &str) -> DirtyLines {
        if s.is_empty() {
            return DirtyLines::None;
        }

        // Delete selection first if present
        let mut dirty = self.delete_selection();
        let start_line = self.cursor.line;
        let mut has_newline = false;

        for ch in s.chars() {
            if ch == '\n' {
                has_newline = true;
            }
            // Note: insert_char won't delete selection again since we already cleared it
            let _ = self.insert_char(ch);
        }

        let insert_dirty = if has_newline {
            DirtyLines::FromLineToEnd(start_line)
        } else {
            DirtyLines::Single(start_line)
        };
        dirty.merge(insert_dirty);
        dirty
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Selection Anchor Tests ====================

    #[test]
    fn test_set_selection_anchor() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_selection_anchor(Position::new(0, 2));
        assert_eq!(buf.selection_anchor, Some(Position::new(0, 2)));
    }

    #[test]
    fn test_set_selection_anchor_at_cursor() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 3));
        buf.set_selection_anchor_at_cursor();
        assert_eq!(buf.selection_anchor, Some(Position::new(1, 3)));
    }

    #[test]
    fn test_clear_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_selection_anchor(Position::new(0, 2));
        buf.clear_selection();
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_has_selection_false_when_no_anchor() {
        let buf = TextBuffer::from_str("hello");
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_has_selection_false_when_anchor_equals_cursor() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 3));
        buf.set_selection_anchor(Position::new(0, 3));
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_has_selection_true_when_selection_exists() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        assert!(buf.has_selection());
    }

    // ==================== Selection Range Tests ====================

    #[test]
    fn test_selection_range_forward() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 5));
        buf.set_selection_anchor(Position::new(0, 0));
        let range = buf.selection_range().unwrap();
        assert_eq!(range, (Position::new(0, 0), Position::new(0, 5)));
    }

    #[test]
    fn test_selection_range_backward() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 0));
        buf.set_selection_anchor(Position::new(0, 5));
        // Should still return in document order (start <= end)
        let range = buf.selection_range().unwrap();
        assert_eq!(range, (Position::new(0, 0), Position::new(0, 5)));
    }

    #[test]
    fn test_selection_range_multiline() {
        let mut buf = TextBuffer::from_str("hello\nworld\ntest");
        buf.set_cursor(Position::new(2, 2));
        buf.set_selection_anchor(Position::new(0, 3));
        let range = buf.selection_range().unwrap();
        assert_eq!(range, (Position::new(0, 3), Position::new(2, 2)));
    }

    #[test]
    fn test_selection_range_none_when_no_anchor() {
        let buf = TextBuffer::from_str("hello");
        assert!(buf.selection_range().is_none());
    }

    #[test]
    fn test_selected_text_single_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        assert_eq!(buf.selected_text(), Some("ell".to_string()));
    }

    #[test]
    fn test_selected_text_multiline() {
        let mut buf = TextBuffer::from_str("hello\nworld\ntest");
        buf.set_cursor(Position::new(1, 3));
        buf.set_selection_anchor(Position::new(0, 3));
        // Should get "lo\nwor"
        assert_eq!(buf.selected_text(), Some("lo\nwor".to_string()));
    }

    #[test]
    fn test_selected_text_empty_when_anchor_equals_cursor() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 3));
        buf.set_selection_anchor(Position::new(0, 3));
        assert!(buf.selected_text().is_none());
    }

    // ==================== Select All Tests ====================

    #[test]
    fn test_select_all_empty_buffer() {
        let mut buf = TextBuffer::new();
        buf.select_all();
        // Empty buffer: anchor at (0,0), cursor at (0,0), no selection
        assert_eq!(buf.selection_anchor, Some(Position::new(0, 0)));
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_select_all_single_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.select_all();
        assert_eq!(buf.selection_anchor, Some(Position::new(0, 0)));
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert!(buf.has_selection());
        assert_eq!(buf.selected_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_select_all_multiline() {
        let mut buf = TextBuffer::from_str("hello\nworld\ntest");
        buf.select_all();
        assert_eq!(buf.selection_anchor, Some(Position::new(0, 0)));
        assert_eq!(buf.cursor_position(), Position::new(2, 4)); // end of "test"
        assert!(buf.has_selection());
        assert_eq!(buf.selected_text(), Some("hello\nworld\ntest".to_string()));
    }

    // ==================== Delete Selection Tests ====================

    #[test]
    fn test_delete_selection_single_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        let dirty = buf.delete_selection();
        assert_eq!(buf.content(), "ho");
        assert_eq!(buf.cursor_position(), Position::new(0, 1));
        assert_eq!(buf.selection_anchor, None);
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_selection_multiline() {
        let mut buf = TextBuffer::from_str("hello\nworld\ntest");
        buf.set_cursor(Position::new(1, 3));
        buf.set_selection_anchor(Position::new(0, 3));
        let dirty = buf.delete_selection();
        assert_eq!(buf.content(), "helld\ntest");
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
        assert_eq!(buf.selection_anchor, None);
        // Multi-line deletion should return FromLineToEnd
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_delete_selection_backward_selection() {
        // Anchor after cursor
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 1));
        buf.set_selection_anchor(Position::new(0, 4));
        let dirty = buf.delete_selection();
        assert_eq!(buf.content(), "ho");
        assert_eq!(buf.cursor_position(), Position::new(0, 1));
        assert_eq!(buf.selection_anchor, None);
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_selection_clears_anchor() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        let _ = buf.delete_selection();
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_delete_selection_cursor_at_start() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        let _ = buf.delete_selection();
        // Cursor should be at the start of the former selection
        assert_eq!(buf.cursor_position(), Position::new(0, 1));
    }

    #[test]
    fn test_delete_selection_no_op_when_no_selection() {
        let mut buf = TextBuffer::from_str("hello");
        // No selection anchor set
        let dirty = buf.delete_selection();
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_selection_no_op_when_anchor_equals_cursor() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 2));
        buf.set_selection_anchor(Position::new(0, 2));
        let dirty = buf.delete_selection();
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::None);
    }

    // ==================== Mutations with Selection Tests ====================

    #[test]
    fn test_insert_char_with_selection_replaces() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1)); // selects "ell"
        let _ = buf.insert_char('X');
        assert_eq!(buf.content(), "hXo");
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_insert_newline_with_selection_replaces() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1)); // selects "ell"
        let _ = buf.insert_newline();
        assert_eq!(buf.content(), "h\no");
        assert_eq!(buf.line_count(), 2);
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_insert_str_with_selection_replaces() {
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11));
        buf.set_selection_anchor(Position::new(0, 6)); // selects "world"
        let _ = buf.insert_str("universe");
        assert_eq!(buf.content(), "hello universe");
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_delete_backward_with_selection_deletes_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1)); // selects "ell"
        let _ = buf.delete_backward();
        // Should delete only the selection, not an additional char
        assert_eq!(buf.content(), "ho");
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_delete_forward_with_selection_deletes_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1)); // selects "ell"
        let _ = buf.delete_forward();
        // Should delete only the selection, not an additional char
        assert_eq!(buf.content(), "ho");
        assert!(!buf.has_selection());
    }

    // ==================== Movement Clears Selection Tests ====================

    #[test]
    fn test_move_left_clears_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.move_left();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_right_clears_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.move_right();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_up_clears_selection() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 2));
        buf.set_selection_anchor(Position::new(1, 0));
        buf.move_up();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_down_clears_selection() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.move_down();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_to_line_start_clears_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.move_to_line_start();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_to_line_end_clears_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 3));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.move_to_line_end();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_to_buffer_start_clears_selection() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 2));
        buf.set_selection_anchor(Position::new(0, 3));
        buf.move_to_buffer_start();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_move_to_buffer_end_clears_selection() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.move_to_buffer_end();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    #[test]
    fn test_set_cursor_clears_selection() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 4));
        buf.set_selection_anchor(Position::new(0, 1));
        buf.set_cursor(Position::new(0, 2));
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor, None);
    }

    // ==================== Move Cursor Preserving Selection Tests ====================
    // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection

    #[test]
    fn test_move_cursor_preserving_selection_keeps_anchor() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 0));
        buf.set_selection_anchor_at_cursor();
        buf.move_cursor_preserving_selection(Position::new(0, 3));

        // Cursor should move
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
        // Anchor should remain unchanged
        assert_eq!(buf.selection_anchor, Some(Position::new(0, 0)));
        // Should have selection from anchor to cursor
        assert!(buf.has_selection());
    }

    #[test]
    fn test_move_cursor_preserving_selection_clamps_line() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 0));
        buf.set_selection_anchor_at_cursor();
        // Try to move past last line
        buf.move_cursor_preserving_selection(Position::new(10, 0));

        // Should clamp to last line
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
    }

    #[test]
    fn test_move_cursor_preserving_selection_clamps_column() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 0));
        buf.set_selection_anchor_at_cursor();
        // Try to move past end of line
        buf.move_cursor_preserving_selection(Position::new(0, 100));

        // Should clamp to line length
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_move_cursor_preserving_selection_multiline() {
        let mut buf = TextBuffer::from_str("hello\nworld\nfoo");
        buf.set_cursor(Position::new(0, 2));
        buf.set_selection_anchor_at_cursor();
        buf.move_cursor_preserving_selection(Position::new(2, 1));

        // Cursor moved to line 2, column 1
        assert_eq!(buf.cursor_position(), Position::new(2, 1));
        // Anchor unchanged at original position
        assert_eq!(buf.selection_anchor, Some(Position::new(0, 2)));
        // Should have selection
        assert!(buf.has_selection());
        // Selected text should be "llo\nworld\nf"
        assert_eq!(buf.selected_text(), Some("llo\nworld\nf".to_string()));
    }

    // ==================== Basic Tests ====================

    #[test]
    fn test_new_empty() {
        let buf = TextBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_from_str() {
        let buf = TextBuffer::from_str("hello\nworld");
        assert_eq!(buf.len(), 11);
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_content(0), "hello");
        assert_eq!(buf.line_content(1), "world");
    }

    #[test]
    fn test_line_content_empty_buffer() {
        let buf = TextBuffer::new();
        assert_eq!(buf.line_content(0), "");
    }

    #[test]
    fn test_line_content_out_of_bounds() {
        let buf = TextBuffer::from_str("hello");
        assert_eq!(buf.line_content(99), "");
    }

    // ==================== Insert Tests ====================

    #[test]
    fn test_insert_at_empty_buffer() {
        let mut buf = TextBuffer::new();
        let dirty = buf.insert_char('a');
        assert_eq!(buf.content(), "a");
        assert_eq!(buf.cursor_position(), Position::new(0, 1));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_insert_at_beginning_of_line() {
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.insert_char('x');
        assert_eq!(buf.content(), "xhello");
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_insert_at_middle_of_line() {
        let mut buf = TextBuffer::from_str("hllo");
        buf.set_cursor(Position::new(0, 1));
        let dirty = buf.insert_char('e');
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_insert_at_end_of_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_line_end();
        let dirty = buf.insert_char('!');
        assert_eq!(buf.content(), "hello!");
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_insert_newline() {
        let mut buf = TextBuffer::from_str("helloworld");
        buf.set_cursor(Position::new(0, 5));
        let dirty = buf.insert_newline();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_content(0), "hello");
        assert_eq!(buf.line_content(1), "world");
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_insert_newline_at_end() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_line_end();
        let dirty = buf.insert_newline();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_content(0), "hello");
        assert_eq!(buf.line_content(1), "");
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_insert_newline_at_beginning() {
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.insert_newline();
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_content(0), "");
        assert_eq!(buf.line_content(1), "hello");
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    // ==================== Delete Backward Tests ====================

    #[test]
    fn test_delete_backward_at_start() {
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.delete_backward();
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_backward_middle_of_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 3));
        let dirty = buf.delete_backward();
        assert_eq!(buf.content(), "helo");
        assert_eq!(buf.cursor_position(), Position::new(0, 2));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_end_of_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_line_end();
        let dirty = buf.delete_backward();
        assert_eq!(buf.content(), "hell");
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_joins_lines() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 0));
        let dirty = buf.delete_backward();
        assert_eq!(buf.content(), "helloworld");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    // ==================== Delete Forward Tests ====================

    #[test]
    fn test_delete_forward_at_end() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_buffer_end();
        let dirty = buf.delete_forward();
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_forward_middle_of_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 2));
        let dirty = buf.delete_forward();
        assert_eq!(buf.content(), "helo");
        assert_eq!(buf.cursor_position(), Position::new(0, 2));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_beginning_of_line() {
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.delete_forward();
        assert_eq!(buf.content(), "ello");
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_joins_lines() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 5)); // At end of "hello"
        let dirty = buf.delete_forward();
        assert_eq!(buf.content(), "helloworld");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    // ==================== Cursor Movement Tests ====================

    #[test]
    fn test_move_left_at_buffer_start() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_move_left_within_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 3));
        buf.move_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 2));
    }

    #[test]
    fn test_move_left_wraps_to_previous_line() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 0));
        buf.move_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_move_right_at_buffer_end() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_buffer_end();
        buf.move_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_move_right_within_line() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 1));
    }

    #[test]
    fn test_move_right_wraps_to_next_line() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 5));
        buf.move_right();
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
    }

    #[test]
    fn test_move_up_at_first_line() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 3));
        buf.move_up();
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
    }

    #[test]
    fn test_move_up() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 3));
        buf.move_up();
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
    }

    #[test]
    fn test_move_up_clamps_column() {
        let mut buf = TextBuffer::from_str("hi\nworld");
        buf.set_cursor(Position::new(1, 4));
        buf.move_up();
        assert_eq!(buf.cursor_position(), Position::new(0, 2)); // "hi" is only 2 chars
    }

    #[test]
    fn test_move_down_at_last_line() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 3));
        buf.move_down();
        assert_eq!(buf.cursor_position(), Position::new(1, 3));
    }

    #[test]
    fn test_move_down() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 3));
        buf.move_down();
        assert_eq!(buf.cursor_position(), Position::new(1, 3));
    }

    #[test]
    fn test_move_down_clamps_column() {
        let mut buf = TextBuffer::from_str("hello\nhi");
        buf.set_cursor(Position::new(0, 4));
        buf.move_down();
        assert_eq!(buf.cursor_position(), Position::new(1, 2)); // "hi" is only 2 chars
    }

    #[test]
    fn test_move_to_line_start() {
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 3));
        buf.move_to_line_start();
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_move_to_line_end() {
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_line_end();
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_move_to_buffer_start() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 3));
        buf.move_to_buffer_start();
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_move_to_buffer_end() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.move_to_buffer_end();
        assert_eq!(buf.cursor_position(), Position::new(1, 5));
    }

    #[test]
    fn test_cursor_on_empty_line() {
        let mut buf = TextBuffer::from_str("hello\n\nworld");
        buf.set_cursor(Position::new(1, 0));
        buf.move_to_line_end();
        assert_eq!(buf.cursor_position(), Position::new(1, 0)); // Empty line has length 0
        buf.move_to_line_start();
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
    }

    // ==================== Insert String Tests ====================

    #[test]
    fn test_insert_str_simple() {
        let mut buf = TextBuffer::new();
        let dirty = buf.insert_str("hello");
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_insert_str_with_newlines() {
        let mut buf = TextBuffer::new();
        let dirty = buf.insert_str("hello\nworld");
        assert_eq!(buf.content(), "hello\nworld");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_insert_str_empty() {
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.insert_str("");
        assert_eq!(buf.content(), "hello");
        assert_eq!(dirty, DirtyLines::None);
    }

    // ==================== Delete To Line End Tests ====================

    #[test]
    fn test_delete_to_line_end_from_middle() {
        // Kill from middle of line: "hello world" with cursor at col 5 → "hello"
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 5));
        let dirty = buf.delete_to_line_end();
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_to_line_end_from_start() {
        // Kill from start of line: "hello" with cursor at col 0 → ""
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.delete_to_line_end();
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_to_line_end_joins_lines() {
        // Kill at end of line (joins next line): "hello\nworld" with cursor at col 5 on line 0 → "helloworld"
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(0, 5)); // At end of "hello"
        let dirty = buf.delete_to_line_end();
        assert_eq!(buf.content(), "helloworld");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_delete_to_line_end_empty_line() {
        // Kill on empty line: "\nfoo" joins with next line
        let mut buf = TextBuffer::from_str("\nfoo");
        buf.set_cursor(Position::new(0, 0)); // On the empty line
        let dirty = buf.delete_to_line_end();
        assert_eq!(buf.content(), "foo");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_delete_to_line_end_at_buffer_end() {
        // Kill at end of buffer: no-op
        let mut buf = TextBuffer::from_str("hello");
        buf.move_to_buffer_end();
        let dirty = buf.delete_to_line_end();
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_to_line_end_cursor_unchanged() {
        // Cursor position unchanged after kill (same line, same column)
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 3));
        let _dirty = buf.delete_to_line_end();
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
    }

    #[test]
    fn test_delete_to_line_end_multiline() {
        // Test on second line of multiline buffer
        let mut buf = TextBuffer::from_str("first\nsecond line\nthird");
        buf.set_cursor(Position::new(1, 7)); // In "second line" at col 7
        let dirty = buf.delete_to_line_end();
        assert_eq!(buf.line_content(1), "second ");
        assert_eq!(buf.cursor_position(), Position::new(1, 7));
        assert_eq!(dirty, DirtyLines::Single(1));
    }

    // ==================== Delete To Line Start Tests ====================

    #[test]
    fn test_delete_to_line_start_from_middle() {
        // Cursor at col 5 in "hello world" → deletes "hello", leaves " world" with cursor at col 0
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 5));
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), " world");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_to_line_start_from_end() {
        // Cursor at end of "hello world" (col 11) → deletes entire line content, leaves "" with cursor at col 0
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11));
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_to_line_start_at_col_0() {
        // Cursor at col 0 → no-op, returns DirtyLines::None
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 0));
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), "hello world");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_to_line_start_with_selection() {
        // Active selection → deletes selection (not line-start), delegates to delete_selection()
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 8));
        buf.set_selection_anchor(Position::new(0, 6)); // selects "wo"
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), "hello rld");
        assert_eq!(buf.cursor_position(), Position::new(0, 6));
        assert!(!buf.has_selection());
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_to_line_start_multiline() {
        // In a multi-line buffer, only affects current line, does not join with previous line
        let mut buf = TextBuffer::from_str("first\nsecond line\nthird");
        buf.set_cursor(Position::new(1, 7)); // In "second line" at col 7
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.line_content(0), "first");
        assert_eq!(buf.line_content(1), "line");
        assert_eq!(buf.line_content(2), "third");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
        assert_eq!(dirty, DirtyLines::Single(1));
    }

    #[test]
    fn test_delete_to_line_start_empty_line() {
        // Cursor at col 0 on empty line → no-op
        let mut buf = TextBuffer::from_str("first\n\nthird");
        buf.set_cursor(Position::new(1, 0)); // On the empty line
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), "first\n\nthird");
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
        assert_eq!(dirty, DirtyLines::None);
    }
}
