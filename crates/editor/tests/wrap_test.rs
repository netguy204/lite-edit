// Chunk: docs/chunks/line_wrap_rendering - Integration tests for soft line wrapping
//!
//! Integration tests for soft line wrapping functionality.
//!
//! These tests verify the logic of line wrapping calculations that match
//! the WrapLayout implementation. Since WrapLayout is internal to the editor
//! crate, these tests verify the expected behavior through documentation
//! and equivalent calculations.
//!
//! The actual WrapLayout struct is unit-tested in wrap_layout.rs.

use lite_edit_buffer::TextBuffer;

// =============================================================================
// Buffer Tests for Wrapping Scenarios
// =============================================================================

#[test]
fn test_buffer_line_lengths_for_wrapping() {
    let content = [
        "a".repeat(50),   // 50 chars
        "b".repeat(150),  // 150 chars
        "c".repeat(250),  // 250 chars
        "d".repeat(10),   // 10 chars
    ]
    .join("\n");

    let buffer = TextBuffer::from_str(&content);

    assert_eq!(buffer.line_count(), 4);
    assert_eq!(buffer.line_len(0), 50);
    assert_eq!(buffer.line_len(1), 150);
    assert_eq!(buffer.line_len(2), 250);
    assert_eq!(buffer.line_len(3), 10);
}

#[test]
fn test_cursor_positioning_at_various_columns() {
    // Create a buffer with a long line
    let long_line = "x".repeat(200);
    let mut buffer = TextBuffer::from_str(&long_line);

    // Cursor can be positioned at any column up to line length
    buffer.set_cursor(lite_edit_buffer::Position::new(0, 0));
    assert_eq!(buffer.cursor_position().col, 0);

    buffer.set_cursor(lite_edit_buffer::Position::new(0, 100));
    assert_eq!(buffer.cursor_position().col, 100);

    buffer.set_cursor(lite_edit_buffer::Position::new(0, 199));
    assert_eq!(buffer.cursor_position().col, 199);

    // Setting cursor past line end clamps to line length
    buffer.set_cursor(lite_edit_buffer::Position::new(0, 500));
    assert_eq!(buffer.cursor_position().col, 200);
}

#[test]
fn test_selection_on_long_line() {
    // Selection can span many columns on a single buffer line
    let long_line = "x".repeat(200);
    let mut buffer = TextBuffer::from_str(&long_line);

    // Set cursor at col 50
    buffer.set_cursor(lite_edit_buffer::Position::new(0, 50));

    // Set anchor at col 150
    buffer.set_selection_anchor(lite_edit_buffer::Position::new(0, 150));

    // Selection should span from 50 to 150
    let range = buffer.selection_range().unwrap();
    assert_eq!(range.0.col, 50);
    assert_eq!(range.1.col, 150);
}

#[test]
fn test_empty_line_behavior() {
    let mut buffer = TextBuffer::from_str("first\n\nthird");

    // Empty line (line 1) has length 0
    assert_eq!(buffer.line_len(1), 0);

    // Can position cursor on empty line
    buffer.set_cursor(lite_edit_buffer::Position::new(1, 0));
    assert_eq!(buffer.cursor_position().line, 1);
    assert_eq!(buffer.cursor_position().col, 0);
}

// =============================================================================
// Wrap Layout Calculation Verification Tests
// =============================================================================

/// These tests verify the expected wrapping calculations that match WrapLayout.
/// They don't import WrapLayout directly but test the same arithmetic.

/// Calculates screen rows for a line (matching WrapLayout::screen_rows_for_line)
fn screen_rows_for_line(char_count: usize, cols_per_row: usize) -> usize {
    if char_count == 0 {
        1
    } else {
        (char_count + cols_per_row - 1) / cols_per_row
    }
}

/// Converts buffer column to screen position (matching WrapLayout::buffer_col_to_screen_pos)
fn buffer_col_to_screen_pos(buf_col: usize, cols_per_row: usize) -> (usize, usize) {
    let row_offset = buf_col / cols_per_row;
    let screen_col = buf_col % cols_per_row;
    (row_offset, screen_col)
}

