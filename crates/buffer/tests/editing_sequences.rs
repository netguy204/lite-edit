// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! Integration tests for realistic editing sequences.
//!
//! These tests verify that the gap buffer and line index stay in sync
//! through complex editing patterns.

use lite_edit_buffer::{Position, TextBuffer};

#[test]
fn test_type_word_then_delete_entirely() {
    let mut buf = TextBuffer::new();

    // Type "hello"
    for ch in "hello".chars() {
        buf.insert_char(ch);
    }
    assert_eq!(buf.content(), "hello");
    assert_eq!(buf.cursor_position(), Position::new(0, 5));

    // Delete it entirely with backspace
    for _ in 0..5 {
        buf.delete_backward();
    }
    assert!(buf.is_empty());
    assert_eq!(buf.cursor_position(), Position::new(0, 0));
}

#[test]
fn test_type_multiple_lines_and_navigate() {
    let mut buf = TextBuffer::new();

    // Type three lines
    buf.insert_str("first line");
    buf.insert_newline();
    buf.insert_str("second line");
    buf.insert_newline();
    buf.insert_str("third line");

    assert_eq!(buf.line_count(), 3);
    assert_eq!(buf.line_content(0), "first line");
    assert_eq!(buf.line_content(1), "second line");
    assert_eq!(buf.line_content(2), "third line");

    // Navigate to middle line, middle position
    buf.set_cursor(Position::new(1, 7)); // "second |line"

    // Insert text
    buf.insert_str("awesome ");
    assert_eq!(buf.line_content(1), "second awesome line");

    // Navigate up
    buf.move_up();
    assert_eq!(buf.cursor_position().line, 0);

    // Navigate down twice
    buf.move_down();
    buf.move_down();
    assert_eq!(buf.cursor_position().line, 2);
}

#[test]
fn test_split_and_rejoin_lines() {
    let mut buf = TextBuffer::from_str("helloworld");

    // Split in the middle
    buf.set_cursor(Position::new(0, 5));
    buf.insert_newline();

    assert_eq!(buf.line_count(), 2);
    assert_eq!(buf.line_content(0), "hello");
    assert_eq!(buf.line_content(1), "world");
    assert_eq!(buf.content(), "hello\nworld");

    // Rejoin with backspace
    buf.delete_backward();

    assert_eq!(buf.line_count(), 1);
    assert_eq!(buf.line_content(0), "helloworld");
    assert_eq!(buf.content(), "helloworld");
    assert_eq!(buf.cursor_position(), Position::new(0, 5));
}

#[test]
fn test_rapid_insert_delete_cycles() {
    let mut buf = TextBuffer::new();

    // Simulate typing with corrections
    buf.insert_str("teh"); // typo
    buf.delete_backward();
    buf.delete_backward();
    buf.delete_backward();
    buf.insert_str("the");

    buf.insert_char(' ');

    buf.insert_str("quikc"); // typo
    buf.delete_backward();
    buf.delete_backward();
    buf.insert_str("ck");

    buf.insert_char(' ');

    buf.insert_str("brown fox");

    assert_eq!(buf.content(), "the quick brown fox");
}

#[test]
fn test_delete_forward_sequence() {
    let mut buf = TextBuffer::from_str("abcdefgh");
    buf.set_cursor(Position::new(0, 2)); // After "ab"

    // Delete "cde" using delete forward
    buf.delete_forward();
    buf.delete_forward();
    buf.delete_forward();

    assert_eq!(buf.content(), "abfgh");
    assert_eq!(buf.cursor_position(), Position::new(0, 2));
}

#[test]
fn test_multiline_deletion() {
    let mut buf = TextBuffer::from_str("line1\nline2\nline3\nline4");

    // Delete line2 by positioning at start and deleting
    buf.set_cursor(Position::new(1, 0));

    // Delete entire "line2\n" - first the content
    for _ in 0..5 {
        buf.delete_forward();
    }
    // Then the newline
    buf.delete_forward();

    assert_eq!(buf.line_count(), 3);
    assert_eq!(buf.line_content(0), "line1");
    assert_eq!(buf.line_content(1), "line3");
    assert_eq!(buf.line_content(2), "line4");
}

