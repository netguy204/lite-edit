// Chunk: docs/chunks/tab_rendering - Tab character rendering and tab-aware coordinate mapping
//!
//! Tab character rendering and tab-aware coordinate mapping.
//!
//! This module provides pure helper functions for tab-stop arithmetic and
//! visual width calculation. The key insight is that tab characters expand
//! to variable widths depending on their position in the line: a tab at
//! visual column 0 spans to column 4, but a tab at column 2 only spans to
//! column 4 (2 columns).
//!
//! All functions in this module are pure (no side effects) and operate on
//! character/visual column indices.

use unicode_width::UnicodeWidthChar;

/// The number of columns per tab stop. This is a compile-time constant.
/// A runtime configuration system does not exist yet.
pub const TAB_WIDTH: usize = 4;

/// Returns the next tab stop column after `visual_col`.
///
/// Tab stops are at columns 0, TAB_WIDTH, 2*TAB_WIDTH, ...
///
/// # Examples
///
/// ```
/// use lite_edit::tab_width::{next_tab_stop, TAB_WIDTH};
/// assert_eq!(TAB_WIDTH, 4);
/// assert_eq!(next_tab_stop(0), 4);
/// assert_eq!(next_tab_stop(1), 4);
/// assert_eq!(next_tab_stop(3), 4);
/// assert_eq!(next_tab_stop(4), 8);
/// assert_eq!(next_tab_stop(7), 8);
/// ```
#[inline]
pub fn next_tab_stop(visual_col: usize) -> usize {
    ((visual_col / TAB_WIDTH) + 1) * TAB_WIDTH
}

/// Returns the visual width of a character at the given visual column.
///
/// - Tab characters span from `visual_col` to the next tab stop.
/// - Wide characters (CJK, emoji) return 2.
/// - Other characters return 1.
/// - Control characters and zero-width characters return 0.
///
/// # Examples
///
/// ```
/// use lite_edit::tab_width::char_visual_width;
/// // Tab at column 0 spans to column 4 (width = 4)
/// assert_eq!(char_visual_width('\t', 0), 4);
/// // Tab at column 2 spans to column 4 (width = 2)
/// assert_eq!(char_visual_width('\t', 2), 2);
/// // Tab at column 4 spans to column 8 (width = 4)
/// assert_eq!(char_visual_width('\t', 4), 4);
/// // Regular character always has width 1
/// assert_eq!(char_visual_width('a', 0), 1);
/// assert_eq!(char_visual_width('a', 5), 1);
/// ```
#[inline]
pub fn char_visual_width(c: char, visual_col: usize) -> usize {
    if c == '\t' {
        next_tab_stop(visual_col) - visual_col
    } else {
        // Use unicode_width for proper wide character handling
        c.width().unwrap_or(0)
    }
}

/// Returns the total visual width of a string, accounting for tabs and wide chars.
///
/// # Examples
///
/// ```
/// use lite_edit::tab_width::line_visual_width;
/// // "a\tb" with TAB_WIDTH=4: 'a' at col 0 (width 1), '\t' at col 1 (width 3), 'b' at col 4 (width 1)
/// // Total: 1 + 3 + 1 = 5
/// assert_eq!(line_visual_width("a\tb"), 5);
/// // Just a tab at column 0 spans to column 4
/// assert_eq!(line_visual_width("\t"), 4);
/// // "abc" = 3 columns
/// assert_eq!(line_visual_width("abc"), 3);
/// ```
pub fn line_visual_width(line: &str) -> usize {
    let mut visual_col = 0;
    for c in line.chars() {
        visual_col += char_visual_width(c, visual_col);
    }
    visual_col
}

/// Converts a buffer column (character index) to a visual column.
///
/// Returns the visual column where the character at `char_col` begins.
///
/// # Examples
///
/// ```
/// use lite_edit::tab_width::char_col_to_visual_col;
/// // "a\tb": char 0 ('a') at visual 0, char 1 ('\t') at visual 1, char 2 ('b') at visual 4
/// assert_eq!(char_col_to_visual_col("a\tb", 0), 0);
/// assert_eq!(char_col_to_visual_col("a\tb", 1), 1);
/// assert_eq!(char_col_to_visual_col("a\tb", 2), 4);
/// // Past end of line returns line's visual width
/// assert_eq!(char_col_to_visual_col("a\tb", 3), 5);
/// ```
pub fn char_col_to_visual_col(line: &str, char_col: usize) -> usize {
    let mut visual_col = 0;
    for (i, c) in line.chars().enumerate() {
        if i >= char_col {
            return visual_col;
        }
        visual_col += char_visual_width(c, visual_col);
    }
    // Past end of line - return total visual width
    visual_col
}

