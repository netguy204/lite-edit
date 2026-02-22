// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! TextBuffer is the main public API for text editing operations.
//!
//! It combines a gap buffer (for efficient character storage) with a line index
//! (for O(1) line access) and tracks cursor position as (line, column).
//!
//! Each mutation operation returns `DirtyLines` indicating which lines changed,
//! enabling downstream rendering to minimize redraws.

use crate::buffer_view::{BufferView, CursorInfo, StyledLine};
use crate::gap_buffer::GapBuffer;
use crate::line_index::LineIndex;
use crate::types::{DirtyLines, Position};

// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Character classification for word boundary detection.
///
/// A "word" is a contiguous run of same-class characters. Boundary detection
/// stops when the character class changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    /// Whitespace characters (space, tab, newline, etc.)
    Whitespace,
    /// Letters: a-z, A-Z, 0-9, underscore
    Letter,
    /// Symbols: everything else (punctuation, operators, etc.)
    Symbol,
}

// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Classifies a character into one of three classes for word boundary detection.
///
/// - `Whitespace`: Any character where `char::is_whitespace()` returns true
/// - `Letter`: ASCII letters (a-z, A-Z), digits (0-9), underscore (_)
/// - `Symbol`: Everything else (punctuation, operators, etc.)
fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_ascii_alphanumeric() || c == '_' {
        CharClass::Letter
    } else {
        CharClass::Symbol
    }
}

// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Returns the start column of the contiguous run containing `chars[col - 1]`.
///
/// Uses the three-class model (Whitespace, Letter, Symbol). A "run" is a maximal
/// contiguous sequence of same-class characters.
///
/// Returns `col` unchanged when `col == 0` or `chars` is empty.
fn word_boundary_left(chars: &[char], col: usize) -> usize {
    if col == 0 || chars.is_empty() {
        return col;
    }
    let col = col.min(chars.len()); // clamp to valid range
    let target_class = char_class(chars[col - 1]);
    let mut boundary = col;
    while boundary > 0 {
        if char_class(chars[boundary - 1]) != target_class {
            break;
        }
        boundary -= 1;
    }
    boundary
}

// Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification
// Spec: docs/trunk/SPEC.md#word-model
/// Returns the first column past the end of the contiguous run starting at `chars[col]`.
///
/// Uses the three-class model (Whitespace, Letter, Symbol). A "run" is a maximal
/// contiguous sequence of same-class characters.
///
/// Returns `col` unchanged when `col >= chars.len()` or `chars` is empty.
fn word_boundary_right(chars: &[char], col: usize) -> usize {
    if col >= chars.len() || chars.is_empty() {
        return col;
    }
    let target_class = char_class(chars[col]);
    let mut boundary = col;
    while boundary < chars.len() {
        if char_class(chars[boundary]) != target_class {
            break;
        }
        boundary += 1;
    }
    boundary
}

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
// Chunk: docs/chunks/buffer_view_trait - BufferView trait implementation
#[derive(Debug)]
pub struct TextBuffer {
    buffer: GapBuffer,
    line_index: LineIndex,
    cursor: Position,
    /// Selection anchor position. When `Some`, the selection spans from anchor to cursor.
    /// The anchor may come before or after the cursor (both directions are valid).
    selection_anchor: Option<Position>,
    /// Accumulated dirty lines for BufferView::take_dirty().
    /// This tracks all mutations since the last drain.
    dirty_lines: DirtyLines,
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
            dirty_lines: DirtyLines::None,
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
            dirty_lines: DirtyLines::None,
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

    // Chunk: docs/chunks/text_selection_rendering - Exposes selection anchor position for selection_range() calculation
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

    // Chunk: docs/chunks/word_double_click_select - Double-click word selection
    // Spec: docs/trunk/SPEC.md#word-model
    /// Selects the word or whitespace run at the given column on the current line.
    ///
    /// Sets the selection anchor at the word start and the cursor at the word end.
    /// Returns `true` if a selection was made, `false` if the line is empty.
    ///
    /// If `col` is past the end of the line, the last run on that line is selected.
    pub fn select_word_at(&mut self, col: usize) -> bool {
        let line_content = self.line_content(self.cursor.line);
        let line_chars: Vec<char> = line_content.chars().collect();

        if line_chars.is_empty() {
            return false;
        }

        // Clamp col to valid range (select last character's run if col past end)
        let col = col.min(line_chars.len().saturating_sub(1));

        let word_start = word_boundary_left(&line_chars, col + 1);
        let word_end = word_boundary_right(&line_chars, col);

        self.selection_anchor = Some(Position::new(self.cursor.line, word_start));
        self.cursor.col = word_end;
        true
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

        let dirty = if is_multiline {
            DirtyLines::FromLineToEnd(start_line)
        } else {
            DirtyLines::Single(start_line)
        };
        self.accumulate_dirty(dirty)
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

    // Chunk: docs/chunks/word_jump_navigation - Word jump navigation
    // Spec: docs/trunk/SPEC.md#word-model
    /// Moves the cursor to the right edge of the current word.
    ///
    /// If the cursor is on whitespace, or at the right edge of a non-whitespace run,
    /// the jump continues past the whitespace to the end of the next non-whitespace run.
    /// Stops at line end. Clears any active selection.
    pub fn move_word_right(&mut self) {
        self.clear_selection();

        let line_content = self.line_content(self.cursor.line);
        let line_chars: Vec<char> = line_content.chars().collect();

        if self.cursor.col >= line_chars.len() {
            // At line end, no-op
            return;
        }

        let cursor_on_whitespace = char_class(line_chars[self.cursor.col]) == CharClass::Whitespace;

        if cursor_on_whitespace {
            // Started on whitespace: skip whitespace, then skip to end of next word
            let past_ws = word_boundary_right(&line_chars, self.cursor.col);
            if past_ws < line_chars.len() {
                let word_end = word_boundary_right(&line_chars, past_ws);
                self.cursor.col = word_end;
            } else {
                self.cursor.col = past_ws;
            }
        } else {
            // Started on non-whitespace: go to end of current word
            let word_end = word_boundary_right(&line_chars, self.cursor.col);
            self.cursor.col = word_end;
        }
    }

    // Chunk: docs/chunks/word_jump_navigation - Word jump navigation
    // Spec: docs/trunk/SPEC.md#word-model
    /// Moves the cursor to the left edge of the current word.
    ///
    /// If the cursor is on whitespace, or at the left edge of a non-whitespace run,
    /// the jump continues past the whitespace to the start of the preceding non-whitespace run.
    /// Stops at column 0. Clears any active selection.
    pub fn move_word_left(&mut self) {
        self.clear_selection();

        if self.cursor.col == 0 {
            // At line start, no-op
            return;
        }

        let line_content = self.line_content(self.cursor.line);
        let line_chars: Vec<char> = line_content.chars().collect();

        // Check char immediately before cursor to determine if we're on whitespace
        // (word_boundary_left looks at chars[col-1])
        let prev_char_is_whitespace = char_class(line_chars[self.cursor.col - 1]) == CharClass::Whitespace;

        if prev_char_is_whitespace {
            // Started on whitespace (or just after it): skip whitespace, then go to start of preceding word
            let past_ws = word_boundary_left(&line_chars, self.cursor.col);
            if past_ws > 0 {
                let word_start = word_boundary_left(&line_chars, past_ws);
                self.cursor.col = word_start;
            } else {
                self.cursor.col = past_ws;
            }
        } else {
            // Started on non-whitespace: go to start of current word
            let word_start = word_boundary_left(&line_chars, self.cursor.col);
            self.cursor.col = word_start;
        }
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
        self.accumulate_dirty(dirty)
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
        self.accumulate_dirty(dirty)
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
            // At the very beginning of the buffer - no-op, don't accumulate
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
            self.accumulate_dirty(DirtyLines::FromLineToEnd(prev_line))
        } else {
            // Delete a regular character within the line
            let deleted = self.buffer.delete_backward();
            if deleted.is_none() {
                return DirtyLines::None;
            }

            self.line_index.remove_char(self.cursor.line);
            self.cursor.col -= 1;

            self.assert_line_index_consistent();
            self.accumulate_dirty(DirtyLines::Single(self.cursor.line))
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
            // At the very end of the buffer - no-op, don't accumulate
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
            self.accumulate_dirty(DirtyLines::FromLineToEnd(self.cursor.line))
        } else {
            // Delete a regular character within the line
            let deleted = self.buffer.delete_forward();
            if deleted.is_none() {
                return DirtyLines::None;
            }

            self.line_index.remove_char(self.cursor.line);

            // Cursor stays in place
            self.assert_line_index_consistent();
            self.accumulate_dirty(DirtyLines::Single(self.cursor.line))
        }
    }

    // Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion
    /// Deletes backward by one word (Alt+Backspace).
    ///
    /// The word boundary rule is character-class based:
    /// - **Non-whitespace class**: If the character immediately before the cursor is non-whitespace,
    ///   delete backward through contiguous non-whitespace characters until hitting whitespace or
    ///   the start of the line.
    /// - **Whitespace class**: If the character immediately before the cursor is whitespace,
    ///   delete backward through contiguous whitespace characters until hitting non-whitespace or
    ///   the start of the line.
    ///
    /// If there is an active selection, deletes the selection instead (consistent with delete_backward).
    /// If at column 0, does nothing (returns `DirtyLines::None`).
    ///
    /// Returns `DirtyLines::Single(line)` for the affected line.
    pub fn delete_backward_word(&mut self) -> DirtyLines {
        // If there's a selection, delete it and return
        if self.has_selection() {
            return self.delete_selection();
        }

        // At column 0, no-op
        if self.cursor.col == 0 {
            return DirtyLines::None;
        }

        // Get the current line content to analyze character classes
        let line_content = self.line_content(self.cursor.line);
        let line_chars: Vec<char> = line_content.chars().collect();

        // Use word_boundary_left to find the start of the run
        let word_start = word_boundary_left(&line_chars, self.cursor.col);
        let chars_to_delete = self.cursor.col - word_start;

        self.sync_gap_to_cursor();

        // Delete characters backward
        for _ in 0..chars_to_delete {
            self.buffer.delete_backward();
            self.line_index.remove_char(self.cursor.line);
        }

        self.cursor.col -= chars_to_delete;

        self.assert_line_index_consistent();
        self.accumulate_dirty(DirtyLines::Single(self.cursor.line))
    }

    // Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
    // Spec: docs/trunk/SPEC.md#word-model
    /// Deletes forward by one word (Alt+D).
    ///
    /// The word boundary rule is character-class based:
    /// - **Non-whitespace class**: If the character at the cursor is non-whitespace,
    ///   delete forward through contiguous non-whitespace characters until hitting whitespace or
    ///   the end of the line.
    /// - **Whitespace class**: If the character at the cursor is whitespace,
    ///   delete forward through contiguous whitespace characters until hitting non-whitespace or
    ///   the end of the line.
    ///
    /// If there is an active selection, deletes the selection instead (consistent with delete_forward).
    /// If at the end of the line (`cursor.col >= line_len`), does nothing (returns `DirtyLines::None`).
    /// Does not delete the newline or join lines.
    ///
    /// Returns `DirtyLines::Single(line)` for the affected line.
    pub fn delete_forward_word(&mut self) -> DirtyLines {
        // If there's a selection, delete it and return
        if self.has_selection() {
            return self.delete_selection();
        }

        // Get the current line content to analyze character classes
        let line_content = self.line_content(self.cursor.line);
        let line_chars: Vec<char> = line_content.chars().collect();

        // At line end, no-op
        if self.cursor.col >= line_chars.len() {
            return DirtyLines::None;
        }

        // Use word_boundary_right to find the end of the run
        let word_end = word_boundary_right(&line_chars, self.cursor.col);
        let chars_to_delete = word_end - self.cursor.col;

        self.sync_gap_to_cursor();

        // Delete characters forward
        for _ in 0..chars_to_delete {
            self.buffer.delete_forward();
            self.line_index.remove_char(self.cursor.line);
        }

        // Cursor stays in place (forward deletion doesn't move cursor)

        self.assert_line_index_consistent();
        self.accumulate_dirty(DirtyLines::Single(self.cursor.line))
    }

