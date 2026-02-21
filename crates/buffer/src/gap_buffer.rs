// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! Gap buffer implementation for efficient text editing.
//!
//! A gap buffer is a character array with a movable gap at the cursor position.
//! Insertions and deletions at the cursor are O(1); moving the cursor is O(gap_distance)
//! but amortizes well for typical editing patterns (locality of edits).

const INITIAL_GAP_SIZE: usize = 64;
const GAP_GROWTH_FACTOR: usize = 2;

/// A gap buffer for efficient text storage and manipulation.
///
/// The buffer stores characters with a "gap" - an empty region that can be moved
/// to any position. Operations at the gap position are O(1), making it ideal for
/// text editing where insertions and deletions are localized.
#[derive(Debug)]
pub struct GapBuffer {
    /// The underlying storage. Contains [pre-gap content | gap | post-gap content].
    data: Vec<char>,
    /// Index where the gap starts (first unused position).
    gap_start: usize,
    /// Index where the gap ends (first used position after gap).
    gap_end: usize,
}

impl GapBuffer {
    /// Creates a new empty gap buffer.
    pub fn new() -> Self {
        let mut data = Vec::with_capacity(INITIAL_GAP_SIZE);
        data.resize(INITIAL_GAP_SIZE, '\0');
        Self {
            data,
            gap_start: 0,
            gap_end: INITIAL_GAP_SIZE,
        }
    }

    /// Creates a gap buffer initialized with the given text.
    pub fn from_str(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let capacity = len + INITIAL_GAP_SIZE;

        let mut data = Vec::with_capacity(capacity);
        data.extend(chars);
        data.resize(capacity, '\0');

        Self {
            data,
            gap_start: len,
            gap_end: capacity,
        }
    }

    /// Returns the logical length of the buffer (excluding the gap).
    pub fn len(&self) -> usize {
        self.data.len() - self.gap_len()
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the current gap size.
    fn gap_len(&self) -> usize {
        self.gap_end - self.gap_start
    }

    /// Returns the current gap position (cursor position in logical coordinates).
    #[allow(dead_code)]
    pub fn gap_position(&self) -> usize {
        self.gap_start
    }

    /// Moves the gap to the specified logical position.
    ///
    /// This is O(distance) where distance is the absolute difference between
    /// the current gap position and the target position.
    pub fn move_gap_to(&mut self, pos: usize) {
        let pos = pos.min(self.len());

        if pos < self.gap_start {
            // Move gap left: shift content from [pos..gap_start] to [gap_end - shift..gap_end]
            let shift = self.gap_start - pos;
            self.data.copy_within(pos..self.gap_start, self.gap_end - shift);
            self.gap_start = pos;
            self.gap_end -= shift;
        } else if pos > self.gap_start {
            // Move gap right: shift content from [gap_end..gap_end + shift] to [gap_start..]
            let shift = pos - self.gap_start;
            self.data.copy_within(self.gap_end..self.gap_end + shift, self.gap_start);
            self.gap_start += shift;
            self.gap_end += shift;
        }
    }

    /// Ensures the gap is at least the specified size.
    ///
    /// Grows the gap in place so that the gap position is preserved.
    /// This is critical: callers (e.g., `insert`) rely on the gap staying
    /// where `move_gap_to` left it.
    fn ensure_gap(&mut self, min_size: usize) {
        if self.gap_len() >= min_size {
            return;
        }

        // Calculate new gap size
        let current_gap = self.gap_len();
        let needed = min_size - current_gap;
        let growth = needed.max(self.data.len() * GAP_GROWTH_FACTOR);

        // Grow in place: insert `growth` new slots at gap_end,
        // shifting the post-gap content right.
        let old_gap_end = self.gap_end;
        let old_len = self.data.len();
        let post_gap_len = old_len - old_gap_end;

        // Extend the backing store
        let new_size = old_len + growth;
        self.data.resize(new_size, '\0');

        // Shift post-gap content to the end of the new buffer
        // (copy from old position to new position, back-to-front safe via copy_within)
        if post_gap_len > 0 {
            let new_post_gap_start = new_size - post_gap_len;
            self.data.copy_within(old_gap_end..old_len, new_post_gap_start);
        }

        // Update gap_end — gap_start stays the same
        self.gap_end = new_size - post_gap_len;
    }

    /// Inserts a character at the current gap position.
    ///
    /// This is O(1) amortized (may grow the buffer occasionally).
    pub fn insert(&mut self, ch: char) {
        self.ensure_gap(1);
        self.data[self.gap_start] = ch;
        self.gap_start += 1;
    }

    /// Inserts a string at the current gap position.
    #[allow(dead_code)]
    pub fn insert_str(&mut self, s: &str) {
        let chars: Vec<char> = s.chars().collect();
        self.ensure_gap(chars.len());
        for ch in chars {
            self.data[self.gap_start] = ch;
            self.gap_start += 1;
        }
    }

    /// Deletes the character before the gap (backspace).
    ///
    /// Returns the deleted character, or None if at the beginning.
    pub fn delete_backward(&mut self) -> Option<char> {
        if self.gap_start == 0 {
            return None;
        }
        self.gap_start -= 1;
        Some(self.data[self.gap_start])
    }

    /// Deletes the character after the gap (delete key).
    ///
    /// Returns the deleted character, or None if at the end.
    pub fn delete_forward(&mut self) -> Option<char> {
        if self.gap_end >= self.data.len() {
            return None;
        }
        let ch = self.data[self.gap_end];
        self.gap_end += 1;
        Some(ch)
    }

    /// Returns the character at the given logical position.
    pub fn char_at(&self, pos: usize) -> Option<char> {
        if pos >= self.len() {
            return None;
        }
        let physical = if pos < self.gap_start {
            pos
        } else {
            pos + self.gap_len()
        };
        Some(self.data[physical])
    }

    /// Returns an iterator over all characters in the buffer.
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.data[..self.gap_start]
            .iter()
            .chain(self.data[self.gap_end..].iter())
            .copied()
    }

