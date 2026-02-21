// Chunk: docs/chunks/text_buffer - Text buffer data structure with gap buffer backing

//! Performance sanity checks for the text buffer.
//!
//! These tests verify that basic operations complete within reasonable time bounds.
//! They are not formal benchmarks but guard against obvious performance regressions.

use lite_edit_buffer::TextBuffer;
use std::time::{Duration, Instant};

#[test]
fn insert_100k_chars_under_100ms() {
    let mut buffer = TextBuffer::new();
    let start = Instant::now();

    for _ in 0..100_000 {
        buffer.insert_char('x');
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "Inserting 100K characters took {:?}, expected < 100ms",
        elapsed
    );

    assert_eq!(buffer.len(), 100_000);
    assert_eq!(buffer.line_count(), 1);
}

#[test]
fn insert_100k_chars_with_newlines_under_200ms() {
    let mut buffer = TextBuffer::new();
    let start = Instant::now();

    for i in 0..100_000 {
        if i % 80 == 79 {
            buffer.insert_newline();
        } else {
            buffer.insert_char('x');
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(200),
        "Inserting 100K characters with newlines took {:?}, expected < 200ms",
        elapsed
    );

    // Should have roughly 100000/80 = 1250 lines
    assert!(buffer.line_count() > 1000);
}

#[test]
fn rapid_cursor_movement() {
    let mut buffer = TextBuffer::from_str(&"x".repeat(10_000));
    let start = Instant::now();

    // Move cursor back and forth many times
    for _ in 0..1000 {
        buffer.move_to_buffer_end();
        buffer.move_to_buffer_start();
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "Rapid cursor movement took {:?}, expected < 50ms",
        elapsed
    );
}

#[test]
fn line_access_performance() {
    // Create a buffer with many lines
    let content: String = (0..1000)
        .map(|i| format!("Line number {}", i))
        .collect::<Vec<_>>()
        .join("\n");

    let buffer = TextBuffer::from_str(&content);
    let start = Instant::now();

    // Access each line many times
    for _ in 0..100 {
        for line in 0..buffer.line_count() {
            let _ = buffer.line_content(line);
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(200),
        "Accessing {} lines 100 times took {:?}, expected < 200ms",
        buffer.line_count(),
        elapsed
    );
}

#[test]
fn delete_all_chars_performance() {
    let mut buffer = TextBuffer::new();

    // Insert 10K characters
    for _ in 0..10_000 {
        buffer.insert_char('x');
    }

    let start = Instant::now();

    // Delete all characters via backspace
    while !buffer.is_empty() {
        buffer.delete_backward();
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "Deleting 10K characters took {:?}, expected < 50ms",
        elapsed
    );

    assert!(buffer.is_empty());
}

#[test]
fn mixed_operations_performance() {
    let mut buffer = TextBuffer::new();
    let start = Instant::now();

    // Simulate realistic editing: type, correct, move, type more
    for iteration in 0..1000 {
        // Type some text
        for ch in format!("Line {}: ", iteration).chars() {
            buffer.insert_char(ch);
        }

        // Make a typo and correct it
        buffer.insert_char('x');
        buffer.delete_backward();

        // Type more
        for ch in "some content here".chars() {
            buffer.insert_char(ch);
        }

        // New line
        buffer.insert_newline();
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "Mixed operations took {:?}, expected < 100ms",
        elapsed
    );

    assert_eq!(buffer.line_count(), 1001); // 1000 lines + 1 empty line at end
}