    // Chunk: docs/chunks/kill_line - Delete from cursor to end of line (Ctrl+K)
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
            self.accumulate_dirty(DirtyLines::FromLineToEnd(self.cursor.line))
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
            self.accumulate_dirty(DirtyLines::Single(self.cursor.line))
        }
    }

    // Chunk: docs/chunks/delete_to_line_start - Delete from cursor to line start
    /// Deletes all characters from the cursor to the start of the current line.
    ///
    /// This implements Cmd+Backspace (macOS standard) behavior:
    /// - If there is an active selection, deletes the selection (consistent with other delete operations)
    /// - If the cursor is at column 0 on line 0, does nothing (already at buffer start)
    /// - If the cursor is at column 0 on line > 0, joins the current line with the previous line
    ///   by deleting the preceding newline, mirroring `delete_to_line_end`'s join behaviour at
    ///   end-of-line. The cursor moves to `(prev_line, prev_line_len)`.
    /// - Otherwise, deletes all characters from cursor position back to column 0 on the same line.
    pub fn delete_to_line_start(&mut self) -> DirtyLines {
        // If there's a selection, delete it and return
        if self.has_selection() {
            return self.delete_selection();
        }

        // At column 0: either a no-op (line 0) or a line join (line > 0)
        if self.cursor.col == 0 {
            if self.cursor.line == 0 {
                return DirtyLines::None;
            }

            // Join with the previous line by deleting the preceding newline
            let prev_line = self.cursor.line - 1;
            let prev_line_len = self.line_len(prev_line);

            self.sync_gap_to_cursor();
            let deleted = self.buffer.delete_backward();
            if deleted != Some('\n') {
                // Should not happen, but handle gracefully
                return DirtyLines::None;
            }

            self.line_index.remove_newline(prev_line);
            self.cursor.line = prev_line;
            self.cursor.col = prev_line_len;

            self.assert_line_index_consistent();
            return self.accumulate_dirty(DirtyLines::FromLineToEnd(prev_line));
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
        self.accumulate_dirty(DirtyLines::Single(current_line))
    }

    // Chunk: docs/chunks/clipboard_operations - Bulk O(n) paste insertion
    /// Inserts a string at the cursor position.
    ///
    /// If there is an active selection, deletes it first before inserting.
    /// This is a convenience method that inserts each character in sequence.
    /// Returns the combined dirty region.
    pub fn insert_str(&mut self, s: &str) -> DirtyLines {
        if s.is_empty() {
            return DirtyLines::None;
        }

        // Delete any active selection first.
        let mut dirty = self.delete_selection();

        let start_line = self.cursor.line;
        let start_col = self.cursor.col;
        let start_offset = self.position_to_offset(self.cursor);

        // ── single-pass scan of the inserted string ───────────────────────
        // Collect absolute line-start offsets for every '\n' found, and track
        // how many non-newline characters follow the last newline (for cursor col).
        let mut new_line_starts: Vec<usize> = Vec::new();
        let mut char_count: usize = 0;
        let mut chars_since_last_newline: usize = 0;

        for ch in s.chars() {
            char_count += 1;
            if ch == '\n' {
                // The new line begins at the offset immediately after this '\n'.
                new_line_starts.push(start_offset + char_count);
                chars_since_last_newline = 0;
            } else {
                chars_since_last_newline += 1;
            }
        }

        // ── bulk gap-buffer insertion (O(n) amortised) ────────────────────
        // Move the gap once to the cursor position, then fill it in one pass.
        // This avoids the O(n·m) cost of calling sync_gap_to_cursor() and
        // line_index.insert_char() once per character inside a loop.
        self.sync_gap_to_cursor();
        self.buffer.insert_str(s);

        // ── bulk line-index update (O(char_count + existing_lines)) ───────
        // Step 1: shift every existing line start that falls after start_line.
        for ls in self.line_index.line_starts_after_mut(start_line) {
            *ls += char_count;
        }
        // Step 2: splice in the new line starts produced by '\n' characters.
        self.line_index.insert_line_starts_after(start_line, &new_line_starts);

        // ── cursor update ─────────────────────────────────────────────────
        let newline_count = new_line_starts.len();
        if newline_count > 0 {
            self.cursor.line = start_line + newline_count;
            self.cursor.col = chars_since_last_newline;
        } else {
            self.cursor.col = start_col + char_count;
        }

        self.assert_line_index_consistent();

        let insert_dirty = if newline_count > 0 {
            DirtyLines::FromLineToEnd(start_line)
        } else {
            DirtyLines::Single(start_line)
        };
        dirty.merge(insert_dirty);
        self.accumulate_dirty(dirty)
    }

    // ==================== Dirty Tracking ====================
    // Chunk: docs/chunks/buffer_view_trait - BufferView dirty tracking

    /// Accumulates dirty lines into the internal field and returns the value.
    ///
    /// This maintains backward compatibility: callers can use the return value
    /// directly, while the accumulated state is available via `take_dirty()`.
    fn accumulate_dirty(&mut self, dirty: DirtyLines) -> DirtyLines {
        self.dirty_lines.merge(dirty.clone());
        dirty
    }
}

// =============================================================================
// BufferView Implementation
// =============================================================================
// Chunk: docs/chunks/buffer_view_trait - BufferView trait implementation for TextBuffer

impl BufferView for TextBuffer {
    fn line_count(&self) -> usize {
        self.line_index.line_count()
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.line_count() {
            return None;
        }
        let content = self.line_content(line);
        Some(StyledLine::plain(content))
    }

    fn line_len(&self, line: usize) -> usize {
        self.line_index
            .line_len(line, self.buffer.len())
            .unwrap_or(0)
    }

    fn take_dirty(&mut self) -> DirtyLines {
        std::mem::take(&mut self.dirty_lines)
    }

    fn is_editable(&self) -> bool {
        true
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        Some(CursorInfo::block(self.cursor))
    }

    fn selection_range(&self) -> Option<(Position, Position)> {
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

    #[test]
    fn test_insert_str_cursor_after_no_newlines() {
        // Cursor ends at start_col + inserted chars on the same line.
        let mut buf = TextBuffer::new();
        buf.insert_str("abc");
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
    }

    #[test]
    fn test_insert_str_cursor_after_newlines() {
        // Cursor ends at (start_line + newline_count, chars_after_last_newline).
        let mut buf = TextBuffer::new();
        buf.insert_str("hello\nworld\nfoo");
        // 2 newlines → cursor on line 2, col = len("foo") = 3
        assert_eq!(buf.cursor_position(), Position::new(2, 3));
    }

    #[test]
    fn test_insert_str_into_middle_of_multiline_buffer() {
        // Inserting at the start of an existing multiline buffer must shift
        // all subsequent line starts correctly.
        let mut buf = TextBuffer::from_str("beta\ngamma");
        buf.set_cursor(Position::new(0, 0));
        buf.insert_str("alpha\n");
        assert_eq!(buf.content(), "alpha\nbeta\ngamma");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line_content(0), "alpha");
        assert_eq!(buf.line_content(1), "beta");
        assert_eq!(buf.line_content(2), "gamma");
        // Cursor just after the inserted newline → start of line 1, col 0.
        assert_eq!(buf.cursor_position(), Position::new(1, 0));
    }