#[test]
fn test_editing_at_line_boundaries() {
    let mut buf = TextBuffer::from_str("abc\ndef");

    // Position at end of first line
    buf.set_cursor(Position::new(0, 3));

    // Insert at line end
    buf.insert_char('!');
    assert_eq!(buf.line_content(0), "abc!");

    // Move to start of next line
    buf.move_right(); // Skip past newline
    assert_eq!(buf.cursor_position(), Position::new(1, 0));

    // Insert at line start
    buf.insert_char('>');
    assert_eq!(buf.line_content(1), ">def");
}

#[test]
fn test_empty_line_operations() {
    let mut buf = TextBuffer::new();

    // Create content with empty lines
    buf.insert_str("first\n\n\nlast");

    assert_eq!(buf.line_count(), 4);
    assert_eq!(buf.line_content(0), "first");
    assert_eq!(buf.line_content(1), "");
    assert_eq!(buf.line_content(2), "");
    assert_eq!(buf.line_content(3), "last");

    // Navigate through empty lines
    buf.set_cursor(Position::new(1, 0));
    assert_eq!(buf.line_len(1), 0);

    buf.move_down();
    assert_eq!(buf.cursor_position(), Position::new(2, 0));
    assert_eq!(buf.line_len(2), 0);

    // Insert into empty line
    buf.insert_str("middle");
    assert_eq!(buf.line_content(2), "middle");
}

#[test]
fn test_cursor_after_complex_operations() {
    let mut buf = TextBuffer::new();

    // Build up content - note: "Hello World" has space after "Hello"
    buf.insert_str("HelloWorld"); // No space to make test clearer
    assert_eq!(buf.cursor_position(), Position::new(0, 10));

    // Move to middle and insert newline (after "Hello")
    buf.set_cursor(Position::new(0, 5));
    buf.insert_newline();
    assert_eq!(buf.cursor_position(), Position::new(1, 0));

    // Type on new line (before "World")
    buf.insert_str("Beautiful ");
    assert_eq!(buf.cursor_position(), Position::new(1, 10));

    // Verify content
    assert_eq!(buf.line_content(0), "Hello");
    assert_eq!(buf.line_content(1), "Beautiful World");
}

#[test]
fn test_full_buffer_navigation() {
    let mut buf = TextBuffer::from_str("first\nsecond\nthird\nfourth\nfifth");

    // Start at beginning
    assert_eq!(buf.cursor_position(), Position::new(0, 0));

    // Move to buffer end
    buf.move_to_buffer_end();
    assert_eq!(buf.cursor_position(), Position::new(4, 5)); // End of "fifth"

    // Move to buffer start
    buf.move_to_buffer_start();
    assert_eq!(buf.cursor_position(), Position::new(0, 0));

    // Navigate using arrows
    for _ in 0..100 {
        // Move right past end of buffer
        buf.move_right();
    }
    assert_eq!(buf.cursor_position(), Position::new(4, 5)); // Clamped to end

    buf.move_to_buffer_start();
    for _ in 0..100 {
        buf.move_left(); // Move left past start of buffer
    }
    assert_eq!(buf.cursor_position(), Position::new(0, 0)); // Clamped to start
}

#[test]
fn test_line_join_from_multiple_positions() {
    // Join lines via backspace at line start
    let mut buf = TextBuffer::from_str("abc\ndef");
    buf.set_cursor(Position::new(1, 0));
    buf.delete_backward();
    assert_eq!(buf.content(), "abcdef");
    assert_eq!(buf.cursor_position(), Position::new(0, 3));

    // Join lines via delete at line end
    let mut buf = TextBuffer::from_str("abc\ndef");
    buf.set_cursor(Position::new(0, 3));
    buf.delete_forward();
    assert_eq!(buf.content(), "abcdef");
    assert_eq!(buf.cursor_position(), Position::new(0, 3));
}

#[test]
fn test_alternating_insert_movement() {
    let mut buf = TextBuffer::new();

    buf.insert_char('a');
    buf.move_left();
    buf.insert_char('b');
    buf.move_right();
    buf.insert_char('c');
    buf.move_left();
    buf.move_left();
    buf.insert_char('d');

    // Trace: a→ba→bac→bdac
    // 1. insert 'a' at 0 → "a", cursor=1
    // 2. move_left → cursor=0
    // 3. insert 'b' at 0 → "ba", cursor=1
    // 4. move_right → cursor=2 (end of buffer)
    // 5. insert 'c' at 2 → "bac", cursor=3
    // 6. move_left → cursor=2
    // 7. move_left → cursor=1
    // 8. insert 'd' at 1 → "bdac", cursor=2
    assert_eq!(buf.content(), "bdac");
}

