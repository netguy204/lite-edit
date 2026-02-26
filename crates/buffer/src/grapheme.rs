// Chunk: docs/chunks/grapheme_cluster_awareness - Grapheme cluster boundary helpers

//! Grapheme cluster boundary detection for proper Unicode text editing.
//!
//! This module provides helper functions for detecting grapheme cluster boundaries
//! in character slices. A grapheme cluster is what users perceive as a single
//! "character" — this includes:
//!
//! - ZWJ emoji sequences: 👨‍👩‍👧‍👦 (4 codepoints + 3 ZWJ = 7 chars)
//! - Combining character sequences: é (e + combining acute = 2 chars)
//! - Regional indicator pairs: 🇺🇸 (2 chars)
//! - Hangul jamo sequences
//!
//! The buffer stores Rust `char` (Unicode scalar values), but cursor movement
//! and deletion should operate on grapheme clusters, not individual chars.

use unicode_segmentation::UnicodeSegmentation;

/// Returns the char offset of the grapheme cluster boundary immediately before `char_offset`.
///
/// If `char_offset` is 0, returns 0.
/// If `char_offset` is at the start of a grapheme, returns the start of the previous grapheme.
/// If `char_offset` is in the middle of a grapheme, returns the start of that grapheme.
///
/// # Arguments
///
/// * `chars` - The character slice representing the line content
/// * `char_offset` - The current char offset (column position)
///
/// # Returns
///
/// The char offset of the grapheme boundary to the left.
pub fn grapheme_boundary_left(chars: &[char], char_offset: usize) -> usize {
    if char_offset == 0 || chars.is_empty() {
        return 0;
    }

    let char_offset = char_offset.min(chars.len());

    // Fast path: if the character before cursor is ASCII, the previous
    // grapheme boundary is simply char_offset - 1.
    // ASCII chars are always single-char graphemes.
    if chars[char_offset - 1].is_ascii() {
        return char_offset - 1;
    }

    // Convert chars to String for grapheme segmentation
    let s: String = chars.iter().collect();

    // Build a list of grapheme start offsets (in chars)
    let mut grapheme_starts: Vec<usize> = Vec::new();
    let mut char_idx = 0;

    for grapheme in s.graphemes(true) {
        grapheme_starts.push(char_idx);
        char_idx += grapheme.chars().count();
    }
    // Add the end position for completeness
    grapheme_starts.push(char_idx);

    // Find the largest grapheme start that is < char_offset
    // This gives us the start of the grapheme containing or immediately before char_offset
    let mut result = 0;
    for &start in &grapheme_starts {
        if start < char_offset {
            result = start;
        } else {
            break;
        }
    }

    result
}

/// Returns the char offset of the grapheme cluster boundary immediately after `char_offset`.
///
/// If `char_offset` is at the start of a grapheme, returns the end of that grapheme.
/// If `char_offset` is in the middle of a grapheme, returns the end of that grapheme.
/// If `char_offset` is >= chars.len(), returns chars.len().
///
/// # Arguments
///
/// * `chars` - The character slice representing the line content
/// * `char_offset` - The current char offset (column position)
///
/// # Returns
///
/// The char offset of the grapheme boundary to the right.
pub fn grapheme_boundary_right(chars: &[char], char_offset: usize) -> usize {
    if chars.is_empty() || char_offset >= chars.len() {
        return chars.len();
    }

    // Fast path: if the character at cursor is ASCII and the next character
    // (if any) is also ASCII, then this is a single-char grapheme.
    let current = chars[char_offset];
    if current.is_ascii() {
        if char_offset + 1 >= chars.len() {
            return char_offset + 1;
        }
        let next = chars[char_offset + 1];
        if next.is_ascii() {
            return char_offset + 1;
        }
        // Fall through to full analysis if next char is non-ASCII
        // (could be a combining mark, though rare with ASCII base)
    }

    // Convert chars to String for grapheme segmentation
    let s: String = chars.iter().collect();

    // Build a list of grapheme end offsets (in chars)
    let mut char_idx = 0;

    for grapheme in s.graphemes(true) {
        let grapheme_len = grapheme.chars().count();
        let grapheme_end = char_idx + grapheme_len;

        // If char_offset is within this grapheme, return its end
        if char_offset < grapheme_end {
            return grapheme_end;
        }

        char_idx = grapheme_end;
    }

    chars.len()
}