    #[test]
    fn test_insert_str_replaces_selection() {
        // If a selection is active, insert_str must delete it first.
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11));
        buf.set_selection_anchor(Position::new(0, 6));
        buf.insert_str("Rust");
        assert_eq!(buf.content(), "hello Rust");
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_insert_str_large_no_newlines() {
        // 10 000 character insert — verifies no truncation and correct line index.
        let big = "x".repeat(10_000);
        let mut buf = TextBuffer::new();
        buf.insert_str(&big);
        assert_eq!(buf.content().len(), 10_000);
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.cursor_position(), Position::new(0, 10_000));
    }

    #[test]
    fn test_insert_str_large_with_newlines() {
        // 1 000-line insert — verifies line index has the right number of entries.
        let line = "hello world\n";
        let big = line.repeat(1_000);
        let mut buf = TextBuffer::new();
        buf.insert_str(&big);
        // Each "hello world\n" adds 1 newline → 1 000 newlines → 1 001 lines
        // (the last empty line after the trailing newline).
        assert_eq!(buf.line_count(), 1_001);
        assert_eq!(buf.cursor_position(), Position::new(1_000, 0));
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
        // In a multi-line buffer, cursor mid-line: only deletes within the current line
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
    fn test_delete_to_line_start_at_col_0_joins_prev_line() {
        // Cursor at col 0, line > 0 → joins with previous line (deletes the newline)
        let mut buf = TextBuffer::from_str("first\nsecond");
        buf.set_cursor(Position::new(1, 0)); // At start of "second"
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), "firstsecond");
        assert_eq!(buf.cursor_position(), Position::new(0, 5)); // end of "first"
        assert_eq!(buf.line_count(), 1);
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_delete_to_line_start_empty_line_joins() {
        // Cursor at col 0 on an empty intermediate line → joins with the previous line
        let mut buf = TextBuffer::from_str("first\n\nthird");
        buf.set_cursor(Position::new(1, 0)); // On the empty line
        let dirty = buf.delete_to_line_start();
        assert_eq!(buf.content(), "first\nthird");
        assert_eq!(buf.cursor_position(), Position::new(0, 5)); // end of "first"
        assert_eq!(buf.line_count(), 2);
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    // ==================== Delete Backward Word Tests ====================
    // Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion

    #[test]
    fn test_delete_backward_word_non_whitespace() {
        // "hello world" with cursor at col 11 → "hello " with cursor at col 6
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11)); // After "world"
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "hello ");
        assert_eq!(buf.cursor_position(), Position::new(0, 6));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_word_whitespace() {
        // "hello   " with cursor at col 8 (trailing spaces) → "hello" with cursor at col 5
        let mut buf = TextBuffer::from_str("hello   ");
        buf.set_cursor(Position::new(0, 8)); // After trailing spaces
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_word_at_start_of_line() {
        // Cursor at col 0 is a no-op
        let mut buf = TextBuffer::from_str("hello");
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_backward_word_with_selection() {
        // With an active selection, delete the selection instead of word
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 8)); // At 'r' in "world"
        buf.set_selection_anchor(Position::new(0, 6)); // Selects "wo"
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "hello rld");
        assert_eq!(buf.cursor_position(), Position::new(0, 6)); // At start of former selection
        assert!(!buf.has_selection());
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_word_mid_line_boundary() {
        // "one two three" with cursor at col 7 (after "two") → "one  three" with cursor at col 4
        let mut buf = TextBuffer::from_str("one two three");
        buf.set_cursor(Position::new(0, 7)); // After "two"
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "one  three");
        assert_eq!(buf.cursor_position(), Position::new(0, 4));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_word_single_word_line() {
        // Delete entire word when it's the only thing on the line
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 5));
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_word_at_col_1() {
        // Delete single character when cursor is at col 1
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 1));
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.content(), "ello");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_backward_word_multiline() {
        // Test on second line of multiline buffer
        let mut buf = TextBuffer::from_str("first\nsecond line\nthird");
        buf.set_cursor(Position::new(1, 11)); // After "line" on second line
        let dirty = buf.delete_backward_word();
        assert_eq!(buf.line_content(1), "second ");
        assert_eq!(buf.cursor_position(), Position::new(1, 7));
        assert_eq!(dirty, DirtyLines::Single(1));
    }

    // ==================== Word Boundary Left Tests ====================
    // Chunk: docs/chunks/word_boundary_primitives - Word boundary scanning primitives

    #[test]
    fn test_word_boundary_left_empty_slice() {
        // Empty slice → returns col unchanged
        let chars: Vec<char> = vec![];
        assert_eq!(word_boundary_left(&chars, 0), 0);
        assert_eq!(word_boundary_left(&chars, 5), 5);
    }

    #[test]
    fn test_word_boundary_left_col_zero() {
        // col == 0 → returns 0
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_left(&chars, 0), 0);
    }

    #[test]
    fn test_word_boundary_left_single_char_non_whitespace() {
        // Single non-whitespace char, col at 1 → returns 0
        let chars: Vec<char> = "x".chars().collect();
        assert_eq!(word_boundary_left(&chars, 1), 0);
    }

    #[test]
    fn test_word_boundary_left_single_char_whitespace() {
        // Single whitespace char, col at 1 → returns 0
        let chars: Vec<char> = " ".chars().collect();
        assert_eq!(word_boundary_left(&chars, 1), 0);
    }

    #[test]
    fn test_word_boundary_left_full_line_non_whitespace() {
        // All non-whitespace, col at end → returns 0
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_left(&chars, 5), 0);
    }

    #[test]
    fn test_word_boundary_left_full_line_whitespace() {
        // All whitespace, col at end → returns 0
        let chars: Vec<char> = "     ".chars().collect();
        assert_eq!(word_boundary_left(&chars, 5), 0);
    }

    #[test]
    fn test_word_boundary_left_non_whitespace_surrounded_by_whitespace() {
        // "  hello  " with cursor mid-word → returns start of word
        let chars: Vec<char> = "  hello  ".chars().collect();
        // cursor at col 5 (after "hel") → boundary at col 2 (start of "hello")
        assert_eq!(word_boundary_left(&chars, 5), 2);
        // cursor at col 7 (after "hello") → boundary at col 2
        assert_eq!(word_boundary_left(&chars, 7), 2);
    }

    #[test]
    fn test_word_boundary_left_whitespace_surrounded_by_non_whitespace() {
        // "hello  world" with cursor in whitespace → returns start of whitespace
        let chars: Vec<char> = "hello  world".chars().collect();
        // cursor at col 6 (in whitespace between words) → boundary at col 5
        assert_eq!(word_boundary_left(&chars, 6), 5);
        // cursor at col 7 (at end of whitespace) → boundary at col 5
        assert_eq!(word_boundary_left(&chars, 7), 5);
    }

    #[test]
    fn test_word_boundary_left_col_at_end_of_slice() {
        // col at chars.len() (boundary condition)
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_left(&chars, 5), 0);
    }

    #[test]
    fn test_word_boundary_left_col_beyond_slice() {
        // col > chars.len() should clamp
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_left(&chars, 10), 0);
    }

    #[test]
    fn test_word_boundary_left_mid_run() {
        // "hello world" cursor at col 8 (middle of "world") → returns col 6
        let chars: Vec<char> = "hello world".chars().collect();
        assert_eq!(word_boundary_left(&chars, 8), 6);
    }

    // ==================== Word Boundary Right Tests ====================
    // Chunk: docs/chunks/word_boundary_primitives - Word boundary scanning primitives

    #[test]
    fn test_word_boundary_right_empty_slice() {
        // Empty slice → returns col unchanged
        let chars: Vec<char> = vec![];
        assert_eq!(word_boundary_right(&chars, 0), 0);
        assert_eq!(word_boundary_right(&chars, 5), 5);
    }

    #[test]
    fn test_word_boundary_right_col_at_end() {
        // col >= chars.len() → returns col
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_right(&chars, 5), 5);
        assert_eq!(word_boundary_right(&chars, 10), 10);
    }

    #[test]
    fn test_word_boundary_right_single_char_non_whitespace() {
        // Single non-whitespace char, col at 0 → returns 1
        let chars: Vec<char> = "x".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 1);
    }

    #[test]
    fn test_word_boundary_right_single_char_whitespace() {
        // Single whitespace char, col at 0 → returns 1
        let chars: Vec<char> = " ".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 1);
    }

    #[test]
    fn test_word_boundary_right_full_line_non_whitespace() {
        // All non-whitespace, col at start → returns chars.len()
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 5);
    }

    #[test]
    fn test_word_boundary_right_full_line_whitespace() {
        // All whitespace, col at start → returns chars.len()
        let chars: Vec<char> = "     ".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 5);
    }

    #[test]
    fn test_word_boundary_right_non_whitespace_surrounded_by_whitespace() {
        // "  hello  " with cursor at start of word → returns end of word
        let chars: Vec<char> = "  hello  ".chars().collect();
        // cursor at col 2 (start of "hello") → boundary at col 7 (end of "hello")
        assert_eq!(word_boundary_right(&chars, 2), 7);
        // cursor at col 4 (middle of "hello") → boundary at col 7
        assert_eq!(word_boundary_right(&chars, 4), 7);
    }

    #[test]
    fn test_word_boundary_right_whitespace_surrounded_by_non_whitespace() {
        // "hello  world" with cursor in whitespace → returns end of whitespace
        let chars: Vec<char> = "hello  world".chars().collect();
        // cursor at col 5 (start of whitespace) → boundary at col 7
        assert_eq!(word_boundary_right(&chars, 5), 7);
        // cursor at col 6 (middle of whitespace) → boundary at col 7
        assert_eq!(word_boundary_right(&chars, 6), 7);
    }

    #[test]
    fn test_word_boundary_right_col_at_start() {
        // col at 0 (boundary condition)
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 5);
    }

    #[test]
    fn test_word_boundary_right_mid_run() {
        // "hello world" cursor at col 2 (middle of "hello") → returns col 5
        let chars: Vec<char> = "hello world".chars().collect();
        assert_eq!(word_boundary_right(&chars, 2), 5);
    }

    // ==================== CharClass Tests ====================
    // Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification

    #[test]
    fn test_char_class_whitespace() {
        assert_eq!(char_class(' '), CharClass::Whitespace);
        assert_eq!(char_class('\t'), CharClass::Whitespace);
        assert_eq!(char_class('\n'), CharClass::Whitespace);
        assert_eq!(char_class('\r'), CharClass::Whitespace);
    }

    #[test]
    fn test_char_class_letter_lowercase() {
        for c in 'a'..='z' {
            assert_eq!(char_class(c), CharClass::Letter);
        }
    }

    #[test]
    fn test_char_class_letter_uppercase() {
        for c in 'A'..='Z' {
            assert_eq!(char_class(c), CharClass::Letter);
        }
    }

    #[test]
    fn test_char_class_letter_digits() {
        for c in '0'..='9' {
            assert_eq!(char_class(c), CharClass::Letter);
        }
    }

    #[test]
    fn test_char_class_letter_underscore() {
        assert_eq!(char_class('_'), CharClass::Letter);
    }

    #[test]
    fn test_char_class_symbol() {
        // Common programming symbols
        for c in ['.', '+', '-', '*', '/', '(', ')', '{', '}', '[', ']', ':', ';', '"', '\'', '!', '@', '#', '$', '%', '^', '&', '=', '<', '>', '?', '|', '\\', '`', '~', ','] {
            assert_eq!(char_class(c), CharClass::Symbol, "Expected Symbol for '{}'", c);
        }
    }

    // ==================== Triclass Boundary Tests ====================
    // Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification

    #[test]
    fn test_word_boundary_left_letter_symbol_transition() {
        // "foo.bar" - cursor after 'r' at col 7 → boundary at col 4 (start of "bar")
        let chars: Vec<char> = "foo.bar".chars().collect();
        assert_eq!(word_boundary_left(&chars, 7), 4);
    }

    #[test]
    fn test_word_boundary_left_symbol_letter_transition() {
        // "foo.bar" - cursor after '.' at col 4 → boundary at col 3 (start of ".")
        let chars: Vec<char> = "foo.bar".chars().collect();
        assert_eq!(word_boundary_left(&chars, 4), 3);
    }

    #[test]
    fn test_word_boundary_left_symbol_run() {
        // "..abc" - cursor after second '.' at col 2 → boundary at col 0
        let chars: Vec<char> = "..abc".chars().collect();
        assert_eq!(word_boundary_left(&chars, 2), 0);
    }

    #[test]
    fn test_word_boundary_left_mixed_operators() {
        // "result+=value" - cursor after '=' at col 8 → boundary at col 6 (start of "+=")
        let chars: Vec<char> = "result+=value".chars().collect();
        assert_eq!(word_boundary_left(&chars, 8), 6);
    }

    #[test]
    fn test_word_boundary_left_underscore_as_letter() {
        // "my_var" - cursor at col 6 → boundary at col 0 (underscore is a letter)
        let chars: Vec<char> = "my_var".chars().collect();
        assert_eq!(word_boundary_left(&chars, 6), 0);
    }

    #[test]
    fn test_word_boundary_left_digits_as_letter() {
        // "x42" - cursor at col 3 → boundary at col 0 (digits are letters)
        let chars: Vec<char> = "x42".chars().collect();
        assert_eq!(word_boundary_left(&chars, 3), 0);
    }

    #[test]
    fn test_word_boundary_right_letter_symbol_transition() {
        // "foo.bar" - cursor at col 0 → boundary at col 3 (end of "foo")
        let chars: Vec<char> = "foo.bar".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 3);
    }

    #[test]
    fn test_word_boundary_right_symbol_letter_transition() {
        // "foo.bar" - cursor at col 3 → boundary at col 4 (end of ".")
        let chars: Vec<char> = "foo.bar".chars().collect();
        assert_eq!(word_boundary_right(&chars, 3), 4);
    }

    #[test]
    fn test_word_boundary_right_symbol_run() {
        // "fn(x)" - cursor at col 2 → boundary at col 3 (end of "(")
        let chars: Vec<char> = "fn(x)".chars().collect();
        assert_eq!(word_boundary_right(&chars, 2), 3);
    }

    #[test]
    fn test_word_boundary_right_mixed_expression() {
        // "fn(x) + y" - cursor at col 6 → boundary at col 7 (end of "+")
        let chars: Vec<char> = "fn(x) + y".chars().collect();
        assert_eq!(word_boundary_right(&chars, 6), 7);
    }

    #[test]
    fn test_word_boundary_right_underscore_as_letter() {
        // "_foo" - cursor at col 0 → boundary at col 4 (underscore is a letter)
        let chars: Vec<char> = "_foo".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 4);
    }

    #[test]
    fn test_word_boundary_right_digits_as_letter() {
        // "var123" - cursor at col 0 → boundary at col 6 (digits are letters)
        let chars: Vec<char> = "var123".chars().collect();
        assert_eq!(word_boundary_right(&chars, 0), 6);
    }

    // ==================== Move Word Right Tests ====================
    // Chunk: docs/chunks/word_jump_navigation - Word jump navigation

    #[test]
    fn test_move_word_right_mid_word() {
        // Cursor mid-word → lands at word end (right edge of non-whitespace run)
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 2)); // In "hello" at col 2
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 5)); // End of "hello"
    }

    #[test]
    fn test_move_word_right_at_word_start() {
        // Cursor at word start → lands at same word's end
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 0)); // Start of "hello"
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 5)); // End of "hello"
    }

    #[test]
    fn test_move_word_right_at_word_end() {
        // Cursor at word end → jumps past whitespace to next word's end
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 5)); // End of "hello"
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 11)); // End of "world"
    }

    #[test]
    fn test_move_word_right_on_whitespace() {
        // Cursor on whitespace between words → jumps to end of next non-whitespace run
        let mut buf = TextBuffer::from_str("hello   world");
        buf.set_cursor(Position::new(0, 6)); // In whitespace
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 13)); // End of "world"
    }

    #[test]
    fn test_move_word_right_at_line_start() {
        // Cursor at line start → jumps to end of first word
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 0)); // Line start
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 5)); // End of "hello"
    }

    #[test]
    fn test_move_word_right_at_line_end() {
        // Cursor at line end → stays at line end (no-op)
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 5)); // Line end
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 5)); // Unchanged
    }

    #[test]
    fn test_move_word_right_empty_line() {
        // Empty line → stays at column 0 (no-op)
        let mut buf = TextBuffer::from_str("");
        buf.set_cursor(Position::new(0, 0));
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_move_word_right_single_char_word() {
        // Single-character word → lands at column 1
        let mut buf = TextBuffer::from_str("a b c");
        buf.set_cursor(Position::new(0, 0)); // Start of "a"
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 1)); // End of "a"
    }

    #[test]
    fn test_move_word_right_clears_selection() {
        // All cases clear any active selection
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 5));
        buf.set_selection_anchor(Position::new(0, 0));
        assert!(buf.has_selection());
        buf.move_word_right();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor(), None);
    }

    #[test]
    fn test_move_word_right_multiple_whitespace() {
        // Multiple whitespace chars treated as one run
        let mut buf = TextBuffer::from_str("hello    world");
        buf.set_cursor(Position::new(0, 5)); // End of "hello"
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 14)); // End of "world"
    }

    #[test]
    fn test_move_word_right_trailing_whitespace() {
        // Trailing whitespace: cursor at last word end → jumps to line end
        let mut buf = TextBuffer::from_str("hello   ");
        buf.set_cursor(Position::new(0, 5)); // End of "hello"
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 8)); // Line end
    }

    // ==================== Move Word Left Tests ====================
    // Chunk: docs/chunks/word_jump_navigation - Word jump navigation

    #[test]
    fn test_move_word_left_mid_word() {
        // Cursor mid-word → lands at word start (left edge of non-whitespace run)
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 8)); // In "world" at col 8
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 6)); // Start of "world"
    }

    #[test]
    fn test_move_word_left_at_word_end() {
        // Cursor at word end → lands at same word's start
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11)); // End of "world" (past last char)
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 6)); // Start of "world"
    }

    #[test]
    fn test_move_word_left_at_word_start() {
        // Cursor at word start → jumps past preceding whitespace to previous word's start
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 6)); // Start of "world"
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Start of "hello"
    }

    #[test]
    fn test_move_word_left_on_whitespace() {
        // Cursor on whitespace between words → jumps to start of preceding non-whitespace run
        let mut buf = TextBuffer::from_str("hello   world");
        buf.set_cursor(Position::new(0, 6)); // In whitespace
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Start of "hello"
    }

    #[test]
    fn test_move_word_left_at_line_start() {
        // Cursor at line start → stays at column 0 (no-op)
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 0)); // Line start
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Unchanged
    }

    #[test]
    fn test_move_word_left_at_line_end() {
        // Cursor at line end → jumps to start of last word
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11)); // Line end (past "world")
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 6)); // Start of "world"
    }

    #[test]
    fn test_move_word_left_empty_line() {
        // Empty line → stays at column 0 (no-op)
        let mut buf = TextBuffer::from_str("");
        buf.set_cursor(Position::new(0, 0));
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_move_word_left_single_char_word() {
        // Single-character word → lands at column 0
        let mut buf = TextBuffer::from_str("a b c");
        buf.set_cursor(Position::new(0, 1)); // End of "a" (on space)
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Start of "a"
    }

    #[test]
    fn test_move_word_left_clears_selection() {
        // All cases clear any active selection
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 11));
        buf.set_selection_anchor(Position::new(0, 0));
        assert!(buf.has_selection());
        buf.move_word_left();
        assert!(!buf.has_selection());
        assert_eq!(buf.selection_anchor(), None);
    }

    #[test]
    fn test_move_word_left_multiple_whitespace() {
        // Multiple whitespace chars treated as one run
        let mut buf = TextBuffer::from_str("hello    world");
        buf.set_cursor(Position::new(0, 9)); // Start of "world"
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Start of "hello"
    }

    #[test]
    fn test_move_word_left_leading_whitespace() {
        // Leading whitespace: cursor at first word start → jumps to column 0
        let mut buf = TextBuffer::from_str("   hello");
        buf.set_cursor(Position::new(0, 3)); // Start of "hello"
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Line start
    }

    // ==================== Delete Forward Word Tests ====================
    // Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion

    #[test]
    fn test_delete_forward_word_mid_word_non_whitespace() {
        // "hello world" with cursor at col 2 (mid-word on non-whitespace)
        // Deletes from cursor to word end → "he world"
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 2)); // At 'l' in "hello"
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "he world");
        assert_eq!(buf.cursor_position(), Position::new(0, 2));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_word_on_whitespace() {
        // "hello  world" with cursor at col 5 (on whitespace between words)
        // Deletes whitespace run only → "helloworld"
        let mut buf = TextBuffer::from_str("hello  world");
        buf.set_cursor(Position::new(0, 5)); // On first space
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "helloworld");
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_word_at_line_end() {
        // Cursor at end of line → no-op, returns DirtyLines::None
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 5)); // At line end
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_position(), Position::new(0, 5));
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_forward_word_at_line_start() {
        // Cursor at line start → deletes first run
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 0)); // At line start
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), " world");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_word_with_selection() {
        // With an active selection, delete the selection instead of word
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 8)); // At 'r' in "world"
        buf.set_selection_anchor(Position::new(0, 6)); // Selects "wo"
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "hello rld");
        assert_eq!(buf.cursor_position(), Position::new(0, 6)); // At start of former selection
        assert!(!buf.has_selection());
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_word_whitespace_only_line() {
        // Line containing only whitespace → deletes entire whitespace run
        let mut buf = TextBuffer::from_str("     ");
        buf.set_cursor(Position::new(0, 0)); // At line start
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    #[test]
    fn test_delete_forward_word_multiline() {
        // Test on second line of multiline buffer
        let mut buf = TextBuffer::from_str("first\nsecond line\nthird");
        buf.set_cursor(Position::new(1, 7)); // After "second ", on 'l' in "line"
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.line_content(1), "second ");
        assert_eq!(buf.cursor_position(), Position::new(1, 7));
        assert_eq!(dirty, DirtyLines::Single(1));
    }

    #[test]
    fn test_delete_forward_word_cursor_past_line_len() {
        // Edge case: cursor.col >= line_len (should be no-op)
        let mut buf = TextBuffer::from_str("hi");
        buf.set_cursor(Position::new(0, 2)); // At line end
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "hi");
        assert_eq!(dirty, DirtyLines::None);
    }

    #[test]
    fn test_delete_forward_word_leading_whitespace() {
        // "  hello" with cursor at col 0 → deletes leading whitespace
        let mut buf = TextBuffer::from_str("  hello");
        buf.set_cursor(Position::new(0, 0));
        let dirty = buf.delete_forward_word();
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
        assert_eq!(dirty, DirtyLines::Single(0));
    }

    // ==================== Select Word At Tests ====================
    // Chunk: docs/chunks/word_double_click_select - Double-click word selection

    #[test]
    fn test_select_word_at_mid_word() {
        // "hello world" with col at middle of "hello" → selects "hello"
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 2));
        let result = buf.select_word_at(2);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 0)); // word start
        assert_eq!(end, Position::new(0, 5)); // word end
        assert_eq!(buf.selected_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_select_word_at_word_start() {
        // "hello world" with col at start of "hello" → selects "hello"
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 0));
        let result = buf.select_word_at(0);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(0, 5));
        assert_eq!(buf.selected_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_select_word_at_whitespace() {
        // "hello  world" with col at whitespace → selects whitespace run
        let mut buf = TextBuffer::from_str("hello  world");
        buf.set_cursor(Position::new(0, 5));
        let result = buf.select_word_at(5);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 5));
        assert_eq!(end, Position::new(0, 7));
        assert_eq!(buf.selected_text(), Some("  ".to_string()));
    }

    #[test]
    fn test_select_word_at_empty_line() {
        // Empty line → returns false, no selection
        let mut buf = TextBuffer::from_str("");
        buf.set_cursor(Position::new(0, 0));
        let result = buf.select_word_at(0);
        assert!(!result);
        assert!(!buf.has_selection());
    }

    #[test]
    fn test_select_word_at_col_zero() {
        // "  hello" with col 0 → selects leading whitespace
        let mut buf = TextBuffer::from_str("  hello");
        buf.set_cursor(Position::new(0, 0));
        let result = buf.select_word_at(0);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(0, 2));
        assert_eq!(buf.selected_text(), Some("  ".to_string()));
    }

    #[test]
    fn test_select_word_at_end_of_line() {
        // "hello world" with col at last char → selects last word
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 10)); // 'd' in "world"
        let result = buf.select_word_at(10);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 6));
        assert_eq!(end, Position::new(0, 11));
        assert_eq!(buf.selected_text(), Some("world".to_string()));
    }

    #[test]
    fn test_select_word_at_past_line_end() {
        // "hello" with col past line end → selects last run
        let mut buf = TextBuffer::from_str("hello");
        buf.set_cursor(Position::new(0, 10)); // Past end
        let result = buf.select_word_at(10);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(0, 5));
        assert_eq!(buf.selected_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_select_word_at_second_word() {
        // "hello world" with col at "world" → selects "world"
        let mut buf = TextBuffer::from_str("hello world");
        buf.set_cursor(Position::new(0, 8));
        let result = buf.select_word_at(8);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 6));
        assert_eq!(end, Position::new(0, 11));
        assert_eq!(buf.selected_text(), Some("world".to_string()));
    }

    #[test]
    fn test_select_word_at_multiline() {
        // Select word on second line
        let mut buf = TextBuffer::from_str("first\nsecond word");
        buf.set_cursor(Position::new(1, 0));
        let result = buf.select_word_at(0);
        assert!(result);
        assert!(buf.has_selection());
        let (start, end) = buf.selection_range().unwrap();
        assert_eq!(start, Position::new(1, 0));
        assert_eq!(end, Position::new(1, 6));
        assert_eq!(buf.selected_text(), Some("second".to_string()));
    }

    // ==================== Triclass Word Operation Integration Tests ====================
    // Chunk: docs/chunks/word_triclass_boundaries - Three-class word boundary classification

    #[test]
    fn test_delete_backward_word_stops_at_symbol() {
        // "foo.bar" with cursor at col 7 → deletes "bar" → "foo."
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 7));
        buf.delete_backward_word();
        assert_eq!(buf.content(), "foo.");
        assert_eq!(buf.cursor_position(), Position::new(0, 4));
    }

    #[test]
    fn test_delete_backward_word_deletes_symbol_run() {
        // "foo.bar" with cursor at col 4 → deletes "." → "foobar"
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 4));
        buf.delete_backward_word();
        assert_eq!(buf.content(), "foobar");
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
    }

    #[test]
    fn test_delete_forward_word_stops_at_symbol() {
        // "foo.bar" with cursor at col 0 → deletes "foo" → ".bar"
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 0));
        buf.delete_forward_word();
        assert_eq!(buf.content(), ".bar");
        assert_eq!(buf.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_move_word_right_stops_at_symbol() {
        // "foo.bar" with cursor at col 0 → moves to col 3 (end of "foo")
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 0));
        buf.move_word_right();
        assert_eq!(buf.cursor_position(), Position::new(0, 3));
    }

    #[test]
    fn test_move_word_left_stops_at_symbol() {
        // "foo.bar" with cursor at col 7 → moves to col 4 (start of "bar")
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 7));
        buf.move_word_left();
        assert_eq!(buf.cursor_position(), Position::new(0, 4));
    }

    #[test]
    fn test_select_word_at_selects_letter_only() {
        // "foo.bar" double-click on 'b' at col 4 → selects "bar" only
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 4));
        buf.select_word_at(4);
        assert_eq!(buf.selected_text(), Some("bar".to_string()));
    }

    #[test]
    fn test_select_word_at_selects_symbol_only() {
        // "foo.bar" double-click on '.' at col 3 → selects "." only
        let mut buf = TextBuffer::from_str("foo.bar");
        buf.set_cursor(Position::new(0, 3));
        buf.select_word_at(3);
        assert_eq!(buf.selected_text(), Some(".".to_string()));
    }

    #[test]
    fn test_underscore_included_in_word() {
        // "my_var" with cursor at col 6 → delete backward deletes entire "my_var"
        let mut buf = TextBuffer::from_str("my_var");
        buf.set_cursor(Position::new(0, 6));
        buf.delete_backward_word();
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn test_digits_included_in_word() {
        // "x42" with cursor at col 3 → delete backward deletes entire "x42"
        let mut buf = TextBuffer::from_str("x42");
        buf.set_cursor(Position::new(0, 3));
        buf.delete_backward_word();
        assert_eq!(buf.content(), "");
    }

    // ==================== BufferView Implementation Tests ====================
    // Chunk: docs/chunks/buffer_view_trait - Tests for BufferView trait implementation

    #[test]
    fn test_buffer_view_line_count() {
        use crate::BufferView;

        let buf = TextBuffer::from_str("hello\nworld\nfoo");
        // Use trait method
        let view: &dyn BufferView = &buf;
        assert_eq!(view.line_count(), 3);
    }

    #[test]
    fn test_buffer_view_styled_line_returns_plain_text() {
        use crate::BufferView;

        let buf = TextBuffer::from_str("hello\nworld");
        let view: &dyn BufferView = &buf;

        let line = view.styled_line(0).unwrap();
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].text, "hello");
        // Default style
        assert!(!line.spans[0].style.bold);

        let line1 = view.styled_line(1).unwrap();
        assert_eq!(line1.spans[0].text, "world");
    }

    #[test]
    fn test_buffer_view_styled_line_out_of_bounds() {
        use crate::BufferView;

        let buf = TextBuffer::from_str("hello");
        let view: &dyn BufferView = &buf;

        assert!(view.styled_line(0).is_some());
        assert!(view.styled_line(1).is_none());
        assert!(view.styled_line(100).is_none());
    }

    #[test]
    fn test_buffer_view_line_len() {
        use crate::BufferView;

        let buf = TextBuffer::from_str("hello\nhi\n");
        let view: &dyn BufferView = &buf;

        assert_eq!(view.line_len(0), 5); // "hello"
        assert_eq!(view.line_len(1), 2); // "hi"
        assert_eq!(view.line_len(2), 0); // empty line
        assert_eq!(view.line_len(100), 0); // out of bounds
    }

    #[test]
    fn test_buffer_view_cursor_info() {
        use crate::{BufferView, CursorShape};

        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.set_cursor(Position::new(1, 3));

        let view: &dyn BufferView = &buf;
        let cursor = view.cursor_info().unwrap();

        assert_eq!(cursor.position, Position::new(1, 3));
        assert_eq!(cursor.shape, CursorShape::Block);
        assert!(cursor.blinking);
    }

    #[test]
    fn test_buffer_view_is_editable() {
        use crate::BufferView;

        let buf = TextBuffer::new();
        let view: &dyn BufferView = &buf;
        assert!(view.is_editable());
    }

    #[test]
    fn test_buffer_view_selection_range_none_when_no_selection() {
        use crate::BufferView;

        let buf = TextBuffer::from_str("hello");
        let view: &dyn BufferView = &buf;
        assert!(view.selection_range().is_none());
    }

    #[test]
    fn test_buffer_view_selection_range_returns_selection() {
        use crate::BufferView;

        let mut buf = TextBuffer::from_str("hello world");
        buf.set_selection_anchor(Position::new(0, 0));
        buf.move_cursor_preserving_selection(Position::new(0, 5));

        let view: &dyn BufferView = &buf;
        let (start, end) = view.selection_range().unwrap();
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(0, 5));
    }

    #[test]
    fn test_buffer_view_take_dirty_accumulates_mutations() {
        use crate::BufferView;

        let mut buf = TextBuffer::from_str("hello");
        // Initial state has no dirty lines
        assert_eq!(buf.take_dirty(), DirtyLines::None);

        // Insert character accumulates dirty
        buf.insert_char('x');
        let dirty = buf.take_dirty();
        assert_eq!(dirty, DirtyLines::Single(0));

        // After take, dirty is cleared
        assert_eq!(buf.take_dirty(), DirtyLines::None);

        // Multiple mutations merge
        buf.insert_char('y');
        buf.insert_newline();
        let dirty = buf.take_dirty();
        // Should be FromLineToEnd since newline affects rest of buffer
        assert_eq!(dirty, DirtyLines::FromLineToEnd(0));
    }

    #[test]
    fn test_buffer_view_object_safety_with_textbuffer() {
        use crate::BufferView;

        let buf = TextBuffer::from_str("test");

        // Verify we can use TextBuffer as Box<dyn BufferView>
        let boxed: Box<dyn BufferView> = Box::new(buf);
        assert_eq!(boxed.line_count(), 1);
        assert!(boxed.is_editable());
    }
}