    /// Returns the content of a range as a String.
    ///
    /// The range is in logical coordinates.
    pub fn slice(&self, start: usize, end: usize) -> String {
        let start = start.min(self.len());
        let end = end.min(self.len());
        if start >= end {
            return String::new();
        }

        let mut result = String::with_capacity(end - start);
        for i in start..end {
            if let Some(ch) = self.char_at(i) {
                result.push(ch);
            }
        }
        result
    }
}

impl Default for GapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for GapBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for ch in self.chars() {
            write!(f, "{}", ch)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let buf = GapBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_from_str() {
        let buf = GapBuffer::from_str("hello");
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.to_string(), "hello");
    }

    #[test]
    fn test_insert() {
        let mut buf = GapBuffer::new();
        buf.insert('a');
        buf.insert('b');
        buf.insert('c');
        assert_eq!(buf.to_string(), "abc");
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_insert_at_middle() {
        let mut buf = GapBuffer::from_str("ac");
        buf.move_gap_to(1);
        buf.insert('b');
        assert_eq!(buf.to_string(), "abc");
    }

    #[test]
    fn test_delete_backward() {
        let mut buf = GapBuffer::from_str("abc");
        buf.move_gap_to(3);
        assert_eq!(buf.delete_backward(), Some('c'));
        assert_eq!(buf.to_string(), "ab");
        assert_eq!(buf.delete_backward(), Some('b'));
        assert_eq!(buf.to_string(), "a");
    }

    #[test]
    fn test_delete_backward_at_start() {
        let mut buf = GapBuffer::from_str("abc");
        buf.move_gap_to(0);
        assert_eq!(buf.delete_backward(), None);
        assert_eq!(buf.to_string(), "abc");
    }

    #[test]
    fn test_delete_forward() {
        let mut buf = GapBuffer::from_str("abc");
        buf.move_gap_to(0);
        assert_eq!(buf.delete_forward(), Some('a'));
        assert_eq!(buf.to_string(), "bc");
    }

    #[test]
    fn test_delete_forward_at_end() {
        let mut buf = GapBuffer::from_str("abc");
        buf.move_gap_to(3);
        assert_eq!(buf.delete_forward(), None);
        assert_eq!(buf.to_string(), "abc");
    }

    #[test]
    fn test_move_gap() {
        let mut buf = GapBuffer::from_str("abcdef");
        assert_eq!(buf.gap_position(), 6);

        buf.move_gap_to(3);
        assert_eq!(buf.gap_position(), 3);
        assert_eq!(buf.to_string(), "abcdef");

        buf.move_gap_to(0);
        assert_eq!(buf.gap_position(), 0);
        assert_eq!(buf.to_string(), "abcdef");

        buf.move_gap_to(6);
        assert_eq!(buf.gap_position(), 6);
        assert_eq!(buf.to_string(), "abcdef");
    }

    #[test]
    fn test_char_at() {
        let buf = GapBuffer::from_str("hello");
        assert_eq!(buf.char_at(0), Some('h'));
        assert_eq!(buf.char_at(4), Some('o'));
        assert_eq!(buf.char_at(5), None);
    }

    #[test]
    fn test_char_at_with_gap_in_middle() {
        let mut buf = GapBuffer::from_str("hello");
        buf.move_gap_to(2);
        assert_eq!(buf.char_at(0), Some('h'));
        assert_eq!(buf.char_at(1), Some('e'));
        assert_eq!(buf.char_at(2), Some('l'));
        assert_eq!(buf.char_at(3), Some('l'));
        assert_eq!(buf.char_at(4), Some('o'));
    }

    #[test]
    fn test_slice() {
        let buf = GapBuffer::from_str("hello world");
        assert_eq!(buf.slice(0, 5), "hello");
        assert_eq!(buf.slice(6, 11), "world");
        assert_eq!(buf.slice(0, 11), "hello world");
    }

    #[test]
    fn test_insert_str() {
        let mut buf = GapBuffer::new();
        buf.insert_str("hello");
        assert_eq!(buf.to_string(), "hello");
        buf.insert_str(" world");
        assert_eq!(buf.to_string(), "hello world");
    }

    #[test]
    fn test_large_insert() {
        let mut buf = GapBuffer::new();
        for i in 0..1000 {
            buf.insert(char::from_u32('a' as u32 + (i % 26) as u32).unwrap());
        }
        assert_eq!(buf.len(), 1000);
    }
}