/// Returns the number of chars in the grapheme cluster ending at `char_offset`.
///
/// Used by delete_backward to know how many chars to delete.
/// If `char_offset` is 0, returns 0.
/// If `char_offset` is in the middle of a grapheme, returns the chars from
/// the grapheme start to char_offset.
///
/// # Arguments
///
/// * `chars` - The character slice representing the line content
/// * `char_offset` - The current char offset (column position)
///
/// # Returns
///
/// The number of chars in the grapheme cluster before the offset.
pub fn grapheme_len_before(chars: &[char], char_offset: usize) -> usize {
    if char_offset == 0 || chars.is_empty() {
        return 0;
    }

    let char_offset = char_offset.min(chars.len());

    // Fast path: if the character before cursor is ASCII, it's always 1 char
    // ASCII chars cannot be combining marks or part of multi-char graphemes
    if chars[char_offset - 1].is_ascii() {
        return 1;
    }

    let boundary = grapheme_boundary_left(chars, char_offset);
    char_offset - boundary
}

/// Returns the number of chars in the grapheme cluster starting at `char_offset`.
///
/// Used by delete_forward to know how many chars to delete.
/// If `char_offset` >= chars.len(), returns 0.
///
/// # Arguments
///
/// * `chars` - The character slice representing the line content
/// * `char_offset` - The current char offset (column position)
///
/// # Returns
///
/// The number of chars in the grapheme cluster at the offset.
pub fn grapheme_len_at(chars: &[char], char_offset: usize) -> usize {
    if chars.is_empty() || char_offset >= chars.len() {
        return 0;
    }

    // Fast path: if the char at cursor is ASCII, check if next char extends it.
    // ASCII chars are always single-char graphemes unless followed by combining marks.
    let current = chars[char_offset];
    if current.is_ascii() {
        if char_offset + 1 >= chars.len() {
            return 1;
        }
        // If the next character is ASCII or not a combining/extending char,
        // then this ASCII char is a single-char grapheme.
        let next = chars[char_offset + 1];
        if next.is_ascii() {
            return 1;
        }
        // For non-ASCII next char, fall through to full analysis
        // (this handles edge cases like variation selectors after ASCII)
    }

    let boundary = grapheme_boundary_right(chars, char_offset);
    boundary - char_offset
}