/// Converts a visual column to a buffer column (character index).
///
/// If the visual column is in the middle of a tab's visual span, returns the
/// tab's character index.
///
/// If the visual column is past the line end, returns the line's character count.
///
/// # Examples
///
/// ```
/// use lite_edit::tab_width::visual_col_to_char_col;
/// // "a\tb": visual 0 -> char 0, visual 1-3 -> char 1 (tab), visual 4 -> char 2
/// assert_eq!(visual_col_to_char_col("a\tb", 0), 0);
/// assert_eq!(visual_col_to_char_col("a\tb", 1), 1);
/// assert_eq!(visual_col_to_char_col("a\tb", 2), 1); // inside tab
/// assert_eq!(visual_col_to_char_col("a\tb", 3), 1); // inside tab
/// assert_eq!(visual_col_to_char_col("a\tb", 4), 2);
/// // Past end returns char count
/// assert_eq!(visual_col_to_char_col("a\tb", 10), 3);
/// ```
pub fn visual_col_to_char_col(line: &str, visual_col: usize) -> usize {
    let mut current_visual = 0;
    for (i, c) in line.chars().enumerate() {
        let char_width = char_visual_width(c, current_visual);
        // If the target visual column falls within this character's span
        if visual_col < current_visual + char_width {
            return i;
        }
        current_visual += char_width;
    }
    // Past end of line - return character count
    line.chars().count()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== next_tab_stop ====================

    #[test]
    fn test_next_tab_stop_at_zero() {
        assert_eq!(next_tab_stop(0), 4);
    }

    #[test]
    fn test_next_tab_stop_mid_first() {
        assert_eq!(next_tab_stop(1), 4);
        assert_eq!(next_tab_stop(2), 4);
        assert_eq!(next_tab_stop(3), 4);
    }

    #[test]
    fn test_next_tab_stop_at_boundary() {
        assert_eq!(next_tab_stop(4), 8);
        assert_eq!(next_tab_stop(8), 12);
    }

    #[test]
    fn test_next_tab_stop_mid_second() {
        assert_eq!(next_tab_stop(5), 8);
        assert_eq!(next_tab_stop(6), 8);
        assert_eq!(next_tab_stop(7), 8);
    }

    // ==================== char_visual_width ====================

    #[test]
    fn test_char_visual_width_tab_at_zero() {
        assert_eq!(char_visual_width('\t', 0), 4);
    }

    #[test]
    fn test_char_visual_width_tab_mid_stop() {
        assert_eq!(char_visual_width('\t', 1), 3);
        assert_eq!(char_visual_width('\t', 2), 2);
        assert_eq!(char_visual_width('\t', 3), 1);
    }

    #[test]
    fn test_char_visual_width_tab_at_boundary() {
        assert_eq!(char_visual_width('\t', 4), 4);
        assert_eq!(char_visual_width('\t', 8), 4);
    }

    #[test]
    fn test_char_visual_width_regular_char() {
        assert_eq!(char_visual_width('a', 0), 1);
        assert_eq!(char_visual_width('a', 5), 1);
        assert_eq!(char_visual_width('z', 100), 1);
    }

    #[test]
    fn test_char_visual_width_wide_char() {
        // CJK character (Chinese, Japanese, Korean) should have width 2
        assert_eq!(char_visual_width('中', 0), 2);
        assert_eq!(char_visual_width('日', 5), 2);
    }

    #[test]
    fn test_char_visual_width_space() {
        assert_eq!(char_visual_width(' ', 0), 1);
    }

    // ==================== line_visual_width ====================

    #[test]
    fn test_line_visual_width_empty() {
        assert_eq!(line_visual_width(""), 0);
    }

    #[test]
    fn test_line_visual_width_simple() {
        assert_eq!(line_visual_width("abc"), 3);
    }

    #[test]
    fn test_line_visual_width_single_tab() {
        assert_eq!(line_visual_width("\t"), 4);
    }

    #[test]
    fn test_line_visual_width_tab_then_char() {
        // Tab from 0->4, then 'a' at 4
        assert_eq!(line_visual_width("\ta"), 5);
    }

    #[test]
    fn test_line_visual_width_char_tab_char() {
        // 'a' at 0 (width 1), tab from 1->4 (width 3), 'b' at 4 (width 1)
        // Total: 1 + 3 + 1 = 5
        assert_eq!(line_visual_width("a\tb"), 5);
    }

    #[test]
    fn test_line_visual_width_multiple_tabs() {
        // Tab 0->4, Tab 4->8
        assert_eq!(line_visual_width("\t\t"), 8);
    }

    #[test]
    fn test_line_visual_width_with_wide_char() {
        // 'a' (1) + '中' (2) + 'b' (1) = 4
        assert_eq!(line_visual_width("a中b"), 4);
    }

    // ==================== char_col_to_visual_col ====================

    #[test]
    fn test_char_col_to_visual_col_simple() {
        assert_eq!(char_col_to_visual_col("abc", 0), 0);
        assert_eq!(char_col_to_visual_col("abc", 1), 1);
        assert_eq!(char_col_to_visual_col("abc", 2), 2);
        assert_eq!(char_col_to_visual_col("abc", 3), 3); // past end
    }

    #[test]
    fn test_char_col_to_visual_col_with_tab() {
        // "a\tb": char 0 at visual 0, char 1 (tab) at visual 1, char 2 at visual 4
        assert_eq!(char_col_to_visual_col("a\tb", 0), 0);
        assert_eq!(char_col_to_visual_col("a\tb", 1), 1);
        assert_eq!(char_col_to_visual_col("a\tb", 2), 4);
        assert_eq!(char_col_to_visual_col("a\tb", 3), 5); // past end
    }

    #[test]
    fn test_char_col_to_visual_col_tab_at_start() {
        // "\tb": char 0 (tab) at visual 0, char 1 at visual 4
        assert_eq!(char_col_to_visual_col("\tb", 0), 0);
        assert_eq!(char_col_to_visual_col("\tb", 1), 4);
    }

    #[test]
    fn test_char_col_to_visual_col_with_wide_char() {
        // "a中b": char 0 at visual 0, char 1 at visual 1, char 2 at visual 3
        assert_eq!(char_col_to_visual_col("a中b", 0), 0);
        assert_eq!(char_col_to_visual_col("a中b", 1), 1);
        assert_eq!(char_col_to_visual_col("a中b", 2), 3);
    }

    // ==================== visual_col_to_char_col ====================

    #[test]
    fn test_visual_col_to_char_col_simple() {
        assert_eq!(visual_col_to_char_col("abc", 0), 0);
        assert_eq!(visual_col_to_char_col("abc", 1), 1);
        assert_eq!(visual_col_to_char_col("abc", 2), 2);
        assert_eq!(visual_col_to_char_col("abc", 3), 3); // past end
        assert_eq!(visual_col_to_char_col("abc", 10), 3); // far past end
    }

    #[test]
    fn test_visual_col_to_char_col_with_tab() {
        // "a\tb": visual 0 -> char 0, visual 1-3 -> char 1 (tab), visual 4 -> char 2
        assert_eq!(visual_col_to_char_col("a\tb", 0), 0);
        assert_eq!(visual_col_to_char_col("a\tb", 1), 1); // start of tab
        assert_eq!(visual_col_to_char_col("a\tb", 2), 1); // inside tab
        assert_eq!(visual_col_to_char_col("a\tb", 3), 1); // inside tab
        assert_eq!(visual_col_to_char_col("a\tb", 4), 2); // 'b'
    }

    #[test]
    fn test_visual_col_to_char_col_tab_at_start() {
        // "\tb": visual 0-3 -> char 0 (tab), visual 4 -> char 1
        assert_eq!(visual_col_to_char_col("\tb", 0), 0);
        assert_eq!(visual_col_to_char_col("\tb", 1), 0);
        assert_eq!(visual_col_to_char_col("\tb", 2), 0);
        assert_eq!(visual_col_to_char_col("\tb", 3), 0);
        assert_eq!(visual_col_to_char_col("\tb", 4), 1);
    }

    #[test]
    fn test_visual_col_to_char_col_with_wide_char() {
        // "a中b": visual 0 -> char 0, visual 1-2 -> char 1 (wide), visual 3 -> char 2
        assert_eq!(visual_col_to_char_col("a中b", 0), 0);
        assert_eq!(visual_col_to_char_col("a中b", 1), 1); // start of wide char
        assert_eq!(visual_col_to_char_col("a中b", 2), 1); // inside wide char
        assert_eq!(visual_col_to_char_col("a中b", 3), 2); // 'b'
    }

    // ==================== Round-trip tests ====================

    #[test]
    fn test_round_trip_simple() {
        let line = "hello";
        for char_col in 0..=line.chars().count() {
            let visual = char_col_to_visual_col(line, char_col);
            let back = visual_col_to_char_col(line, visual);
            assert_eq!(back, char_col, "round trip failed for char_col={char_col}");
        }
    }

    #[test]
    fn test_round_trip_with_tabs() {
        let line = "a\tb\tc";
        // For positions that are NOT inside a tab, round-trip should work
        // Positions: 'a' at char 0, '\t' at char 1, 'b' at char 2, '\t' at char 3, 'c' at char 4
        let test_positions = [0, 1, 2, 3, 4]; // character positions
        for &char_col in &test_positions {
            let visual = char_col_to_visual_col(line, char_col);
            let back = visual_col_to_char_col(line, visual);
            assert_eq!(back, char_col, "round trip failed for char_col={char_col}, visual={visual}");
        }
    }
}
