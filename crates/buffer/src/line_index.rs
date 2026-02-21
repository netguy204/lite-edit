// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! Line index for tracking line boundaries in the text buffer.
//!
//! Maintains an array of line start offsets for O(1) line count and O(1) line access.
//! Supports incremental updates when lines are inserted or deleted.

/// Tracks line boundaries in a text buffer.
///
/// The line index maintains a list of character offsets where each line starts.
/// This enables O(1) access to line count and O(log n) lookup of which line
/// contains a given offset.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Character offsets where each line starts. line_starts[0] = 0 always.
    /// Each entry points to the first character of that line.
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Creates a new line index with a single empty line.
    pub fn new() -> Self {
        Self {
            line_starts: vec![0],
        }
    }

    /// Rebuilds the line index from the given content.
    ///
    /// This is O(n) where n is the content length, but should only be needed
    /// for bulk operations like loading a file.
    pub fn rebuild<I>(&mut self, content: I)
    where
        I: IntoIterator<Item = char>,
    {
        self.line_starts.clear();
        self.line_starts.push(0);

        let mut offset = 0;
        for ch in content {
            offset += 1;
            if ch == '\n' {
                self.line_starts.push(offset);
            }
        }
    }

    /// Returns the number of lines in the buffer.
    ///
    /// A buffer always has at least one line (even if empty).
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Returns the character offset where the given line starts.
    ///
    /// Returns None if the line index is out of bounds.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line).copied()
    }

    /// Returns the character offset of the end of the given line.
    ///
    /// For all lines except the last, this points to the newline character.
    /// For the last line, this equals the total buffer length.
    ///
    /// `total_len` is the total number of characters in the buffer.
    pub fn line_end(&self, line: usize, total_len: usize) -> Option<usize> {
        if line >= self.line_count() {
            return None;
        }

        if line + 1 < self.line_count() {
            // Not the last line: end is the start of the next line minus 1 (the newline)
            Some(self.line_starts[line + 1] - 1)
        } else {
            // Last line: end is the buffer length
            Some(total_len)
        }
    }

    /// Returns the length of the given line (excluding the newline character).
    pub fn line_len(&self, line: usize, total_len: usize) -> Option<usize> {
        let start = self.line_start(line)?;
        let end = self.line_end(line, total_len)?;
        Some(end - start)
    }

    /// Returns the line number containing the given character offset.
    ///
    /// Uses binary search for O(log n) lookup.
    pub fn line_at_offset(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }

    /// Updates the index when a newline is inserted.
    ///
    /// `offset` is the character position where the newline was inserted.
    /// This shifts all subsequent line starts and inserts a new line.
    pub fn insert_newline(&mut self, offset: usize) {
        // Find which line this newline is being inserted into
        let line = self.line_at_offset(offset);

        // The new line starts at offset + 1 (after the newline character)
        let new_line_start = offset + 1;

        // Shift all subsequent line starts by 1 (for the inserted newline)
        for start in self.line_starts.iter_mut().skip(line + 1) {
            *start += 1;
        }

        // Insert the new line start
        self.line_starts.insert(line + 1, new_line_start);
    }

    /// Updates the index when a newline is removed (lines are joined).
    ///
    /// `line` is the line number where the newline at its end is being removed.
    /// This removes the following line and shifts all subsequent line starts.
    pub fn remove_newline(&mut self, line: usize) {
        if line + 1 >= self.line_count() {
            // No newline at end of last line
            return;
        }

        // Remove the following line (it's being merged into this line)
        self.line_starts.remove(line + 1);

        // Shift all subsequent line starts by -1 (for the removed newline)
        for start in self.line_starts.iter_mut().skip(line + 1) {
            *start -= 1;
        }
    }

    /// Updates the index when a character (not a newline) is inserted.
    ///
    /// `line` is the line where the character was inserted.
    /// This shifts all subsequent line starts by 1.
    pub fn insert_char(&mut self, line: usize) {
        for start in self.line_starts.iter_mut().skip(line + 1) {
            *start += 1;
        }
    }

    /// Updates the index when a character (not a newline) is removed.
    ///
    /// `line` is the line where the character was removed.
    /// This shifts all subsequent line starts by -1.
    pub fn remove_char(&mut self, line: usize) {
        for start in self.line_starts.iter_mut().skip(line + 1) {
            *start -= 1;
        }
    }

    /// Returns a mutable slice of all line starts strictly after `after_line`.
    ///
    /// Used by `TextBuffer::insert_str` to shift existing line starts in bulk
    /// when many characters are inserted at once, avoiding O(n·m) updates.
    pub fn line_starts_after_mut(&mut self, after_line: usize) -> &mut [usize] {
        let start = (after_line + 1).min(self.line_starts.len());
        &mut self.line_starts[start..]
    }

    /// Inserts multiple new line-start offsets after `after_line` in a single
    /// splice, preserving the ascending sort order of the array.
    ///
    /// `new_starts` must already be in ascending order and must logically
    /// follow `line_starts[after_line]`. Used by `TextBuffer::insert_str`.
    pub fn insert_line_starts_after(&mut self, after_line: usize, new_starts: &[usize]) {
        if new_starts.is_empty() {
            return;
        }
        let insert_pos = after_line + 1;
        let old_len = self.line_starts.len();
        let add = new_starts.len();

        // Extend storage, then shift the tail right to make room.
        self.line_starts.resize(old_len + add, 0);
        self.line_starts.copy_within(insert_pos..old_len, insert_pos + add);
        self.line_starts[insert_pos..insert_pos + add].copy_from_slice(new_starts);
    }

    /// Returns the raw line_starts array (for debug validation).
    #[cfg(any(debug_assertions, test))]
    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let index = LineIndex::new();
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_start(0), Some(0));
    }

    #[test]
    fn test_rebuild_empty() {
        let mut index = LineIndex::new();
        index.rebuild("".chars());
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_start(0), Some(0));
    }

    #[test]
    fn test_rebuild_single_line() {
        let mut index = LineIndex::new();
        index.rebuild("hello".chars());
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_start(0), Some(0));
    }

    #[test]
    fn test_rebuild_multiple_lines() {
        let mut index = LineIndex::new();
        index.rebuild("hello\nworld\n".chars());
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_start(0), Some(0));
        assert_eq!(index.line_start(1), Some(6)); // After "hello\n"
        assert_eq!(index.line_start(2), Some(12)); // After "world\n"
    }

    #[test]
    fn test_line_end() {
        let mut index = LineIndex::new();
        index.rebuild("hello\nworld".chars());
        assert_eq!(index.line_end(0, 11), Some(5)); // "hello" ends at 5 (before \n)
        assert_eq!(index.line_end(1, 11), Some(11)); // "world" ends at 11
    }

    #[test]
    fn test_line_len() {
        let mut index = LineIndex::new();
        index.rebuild("hello\nworld".chars());
        assert_eq!(index.line_len(0, 11), Some(5)); // "hello"
        assert_eq!(index.line_len(1, 11), Some(5)); // "world"
    }

    #[test]
    fn test_line_at_offset() {
        let mut index = LineIndex::new();
        index.rebuild("hello\nworld\nfoo".chars());

        assert_eq!(index.line_at_offset(0), 0); // 'h'
        assert_eq!(index.line_at_offset(4), 0); // 'o'
        assert_eq!(index.line_at_offset(5), 0); // '\n'
        assert_eq!(index.line_at_offset(6), 1); // 'w'
        assert_eq!(index.line_at_offset(11), 1); // '\n'
        assert_eq!(index.line_at_offset(12), 2); // 'f'
    }

    #[test]
    fn test_insert_newline() {
        let mut index = LineIndex::new();
        index.rebuild("helloworld".chars());
        assert_eq!(index.line_count(), 1);

        // Insert newline after "hello" (at position 5)
        index.insert_newline(5);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_start(0), Some(0));
        assert_eq!(index.line_start(1), Some(6)); // "world" starts at 6
    }

    #[test]
    fn test_remove_newline() {
        let mut index = LineIndex::new();
        index.rebuild("hello\nworld".chars());
        assert_eq!(index.line_count(), 2);

        // Remove newline at end of line 0
        index.remove_newline(0);
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_start(0), Some(0));
    }

    #[test]
    fn test_insert_char() {
        let mut index = LineIndex::new();
        index.rebuild("a\nb\nc".chars());
        assert_eq!(index.line_start(0), Some(0));
        assert_eq!(index.line_start(1), Some(2));
        assert_eq!(index.line_start(2), Some(4));

        // Insert a character on line 0
        index.insert_char(0);
        assert_eq!(index.line_start(0), Some(0)); // Unchanged
        assert_eq!(index.line_start(1), Some(3)); // Shifted by 1
        assert_eq!(index.line_start(2), Some(5)); // Shifted by 1
    }

    #[test]
    fn test_remove_char() {
        let mut index = LineIndex::new();
        index.rebuild("aa\nbb\ncc".chars());
        assert_eq!(index.line_start(1), Some(3));
        assert_eq!(index.line_start(2), Some(6));

        // Remove a character on line 0
        index.remove_char(0);
        assert_eq!(index.line_start(1), Some(2));
        assert_eq!(index.line_start(2), Some(5));
    }
}