/// Converts screen position to buffer column (matching WrapLayout::screen_pos_to_buffer_col)
fn screen_pos_to_buffer_col(row_offset: usize, screen_col: usize, cols_per_row: usize) -> usize {
    row_offset * cols_per_row + screen_col
}

#[test]
fn test_screen_rows_calculation() {
    let cols_per_row = 100;

    // Empty line = 1 row
    assert_eq!(screen_rows_for_line(0, cols_per_row), 1);

    // Lines that fit = 1 row
    assert_eq!(screen_rows_for_line(50, cols_per_row), 1);
    assert_eq!(screen_rows_for_line(100, cols_per_row), 1);

    // Lines that wrap
    assert_eq!(screen_rows_for_line(101, cols_per_row), 2);
    assert_eq!(screen_rows_for_line(200, cols_per_row), 2);
    assert_eq!(screen_rows_for_line(201, cols_per_row), 3);
    assert_eq!(screen_rows_for_line(300, cols_per_row), 3);
    assert_eq!(screen_rows_for_line(350, cols_per_row), 4);
}

#[test]
fn test_buffer_col_to_screen_pos_calculation() {
    let cols_per_row = 100;

    // First row
    assert_eq!(buffer_col_to_screen_pos(0, cols_per_row), (0, 0));
    assert_eq!(buffer_col_to_screen_pos(50, cols_per_row), (0, 50));
    assert_eq!(buffer_col_to_screen_pos(99, cols_per_row), (0, 99));

    // Second row (wrapped)
    assert_eq!(buffer_col_to_screen_pos(100, cols_per_row), (1, 0));
    assert_eq!(buffer_col_to_screen_pos(150, cols_per_row), (1, 50));
    assert_eq!(buffer_col_to_screen_pos(199, cols_per_row), (1, 99));

    // Third row
    assert_eq!(buffer_col_to_screen_pos(200, cols_per_row), (2, 0));
    assert_eq!(buffer_col_to_screen_pos(250, cols_per_row), (2, 50));
}

#[test]
fn test_screen_pos_to_buffer_col_calculation() {
    let cols_per_row = 100;

    // First row
    assert_eq!(screen_pos_to_buffer_col(0, 0, cols_per_row), 0);
    assert_eq!(screen_pos_to_buffer_col(0, 50, cols_per_row), 50);
    assert_eq!(screen_pos_to_buffer_col(0, 99, cols_per_row), 99);

    // Second row
    assert_eq!(screen_pos_to_buffer_col(1, 0, cols_per_row), 100);
    assert_eq!(screen_pos_to_buffer_col(1, 50, cols_per_row), 150);
    assert_eq!(screen_pos_to_buffer_col(1, 99, cols_per_row), 199);

    // Third row
    assert_eq!(screen_pos_to_buffer_col(2, 0, cols_per_row), 200);
}

#[test]
fn test_round_trip_conversion() {
    let cols_per_row = 100;

    for buf_col in [0, 1, 50, 99, 100, 101, 150, 199, 200, 500, 999, 1000] {
        let (row_off, screen_col) = buffer_col_to_screen_pos(buf_col, cols_per_row);
        let round_trip = screen_pos_to_buffer_col(row_off, screen_col, cols_per_row);
        assert_eq!(
            round_trip, buf_col,
            "Round trip failed for buf_col={buf_col}"
        );
    }
}

#[test]
fn test_narrow_viewport_wrapping() {
    // 10 columns per row = lots of wrapping
    let cols_per_row = 10;

    // 25 chars wraps to 3 rows
    assert_eq!(screen_rows_for_line(25, cols_per_row), 3);

    // Position 15 is row 1, col 5
    assert_eq!(buffer_col_to_screen_pos(15, cols_per_row), (1, 5));
}

#[test]
fn test_very_long_line_wrapping() {
    let cols_per_row = 100;

    // 1000 chars = 10 rows
    assert_eq!(screen_rows_for_line(1000, cols_per_row), 10);

    // Position 999 = row 9, col 99
    assert_eq!(buffer_col_to_screen_pos(999, cols_per_row), (9, 99));
}

// =============================================================================
// Total Screen Rows Calculation
// =============================================================================