/// Returns true if `char_offset` is at a grapheme cluster boundary.
///
/// A position is at a boundary if it's at the start of a grapheme cluster
/// (or at position 0, or at the end of the string).
///
/// # Arguments
///
/// * `chars` - The character slice representing the line content
/// * `char_offset` - The char offset to check
///
/// # Returns
///
/// True if the position is at a grapheme boundary.
pub fn is_grapheme_boundary(chars: &[char], char_offset: usize) -> bool {
    if chars.is_empty() || char_offset == 0 || char_offset >= chars.len() {
        return true;
    }

    // Convert chars to String for grapheme segmentation
    let s: String = chars.iter().collect();

    // Build a set of grapheme start offsets
    let mut char_idx = 0;
    for grapheme in s.graphemes(true) {
        if char_idx == char_offset {
            return true;
        }
        char_idx += grapheme.chars().count();
    }

    // char_offset == chars.len() is also a boundary
    char_offset == chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ASCII Tests ====================

    #[test]
    fn test_ascii_boundary_left() {
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(grapheme_boundary_left(&chars, 0), 0);
        assert_eq!(grapheme_boundary_left(&chars, 1), 0);
        assert_eq!(grapheme_boundary_left(&chars, 3), 2);
        assert_eq!(grapheme_boundary_left(&chars, 5), 4);
    }

    #[test]
    fn test_ascii_boundary_right() {
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(grapheme_boundary_right(&chars, 0), 1);
        assert_eq!(grapheme_boundary_right(&chars, 2), 3);
        assert_eq!(grapheme_boundary_right(&chars, 4), 5);
        assert_eq!(grapheme_boundary_right(&chars, 5), 5);
    }

    #[test]
    fn test_ascii_len_before() {
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(grapheme_len_before(&chars, 0), 0);
        assert_eq!(grapheme_len_before(&chars, 1), 1);
        assert_eq!(grapheme_len_before(&chars, 3), 1);
        assert_eq!(grapheme_len_before(&chars, 5), 1);
    }

    #[test]
    fn test_ascii_len_at() {
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(grapheme_len_at(&chars, 0), 1);
        assert_eq!(grapheme_len_at(&chars, 2), 1);
        assert_eq!(grapheme_len_at(&chars, 4), 1);
        assert_eq!(grapheme_len_at(&chars, 5), 0);
    }

    // ==================== ZWJ Emoji Tests ====================

    #[test]
    fn test_zwj_emoji_boundary_left() {
        // 👨‍👩‍👧‍👦 is 7 chars: U+1F468 U+200D U+1F469 U+200D U+1F467 U+200D U+1F466
        let chars: Vec<char> = "a👨‍👩‍👧‍👦b".chars().collect();
        // "a" = 1 char, emoji = 7 chars, "b" = 1 char, total = 9 chars
        assert_eq!(chars.len(), 9);

        // At start of emoji (col 1), boundary_left should go to 0 (before 'a')
        assert_eq!(grapheme_boundary_left(&chars, 1), 0);

        // At end of emoji (col 8), boundary_left should go to 1 (start of emoji)
        assert_eq!(grapheme_boundary_left(&chars, 8), 1);

        // At 'b' (col 9), boundary_left should go to 8 (end of emoji, which is start of 'b')
        assert_eq!(grapheme_boundary_left(&chars, 9), 8);
    }

    #[test]
    fn test_zwj_emoji_boundary_right() {
        let chars: Vec<char> = "a👨‍👩‍👧‍👦b".chars().collect();

        // At 'a' (col 0), should move to end of 'a' (col 1)
        assert_eq!(grapheme_boundary_right(&chars, 0), 1);

        // At start of emoji (col 1), should move to end of emoji (col 8)
        assert_eq!(grapheme_boundary_right(&chars, 1), 8);

        // At 'b' (col 8), should move to end (col 9)
        assert_eq!(grapheme_boundary_right(&chars, 8), 9);
    }

    #[test]
    fn test_zwj_emoji_len_before() {
        let chars: Vec<char> = "a👨‍👩‍👧‍👦b".chars().collect();

        // At end of emoji (col 8), should return 7 (length of emoji)
        assert_eq!(grapheme_len_before(&chars, 8), 7);

        // At 'b' end (col 9), should return 1
        assert_eq!(grapheme_len_before(&chars, 9), 1);
    }

    #[test]
    fn test_zwj_emoji_len_at() {
        let chars: Vec<char> = "a👨‍👩‍👧‍👦b".chars().collect();

        // At start of emoji (col 1), should return 7
        assert_eq!(grapheme_len_at(&chars, 1), 7);

        // At 'b' (col 8), should return 1
        assert_eq!(grapheme_len_at(&chars, 8), 1);
    }

    // ==================== Combining Character Tests ====================

    #[test]
    fn test_combining_char_boundary_left() {
        // "é" as e + combining acute = 2 chars: U+0065 U+0301
        let chars: Vec<char> = "ae\u{0301}b".chars().collect();
        // "a" = 1, "é" = 2, "b" = 1, total = 4 chars
        assert_eq!(chars.len(), 4);

        // At start of "é" (col 1), boundary_left should go to 0
        assert_eq!(grapheme_boundary_left(&chars, 1), 0);

        // At end of "é" (col 3), boundary_left should go to 1 (start of "é")
        assert_eq!(grapheme_boundary_left(&chars, 3), 1);

        // At 'b' end (col 4), boundary_left should go to 3
        assert_eq!(grapheme_boundary_left(&chars, 4), 3);
    }

    #[test]
    fn test_combining_char_boundary_right() {
        let chars: Vec<char> = "ae\u{0301}b".chars().collect();

        // At 'a' (col 0), should move to 1
        assert_eq!(grapheme_boundary_right(&chars, 0), 1);

        // At start of "é" (col 1), should move to 3 (end of é)
        assert_eq!(grapheme_boundary_right(&chars, 1), 3);

        // At 'b' (col 3), should move to 4
        assert_eq!(grapheme_boundary_right(&chars, 3), 4);
    }

    #[test]
    fn test_combining_char_len_before() {
        let chars: Vec<char> = "ae\u{0301}b".chars().collect();

        // At end of "é" (col 3), should return 2
        assert_eq!(grapheme_len_before(&chars, 3), 2);
    }

    #[test]
    fn test_combining_char_len_at() {
        let chars: Vec<char> = "ae\u{0301}b".chars().collect();

        // At start of "é" (col 1), should return 2
        assert_eq!(grapheme_len_at(&chars, 1), 2);
    }

    // ==================== Regional Indicator Tests ====================

    #[test]
    fn test_regional_indicator_boundary_left() {
        // 🇺🇸 is 2 chars: U+1F1FA U+1F1F8
        let chars: Vec<char> = "a🇺🇸b".chars().collect();
        // "a" = 1, flag = 2, "b" = 1, total = 4 chars
        assert_eq!(chars.len(), 4);

        // At end of flag (col 3), boundary_left should go to 1 (start of flag)
        assert_eq!(grapheme_boundary_left(&chars, 3), 1);
    }

    #[test]
    fn test_regional_indicator_boundary_right() {
        let chars: Vec<char> = "a🇺🇸b".chars().collect();

        // At start of flag (col 1), should move to 3 (end of flag)
        assert_eq!(grapheme_boundary_right(&chars, 1), 3);
    }

    #[test]
    fn test_regional_indicator_len_before() {
        let chars: Vec<char> = "a🇺🇸b".chars().collect();

        // At end of flag (col 3), should return 2
        assert_eq!(grapheme_len_before(&chars, 3), 2);
    }

    #[test]
    fn test_regional_indicator_len_at() {
        let chars: Vec<char> = "a🇺🇸b".chars().collect();

        // At start of flag (col 1), should return 2
        assert_eq!(grapheme_len_at(&chars, 1), 2);
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_empty_slice() {
        let chars: Vec<char> = Vec::new();
        assert_eq!(grapheme_boundary_left(&chars, 0), 0);
        assert_eq!(grapheme_boundary_left(&chars, 5), 0);
        assert_eq!(grapheme_boundary_right(&chars, 0), 0);
        assert_eq!(grapheme_len_before(&chars, 0), 0);
        assert_eq!(grapheme_len_at(&chars, 0), 0);
    }

    #[test]
    fn test_single_char() {
        let chars: Vec<char> = "x".chars().collect();
        assert_eq!(grapheme_boundary_left(&chars, 0), 0);
        assert_eq!(grapheme_boundary_left(&chars, 1), 0);
        assert_eq!(grapheme_boundary_right(&chars, 0), 1);
        assert_eq!(grapheme_boundary_right(&chars, 1), 1);
        assert_eq!(grapheme_len_before(&chars, 1), 1);
        assert_eq!(grapheme_len_at(&chars, 0), 1);
    }

    #[test]
    fn test_only_emoji() {
        let chars: Vec<char> = "👨‍👩‍👧‍👦".chars().collect();
        assert_eq!(chars.len(), 7);

        assert_eq!(grapheme_boundary_left(&chars, 0), 0);
        assert_eq!(grapheme_boundary_left(&chars, 7), 0);
        assert_eq!(grapheme_boundary_right(&chars, 0), 7);
        assert_eq!(grapheme_len_before(&chars, 7), 7);
        assert_eq!(grapheme_len_at(&chars, 0), 7);
    }

    #[test]
    fn test_offset_beyond_length() {
        let chars: Vec<char> = "abc".chars().collect();
        assert_eq!(grapheme_boundary_left(&chars, 10), 2); // clamped to len, then find left
        assert_eq!(grapheme_boundary_right(&chars, 10), 3);
        assert_eq!(grapheme_len_before(&chars, 10), 1);
        assert_eq!(grapheme_len_at(&chars, 10), 0);
    }
}