#[test]
fn test_inserting_at_various_buffer_positions() {
    let mut buf = TextBuffer::new();

    // Insert at empty buffer
    buf.insert_char('m');
    assert_eq!(buf.content(), "m");

    // Insert at end (most common)
    buf.insert_char('n');
    assert_eq!(buf.content(), "mn");

    // Insert at beginning
    buf.move_to_buffer_start();
    buf.insert_char('l');
    assert_eq!(buf.content(), "lmn");

    // Insert in middle
    buf.set_cursor(Position::new(0, 2));
    buf.insert_char('x');
    assert_eq!(buf.content(), "lmxn");
}

/// Regression test: navigate to a blank line and type.
///
/// Repro of reported bug where typing on an empty line caused text to appear
/// on the line below the cursor.
#[test]
fn test_type_on_blank_line_after_many_edits() {
    // Start with content that has a blank line (like the demo buffer)
    let mut buf = TextBuffer::from_str("}\n\nimpl LiteEdit {");
    // line 0: "}"
    // line 1: "" (blank)
    // line 2: "impl LiteEdit {"

    // Simulate typing several lines at the beginning (like the user did)
    // Cursor starts at (0, 0)
    for ch in "bam boom bam".chars() {
        buf.insert_char(ch);
    }
    buf.insert_char('\n'); // Enter
    buf.insert_char('\n'); // Enter (blank line)
    for ch in "wish this was real".chars() {
        buf.insert_char(ch);
    }
    buf.insert_char('\n'); // Enter
    buf.insert_char('\n'); // Enter (blank line)
    for ch in "maybe too long".chars() {
        buf.insert_char(ch);
    }
    buf.insert_char('\n'); // Enter

    // Now navigate DOWN to the blank line between "}" and "impl LiteEdit {"
    // First, figure out which line that is by searching for it
    let line_count = buf.line_count();
    let mut blank_line = None;
    for i in 0..line_count {
        if buf.line_content(i) == "}" {
            // The blank line should be i+1
            if i + 1 < line_count && buf.line_content(i + 1).is_empty() {
                blank_line = Some(i + 1);
                break;
            }
        }
    }
    let blank_line = blank_line.expect("should find blank line after '}'");

    // Navigate there with move_down (like the user did with arrow keys)
    buf.set_cursor(Position::new(0, 0));
    for _ in 0..blank_line {
        buf.move_down();
    }
    assert_eq!(buf.cursor_position().line, blank_line);
    assert_eq!(buf.cursor_position().col, 0);
    assert_eq!(buf.line_content(blank_line), "");

    // NOW TYPE on the blank line — this is where the bug was reported
    let impl_line = blank_line + 1;
    let impl_content_before = buf.line_content(impl_line);
    assert_eq!(impl_content_before, "impl LiteEdit {");

    for ch in "hello from blank".chars() {
        buf.insert_char(ch);
    }

    // CRITICAL: text must appear on the blank line, NOT on the impl line
    assert_eq!(
        buf.line_content(blank_line),
        "hello from blank",
        "Text should appear on the blank line (line {}), not below it",
        blank_line
    );
    assert_eq!(
        buf.line_content(impl_line),
        "impl LiteEdit {",
        "impl line (line {}) should be unchanged",
        impl_line
    );
    assert_eq!(
        buf.cursor_position(),
        Position::new(blank_line, 16),
        "Cursor should be at end of typed text on the blank line"
    );
}

/// Validates line_index consistency after a complex sequence matching
/// the user's reported editing pattern.
#[test]
fn test_line_index_consistency_after_mixed_edits() {
    let mut buf = TextBuffer::from_str("}\n\nimpl LiteEdit {\n    pub fn new() -> Self {\n    }\n}");

    // Type at the beginning
    for ch in "first line\n\nsecond line\n".chars() {
        buf.insert_char(ch);
    }

    // Navigate to every line and verify content matches a fresh rebuild
    let content = buf.content();
    let fresh = TextBuffer::from_str(&content);

    assert_eq!(buf.line_count(), fresh.line_count(),
        "Line count mismatch after edits");
    for i in 0..buf.line_count() {
        assert_eq!(
            buf.line_content(i),
            fresh.line_content(i),
            "Line {} content mismatch:\n  incremental: {:?}\n  from_str:    {:?}",
            i,
            buf.line_content(i),
            fresh.line_content(i),
        );
    }
}