#[test]
fn test_total_screen_rows_for_buffer() {
    let content = [
        "a".repeat(50),   // 50 chars - 1 screen row
        "b".repeat(150),  // 150 chars - 2 screen rows
        "c".repeat(250),  // 250 chars - 3 screen rows
        "d".repeat(10),   // 10 chars - 1 screen row
    ]
    .join("\n");

    let buffer = TextBuffer::from_str(&content);
    let cols_per_row = 100;

    // Calculate total screen rows
    let total_rows: usize = (0..buffer.line_count())
        .map(|i| screen_rows_for_line(buffer.line_len(i), cols_per_row))
        .sum();

    assert_eq!(total_rows, 1 + 2 + 3 + 1);
}

// =============================================================================
// Hit-Testing Simulation
// =============================================================================

/// Simulates the hit-testing algorithm from pixel_to_buffer_position_wrapped
fn hit_test_line(
    target_screen_row: usize,
    line_lengths: &[usize],
    cols_per_row: usize,
) -> Option<(usize, usize)> {
    // (buffer_line, row_offset_in_line)
    let mut cumulative_screen_row: usize = 0;

    for (buffer_line, &line_len) in line_lengths.iter().enumerate() {
        let rows_for_line = screen_rows_for_line(line_len, cols_per_row);
        let next_cumulative = cumulative_screen_row + rows_for_line;

        if target_screen_row < next_cumulative {
            let row_offset_in_line = target_screen_row - cumulative_screen_row;
            return Some((buffer_line, row_offset_in_line));
        }

        cumulative_screen_row = next_cumulative;
    }

    None
}

#[test]
fn test_hit_test_simulation() {
    let line_lengths = vec![50, 150, 250, 10]; // 1 + 2 + 3 + 1 = 7 screen rows
    let cols_per_row = 100;

    // Click on screen row 0 -> buffer line 0, row offset 0
    assert_eq!(hit_test_line(0, &line_lengths, cols_per_row), Some((0, 0)));

    // Click on screen row 1 -> buffer line 1, row offset 0
    assert_eq!(hit_test_line(1, &line_lengths, cols_per_row), Some((1, 0)));

    // Click on screen row 2 -> buffer line 1, row offset 1 (continuation)
    assert_eq!(hit_test_line(2, &line_lengths, cols_per_row), Some((1, 1)));

    // Click on screen row 3 -> buffer line 2, row offset 0
    assert_eq!(hit_test_line(3, &line_lengths, cols_per_row), Some((2, 0)));

    // Click on screen row 5 -> buffer line 2, row offset 2 (2nd continuation)
    assert_eq!(hit_test_line(5, &line_lengths, cols_per_row), Some((2, 2)));

    // Click on screen row 6 -> buffer line 3, row offset 0
    assert_eq!(hit_test_line(6, &line_lengths, cols_per_row), Some((3, 0)));

    // Click on screen row 7 -> beyond content
    assert_eq!(hit_test_line(7, &line_lengths, cols_per_row), None);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_single_column_viewport() {
    // Edge case: 1 column per row
    let cols_per_row = 1;

    // Every char is its own row
    assert_eq!(screen_rows_for_line(5, cols_per_row), 5);

    // Position 3 = row 3, col 0
    assert_eq!(buffer_col_to_screen_pos(3, cols_per_row), (3, 0));
}

#[test]
fn test_exact_fit_no_wrap() {
    let cols_per_row = 100;

    // Exactly 100 chars fits in 1 row
    assert_eq!(screen_rows_for_line(100, cols_per_row), 1);

    // Position 99 is the last column of row 0
    assert_eq!(buffer_col_to_screen_pos(99, cols_per_row), (0, 99));

    // Position 100 starts row 1
    assert_eq!(buffer_col_to_screen_pos(100, cols_per_row), (1, 0));
}

#[test]
fn test_continuation_row_detection() {
    // A continuation row is any row_offset > 0
    assert!(!is_continuation_row(0));
    assert!(is_continuation_row(1));
    assert!(is_continuation_row(2));
    assert!(is_continuation_row(100));
}

fn is_continuation_row(row_offset: usize) -> bool {
    row_offset > 0
}
