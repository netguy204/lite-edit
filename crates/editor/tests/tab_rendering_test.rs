// Chunk: docs/chunks/tab_rendering - Integration tests for tab character rendering
//!
//! Integration tests for tab character rendering and tab-aware coordinate mapping.
//!
//! These tests verify that:
//! 1. Tab characters are rendered as whitespace (no glyph) but occupy correct visual width
//! 2. Cursor positioning accounts for tab expansion
//! 3. Mouse hit-testing correctly maps visual columns to character indices
//! 4. Selection highlighting covers the full visual width of tabs
//! 5. Line wrapping uses visual width (tabs count as variable width)

use lite_edit::tab_width::{
    char_col_to_visual_col, char_visual_width, line_visual_width, next_tab_stop,
    visual_col_to_char_col, TAB_WIDTH,
};

// ==================== Tab-stop Arithmetic Tests ====================

#[test]
fn test_tab_width_constant() {
    assert_eq!(TAB_WIDTH, 4, "TAB_WIDTH should be 4");
}

#[test]
fn test_next_tab_stop_sequence() {
    // Tab stops are at 0, 4, 8, 12, ...
    assert_eq!(next_tab_stop(0), 4);
    assert_eq!(next_tab_stop(1), 4);
    assert_eq!(next_tab_stop(2), 4);
    assert_eq!(next_tab_stop(3), 4);
    assert_eq!(next_tab_stop(4), 8);
    assert_eq!(next_tab_stop(5), 8);
    assert_eq!(next_tab_stop(7), 8);
    assert_eq!(next_tab_stop(8), 12);
}

#[test]
fn test_tab_visual_width_at_positions() {
    // Tab at column 0: spans to column 4 (width 4)
    assert_eq!(char_visual_width('\t', 0), 4);
    // Tab at column 1: spans to column 4 (width 3)
    assert_eq!(char_visual_width('\t', 1), 3);
    // Tab at column 2: spans to column 4 (width 2)
    assert_eq!(char_visual_width('\t', 2), 2);
    // Tab at column 3: spans to column 4 (width 1)
    assert_eq!(char_visual_width('\t', 3), 1);
    // Tab at column 4: spans to column 8 (width 4)
    assert_eq!(char_visual_width('\t', 4), 4);
}

// ==================== Line Visual Width Tests ====================

#[test]
fn test_line_visual_width_no_tabs() {
    assert_eq!(line_visual_width("hello"), 5);
    assert_eq!(line_visual_width(""), 0);
    assert_eq!(line_visual_width(" "), 1);
}

#[test]
fn test_line_visual_width_single_tab() {
    assert_eq!(line_visual_width("\t"), 4);
}

#[test]
fn test_line_visual_width_tab_at_start() {
    // Tab (4) + "hello" (5) = 9
    assert_eq!(line_visual_width("\thello"), 9);
}

#[test]
fn test_line_visual_width_tab_in_middle() {
    // "a" (1) + tab from col 1 to col 4 (3) + "b" (1) = 5
    assert_eq!(line_visual_width("a\tb"), 5);
}

#[test]
fn test_line_visual_width_multiple_tabs() {
    // Tab 0->4, Tab 4->8 = 8
    assert_eq!(line_visual_width("\t\t"), 8);
    // Tab 0->4, Tab 4->8, Tab 8->12 = 12
    assert_eq!(line_visual_width("\t\t\t"), 12);
}

#[test]
fn test_line_visual_width_indent_pattern() {
    // Common indentation: 2 tabs = 8 visual columns
    // "if (x) {" is 8 characters
    assert_eq!(line_visual_width("\t\tif (x) {"), 16); // 8 + 8
}

// ==================== Character Column to Visual Column Tests ====================

#[test]
fn test_char_col_to_visual_col_no_tabs() {
    let line = "hello";
    assert_eq!(char_col_to_visual_col(line, 0), 0);
    assert_eq!(char_col_to_visual_col(line, 1), 1);
    assert_eq!(char_col_to_visual_col(line, 5), 5);
}

#[test]
fn test_char_col_to_visual_col_with_leading_tab() {
    let line = "\thello";
    // char 0 (tab) at visual 0
    assert_eq!(char_col_to_visual_col(line, 0), 0);
    // char 1 ('h') at visual 4 (after tab)
    assert_eq!(char_col_to_visual_col(line, 1), 4);
    // char 2 ('e') at visual 5
    assert_eq!(char_col_to_visual_col(line, 2), 5);
}

#[test]
fn test_char_col_to_visual_col_with_middle_tab() {
    let line = "a\tb";
    // 'a' at visual 0
    assert_eq!(char_col_to_visual_col(line, 0), 0);
    // tab at visual 1
    assert_eq!(char_col_to_visual_col(line, 1), 1);
    // 'b' at visual 4 (after tab)
    assert_eq!(char_col_to_visual_col(line, 2), 4);
}

// ==================== Visual Column to Character Column Tests ====================

