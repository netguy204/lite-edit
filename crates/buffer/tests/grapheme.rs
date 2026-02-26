// Chunk: docs/chunks/grapheme_cluster_awareness - Grapheme cluster awareness integration tests

//! Integration tests for grapheme-aware editing operations.
//!
//! These tests verify that cursor movement and deletion operations
//! respect grapheme cluster boundaries for proper Unicode text editing.

use lite_edit_buffer::{Position, TextBuffer};

// ==================== Backspace Tests ====================

#[test]
fn test_backspace_deletes_zwj_emoji_entirely() {
    // 👨‍👩‍👧‍👦 is 7 chars but should be deleted as one grapheme
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦b");

    // Move cursor to after the emoji (position 8: 'a' + 7 emoji chars)
    buf.set_cursor(Position::new(0, 8));

    // Backspace should delete the entire emoji, not just one char
    buf.delete_backward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_backspace_deletes_combining_character_sequence() {
    // "é" as e + combining acute = 2 chars, should delete as one grapheme
    let mut buf = TextBuffer::from_str("ae\u{0301}b");
    // Content is: a, e, combining_acute, b = 4 chars

    // Move cursor to after the é (position 3)
    buf.set_cursor(Position::new(0, 3));

    // Backspace should delete both 'e' and the combining acute
    buf.delete_backward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_backspace_deletes_regional_indicator_pair() {
    // 🇺🇸 is 2 chars (U+1F1FA U+1F1F8), should delete as one grapheme
    let mut buf = TextBuffer::from_str("a🇺🇸b");
    // Content is: a, flag_u, flag_s, b = 4 chars

    // Move cursor to after the flag (position 3)
    buf.set_cursor(Position::new(0, 3));

    // Backspace should delete both regional indicator chars
    buf.delete_backward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_backspace_ascii_unchanged() {
    let mut buf = TextBuffer::from_str("abc");
    buf.set_cursor(Position::new(0, 3));

    buf.delete_backward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 2));
}

#[test]
fn test_backspace_at_start_joins_lines() {
    // Verify newline joining still works (grapheme logic only applies within lines)
    let mut buf = TextBuffer::from_str("hello\nworld");
    buf.set_cursor(Position::new(1, 0));

    buf.delete_backward();

    assert_eq!(buf.content(), "helloworld");
    assert_eq!(buf.cursor_position(), Position::new(0, 5));
}

// ==================== Delete Forward Tests ====================

#[test]
fn test_delete_forward_removes_zwj_emoji_entirely() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦b");

    // Move cursor to start of emoji (position 1)
    buf.set_cursor(Position::new(0, 1));

    // Delete should remove the entire emoji
    buf.delete_forward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_delete_forward_removes_combining_sequence() {
    let mut buf = TextBuffer::from_str("ae\u{0301}b");

    // Move cursor to start of é (position 1)
    buf.set_cursor(Position::new(0, 1));

    // Delete should remove both 'e' and the combining accent
    buf.delete_forward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_delete_forward_removes_regional_indicator_pair() {
    let mut buf = TextBuffer::from_str("a🇺🇸b");

    // Move cursor to start of flag (position 1)
    buf.set_cursor(Position::new(0, 1));

    // Delete should remove both regional indicator chars
    buf.delete_forward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_delete_forward_ascii_unchanged() {
    let mut buf = TextBuffer::from_str("abc");
    buf.set_cursor(Position::new(0, 0));

    buf.delete_forward();

    assert_eq!(buf.content(), "bc");
    assert_eq!(buf.cursor_position(), Position::new(0, 0));
}

#[test]
fn test_delete_forward_at_end_joins_lines() {
    let mut buf = TextBuffer::from_str("hello\nworld");
    buf.set_cursor(Position::new(0, 5));

    buf.delete_forward();

    assert_eq!(buf.content(), "helloworld");
    assert_eq!(buf.cursor_position(), Position::new(0, 5));
}

// ==================== Cursor Movement Tests ====================

#[test]
fn test_move_right_past_zwj_emoji() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦b");
    buf.set_cursor(Position::new(0, 1)); // At start of emoji

    buf.move_right();

    // Should move past entire emoji (7 chars) to position 8
    assert_eq!(buf.cursor_position(), Position::new(0, 8));
}

#[test]
fn test_move_left_past_zwj_emoji() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦b");
    buf.set_cursor(Position::new(0, 8)); // At end of emoji

    buf.move_left();

    // Should move to start of emoji (position 1)
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_right_past_combining_char() {
    let mut buf = TextBuffer::from_str("ae\u{0301}b");
    buf.set_cursor(Position::new(0, 1)); // At start of é

    buf.move_right();

    // Should move past both 'e' and combining accent to position 3
    assert_eq!(buf.cursor_position(), Position::new(0, 3));
}

#[test]
fn test_move_left_past_combining_char() {
    let mut buf = TextBuffer::from_str("ae\u{0301}b");
    buf.set_cursor(Position::new(0, 3)); // After é

    buf.move_left();

    // Should move to start of é (position 1)
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_right_past_regional_indicator() {
    let mut buf = TextBuffer::from_str("a🇺🇸b");
    buf.set_cursor(Position::new(0, 1)); // At start of flag

    buf.move_right();

    // Should move past both chars to position 3
    assert_eq!(buf.cursor_position(), Position::new(0, 3));
}

#[test]
fn test_move_left_past_regional_indicator() {
    let mut buf = TextBuffer::from_str("a🇺🇸b");
    buf.set_cursor(Position::new(0, 3)); // After flag

    buf.move_left();

    // Should move to start of flag (position 1)
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_right_ascii_unchanged() {
    let mut buf = TextBuffer::from_str("abc");
    buf.set_cursor(Position::new(0, 0));

    buf.move_right();

    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_left_ascii_unchanged() {
    let mut buf = TextBuffer::from_str("abc");
    buf.set_cursor(Position::new(0, 2));

    buf.move_left();

    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_right_at_line_end_goes_to_next_line() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦\nb");
    buf.set_cursor(Position::new(0, 8)); // At end of first line

    buf.move_right();

    // Should move to start of next line
    assert_eq!(buf.cursor_position(), Position::new(1, 0));
}

#[test]
fn test_move_left_at_line_start_goes_to_prev_line() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦\nb");
    buf.set_cursor(Position::new(1, 0)); // At start of second line

    buf.move_left();

    // Should move to end of previous line
    assert_eq!(buf.cursor_position(), Position::new(0, 8));
}

// ==================== Selection Tests ====================

#[test]
fn test_anchor_and_move_right_selects_zwj_emoji() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦b");
    buf.set_cursor(Position::new(0, 1)); // At start of emoji

    // Set anchor at current position, then move right
    buf.set_selection_anchor_at_cursor();
    buf.move_cursor_preserving_selection(Position::new(0, 8)); // End of emoji

    // Selection should span the entire emoji
    let (start, end) = buf.selection_range().expect("should have selection");
    assert_eq!(start, Position::new(0, 1));
    assert_eq!(end, Position::new(0, 8));
}

#[test]
fn test_anchor_and_move_left_selects_zwj_emoji() {
    let mut buf = TextBuffer::from_str("a👨‍👩‍👧‍👦b");
    buf.set_cursor(Position::new(0, 8)); // At end of emoji

    // Set anchor at current position, then move left
    buf.set_selection_anchor_at_cursor();
    buf.move_cursor_preserving_selection(Position::new(0, 1)); // Start of emoji

    // Selection should span the entire emoji
    let (start, end) = buf.selection_range().expect("should have selection");
    assert_eq!(start, Position::new(0, 1));
    assert_eq!(end, Position::new(0, 8));
}

#[test]
fn test_anchor_and_move_right_selects_combining_char() {
    let mut buf = TextBuffer::from_str("ae\u{0301}b");
    buf.set_cursor(Position::new(0, 1)); // At start of é

    buf.set_selection_anchor_at_cursor();
    buf.move_cursor_preserving_selection(Position::new(0, 3)); // End of é

    let (start, end) = buf.selection_range().expect("should have selection");
    assert_eq!(start, Position::new(0, 1));
    assert_eq!(end, Position::new(0, 3));
}

#[test]
fn test_anchor_and_move_left_selects_regional_indicator() {
    let mut buf = TextBuffer::from_str("a🇺🇸b");
    buf.set_cursor(Position::new(0, 3)); // After flag

    buf.set_selection_anchor_at_cursor();
    buf.move_cursor_preserving_selection(Position::new(0, 1)); // Start of flag

    let (start, end) = buf.selection_range().expect("should have selection");
    assert_eq!(start, Position::new(0, 1));
    assert_eq!(end, Position::new(0, 3));
}

// ==================== Word Selection Tests ====================

#[test]
fn test_select_word_with_combining_chars() {
    // Word "café" with combining accent on the e
    let mut buf = TextBuffer::from_str("hello cafe\u{0301} world");
    // chars: h e l l o   c a f e ́   w o r l d
    //        0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16

    // Position cursor on line 0, then select word at col 8 (on the 'f')
    buf.set_cursor(Position::new(0, 8));
    buf.select_word_at(8);

    // Word selection should include the entire word including the combining char
    let (start, end) = buf.selection_range().expect("should have selection");
    assert_eq!(start, Position::new(0, 6));  // 'c' position
    assert_eq!(end, Position::new(0, 11));   // After combining char
}

#[test]
fn test_double_click_selection_respects_grapheme_boundary() {
    // A word containing emoji as part of its "text" shouldn't split graphemes
    // Here we test that word boundaries don't land inside a grapheme
    let mut buf = TextBuffer::from_str("hello 👨‍👩‍👧‍👦 world");

    // Position cursor on line 0, then select word at col 6 (start of emoji)
    buf.set_cursor(Position::new(0, 6));
    buf.select_word_at(6);

    // Selection should be the entire emoji, not partial
    let (start, end) = buf.selection_range().expect("should have selection");
    assert_eq!(start, Position::new(0, 6));   // Start of emoji
    assert_eq!(end, Position::new(0, 13));    // End of emoji (6 + 7 = 13)
}

// ==================== Hangul Jamo Tests ====================

#[test]
fn test_backspace_deletes_hangul_syllable() {
    // 한 (U+D55C) is a precomposed Hangul syllable (1 char)
    let mut buf = TextBuffer::from_str("a한b");
    buf.set_cursor(Position::new(0, 2));

    buf.delete_backward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_right_past_hangul_syllable() {
    let mut buf = TextBuffer::from_str("a한b");
    buf.set_cursor(Position::new(0, 1));

    buf.move_right();

    assert_eq!(buf.cursor_position(), Position::new(0, 2));
}

#[test]
fn test_hangul_jamo_sequence_decomposed() {
    // Decomposed Hangul: ᄒ (U+1112) + ᅡ (U+1161) + ᆫ (U+11AB) = 한
    // This is 3 chars but represents one grapheme
    let mut buf = TextBuffer::from_str("a\u{1112}\u{1161}\u{11AB}b");
    // Content should be: a, jamo1, jamo2, jamo3, b = 5 chars

    buf.set_cursor(Position::new(0, 4)); // After the decomposed Hangul

    // Backspace should delete all 3 jamo chars as one grapheme
    buf.delete_backward();

    assert_eq!(buf.content(), "ab");
    assert_eq!(buf.cursor_position(), Position::new(0, 1));
}

#[test]
fn test_move_right_past_decomposed_hangul() {
    let mut buf = TextBuffer::from_str("a\u{1112}\u{1161}\u{11AB}b");
    buf.set_cursor(Position::new(0, 1)); // At start of jamo sequence

    buf.move_right();

    // Should move past all 3 jamo chars to position 4
    assert_eq!(buf.cursor_position(), Position::new(0, 4));
}

// ==================== Multiple Grapheme Tests ====================

#[test]
fn test_delete_multiple_emojis_sequentially() {
    let mut buf = TextBuffer::from_str("👍👎");
    // 👍 = 1 char (U+1F44D), 👎 = 1 char (U+1F44E)
    buf.set_cursor(Position::new(0, 2));

    // Delete second emoji
    buf.delete_backward();
    assert_eq!(buf.content(), "👍");

    // Delete first emoji
    buf.delete_backward();
    assert_eq!(buf.content(), "");
}

#[test]
fn test_navigate_through_mixed_content() {
    // Mix of ASCII, emoji, combining chars
    let mut buf = TextBuffer::from_str("hi👋bye\u{0301}");
    // h i 👋 b y e ́  (combining acute on 'e')
    // 0 1 2  3 4 5 6

    buf.set_cursor(Position::new(0, 0));

    // Move right through content
    buf.move_right(); // h -> i
    assert_eq!(buf.cursor_position().col, 1);

    buf.move_right(); // i -> 👋
    assert_eq!(buf.cursor_position().col, 2);

    buf.move_right(); // 👋 -> b
    assert_eq!(buf.cursor_position().col, 3);

    buf.move_right(); // b -> y
    assert_eq!(buf.cursor_position().col, 4);

    buf.move_right(); // y -> é (e + combining)
    assert_eq!(buf.cursor_position().col, 5);

    buf.move_right(); // é -> end (past combining char)
    assert_eq!(buf.cursor_position().col, 7);
}