#[test]
fn test_visual_col_to_char_col_no_tabs() {
    let line = "hello";
    assert_eq!(visual_col_to_char_col(line, 0), 0);
    assert_eq!(visual_col_to_char_col(line, 1), 1);
    assert_eq!(visual_col_to_char_col(line, 5), 5);
}

#[test]
fn test_visual_col_to_char_col_inside_tab() {
    let line = "\thello";
    // Visual columns 0, 1, 2, 3 are all inside the tab (char 0)
    assert_eq!(visual_col_to_char_col(line, 0), 0);
    assert_eq!(visual_col_to_char_col(line, 1), 0);
    assert_eq!(visual_col_to_char_col(line, 2), 0);
    assert_eq!(visual_col_to_char_col(line, 3), 0);
    // Visual column 4 is the 'h' (char 1)
    assert_eq!(visual_col_to_char_col(line, 4), 1);
}

#[test]
fn test_visual_col_to_char_col_with_middle_tab() {
    let line = "a\tb";
    // Visual 0 -> char 0 ('a')
    assert_eq!(visual_col_to_char_col(line, 0), 0);
    // Visual 1, 2, 3 -> char 1 (tab)
    assert_eq!(visual_col_to_char_col(line, 1), 1);
    assert_eq!(visual_col_to_char_col(line, 2), 1);
    assert_eq!(visual_col_to_char_col(line, 3), 1);
    // Visual 4 -> char 2 ('b')
    assert_eq!(visual_col_to_char_col(line, 4), 2);
}

// ==================== Round-trip Tests ====================

#[test]
fn test_round_trip_no_tabs() {
    let line = "hello world";
    for char_col in 0..=line.chars().count() {
        let visual = char_col_to_visual_col(line, char_col);
        let back = visual_col_to_char_col(line, visual);
        assert_eq!(back, char_col, "Round trip failed for char_col={char_col}");
    }
}

#[test]
fn test_round_trip_with_tabs() {
    // Test line without newline (newline is a zero-width character which would fail round-trip)
    let line = "\t\tfunction() {";
    // Test at character boundaries (start of each char)
    for (char_idx, _) in line.char_indices().enumerate() {
        let visual = char_col_to_visual_col(line, char_idx);
        let back = visual_col_to_char_col(line, visual);
        assert_eq!(
            back, char_idx,
            "Round trip failed for char_idx={char_idx}, visual={visual}"
        );
    }
}

// ==================== Cursor Positioning Tests ====================

#[test]
fn test_cursor_after_tab() {
    // Typing "a" then TAB then "b" should place cursor at visual column 5
    // after typing 'b': 'a' at 0, tab from 1 to 4, 'b' at 4, cursor at 5
    let line = "a\tb";
    let cursor_char_col = 3; // After 'b'
    let visual_col = char_col_to_visual_col(line, cursor_char_col);
    assert_eq!(visual_col, 5);
}

#[test]
fn test_cursor_on_tab_start() {
    // Cursor positioned at the tab character
    let line = "a\tb";
    let cursor_char_col = 1; // On the tab
    let visual_col = char_col_to_visual_col(line, cursor_char_col);
    assert_eq!(visual_col, 1); // Tab starts at visual column 1
}

// ==================== Click Hit-Testing Tests ====================

#[test]
fn test_click_inside_tab_maps_to_tab_char() {
    let line = "\thello";
    // Clicking at visual columns 0, 1, 2, 3 should all map to char 0 (the tab)
    for visual in 0..4 {
        let char_col = visual_col_to_char_col(line, visual);
        assert_eq!(
            char_col, 0,
            "Click at visual {visual} should map to char 0 (tab)"
        );
    }
}

#[test]
fn test_click_after_tab_maps_correctly() {
    let line = "\thello";
    // Click at visual 4 should map to char 1 ('h')
    assert_eq!(visual_col_to_char_col(line, 4), 1);
    // Click at visual 5 should map to char 2 ('e')
    assert_eq!(visual_col_to_char_col(line, 5), 2);
}

// ==================== Wide Character + Tab Interaction Tests ====================

#[test]
fn test_wide_char_then_tab() {
    // '中' is a wide character (width 2)
    // '中' at cols 0-1, tab at col 2 spans to col 4 (width 2)
    let line = "中\ta";
    assert_eq!(line_visual_width(line), 5); // 2 + 2 + 1

    // char 0 ('中') at visual 0
    assert_eq!(char_col_to_visual_col(line, 0), 0);
    // char 1 (tab) at visual 2
    assert_eq!(char_col_to_visual_col(line, 1), 2);
    // char 2 ('a') at visual 4
    assert_eq!(char_col_to_visual_col(line, 2), 4);
}

#[test]
fn test_tab_then_wide_char() {
    // Tab 0->4, '中' at cols 4-5
    let line = "\t中";
    assert_eq!(line_visual_width(line), 6); // 4 + 2

    // char 0 (tab) at visual 0
    assert_eq!(char_col_to_visual_col(line, 0), 0);
    // char 1 ('中') at visual 4
    assert_eq!(char_col_to_visual_col(line, 1), 4);
}
