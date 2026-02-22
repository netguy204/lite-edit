// Chunk: docs/chunks/line_wrap_rendering - Soft line wrapping coordinate mapping
//!
//! Wrap layout calculation for soft line wrapping
//!
//! This module provides coordinate mapping between logical (buffer) and visual (screen)
//! line positions. The key insight is that with a fixed-width (monospace) font, all
//! coordinate mapping reduces to pure O(1) integer arithmetic:
//!
//! ```text
//! cols_per_row   = floor(viewport_width_px / glyph_width_px)
//! screen_rows(line)   = ceil(line.char_count / cols_per_row)     // O(1)
//! screen_pos(buf_col) = divmod(buf_col, cols_per_row)            // O(1) → (row_offset, col)
//! buffer_col(row_off, col) = row_off * cols_per_row + col        // O(1)
//! ```
//!
//! No cache, no data structure, no invalidation. The `WrapLayout` struct is stateless
//! and computes all mappings on the fly.

use crate::font::FontMetrics;

/// Stateless layout calculator for soft line wrapping.
///
/// This struct encapsulates the wrap arithmetic and becomes the single source of truth
/// for all logical-line ↔ visual-line coordinate mapping. It's recomputed whenever the
/// viewport width or font metrics change.
#[derive(Debug, Clone, Copy)]
pub struct WrapLayout {
    /// Number of character columns that fit in the viewport
    cols_per_row: usize,
    /// Glyph width in pixels (from FontMetrics)
    glyph_width: f32,
    /// Line height in pixels
    line_height: f32,
}

impl WrapLayout {
    /// Creates a new WrapLayout from viewport width and font metrics.
    ///
    /// # Arguments
    /// * `viewport_width_px` - The viewport width in pixels
    /// * `metrics` - Font metrics providing glyph dimensions
    ///
    /// # Returns
    /// A new WrapLayout configured for the given dimensions.
    pub fn new(viewport_width_px: f32, metrics: &FontMetrics) -> Self {
        let glyph_width = metrics.advance_width as f32;
        let line_height = metrics.line_height as f32;

        // Calculate how many characters fit per screen row
        // Use floor to ensure we don't overflow the viewport
        let cols_per_row = if glyph_width > 0.0 {
            (viewport_width_px / glyph_width).floor() as usize
        } else {
            1 // Fallback to prevent division by zero
        };

        // Ensure at least 1 column per row
        let cols_per_row = cols_per_row.max(1);

        Self {
            cols_per_row,
            glyph_width,
            line_height,
        }
    }

    /// Returns the number of character columns per screen row.
    #[inline]
    pub fn cols_per_row(&self) -> usize {
        self.cols_per_row
    }

    /// Returns the glyph width in pixels.
    #[inline]
    pub fn glyph_width(&self) -> f32 {
        self.glyph_width
    }

    /// Returns the line height in pixels.
    #[inline]
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    // Chunk: docs/chunks/line_wrap_rendering - O(1) screen row count for a buffer line
    /// Returns the number of visual screen rows needed to display a line with `char_count` characters.
    ///
    /// This is O(1) arithmetic: `ceil(char_count / cols_per_row)`.
    ///
    /// # Arguments
    /// * `char_count` - Number of characters in the buffer line
    ///
    /// # Returns
    /// The number of screen rows needed (at least 1, even for empty lines).
    #[inline]
    pub fn screen_rows_for_line(&self, char_count: usize) -> usize {
        if char_count == 0 {
            1 // Empty line still takes one screen row
        } else {
            // Ceiling division: (char_count + cols_per_row - 1) / cols_per_row
            (char_count + self.cols_per_row - 1) / self.cols_per_row
        }
    }

    // Chunk: docs/chunks/line_wrap_rendering - O(1) buffer column to screen position
    /// Converts a buffer column to screen position within a wrapped line.
    ///
    /// This is O(1) arithmetic: `divmod(buf_col, cols_per_row)`.
    ///
    /// # Arguments
    /// * `buf_col` - The column index in the buffer line (0-indexed)
    ///
    /// # Returns
    /// A tuple `(row_offset, screen_col)` where:
    /// - `row_offset` is which screen row within the wrapped line (0 = first row)
    /// - `screen_col` is the column within that screen row
    #[inline]
    pub fn buffer_col_to_screen_pos(&self, buf_col: usize) -> (usize, usize) {
        let row_offset = buf_col / self.cols_per_row;
        let screen_col = buf_col % self.cols_per_row;
        (row_offset, screen_col)
    }

    // Chunk: docs/chunks/line_wrap_rendering - O(1) screen position to buffer column
    /// Converts a screen position within a wrapped line back to a buffer column.
    ///
    /// This is O(1) arithmetic: `row_offset * cols_per_row + screen_col`.
    ///
    /// # Arguments
    /// * `row_offset` - Which screen row within the wrapped line (0 = first row)
    /// * `screen_col` - Column within that screen row
    ///
    /// # Returns
    /// The buffer column index.
    #[inline]
    pub fn screen_pos_to_buffer_col(&self, row_offset: usize, screen_col: usize) -> usize {
        row_offset * self.cols_per_row + screen_col
    }

    // Chunk: docs/chunks/line_wrap_rendering - Continuation row detection
    /// Returns true if this is a continuation row (not the first row of a buffer line).
    ///
    /// Continuation rows are rendered with a visual indicator (left-edge border).
    ///
    /// # Arguments
    /// * `row_offset` - Which screen row within the wrapped line (0 = first row)
    #[inline]
    pub fn is_continuation_row(&self, row_offset: usize) -> bool {
        row_offset > 0
    }

    // Chunk: docs/chunks/line_wrap_rendering - Pixel position for wrapped character
    /// Calculates the screen position (x, y) in pixels for a character at the given
    /// buffer column within a wrapped line.
    ///
    /// # Arguments
    /// * `first_screen_row` - The cumulative screen row where this buffer line starts
    /// * `buf_col` - The buffer column within the line
    /// * `y_offset` - Vertical offset for smooth scrolling
    ///
    /// # Returns
    /// A tuple `(x, y)` in pixel coordinates.
    #[inline]
    pub fn position_for_wrapped(
        &self,
        first_screen_row: usize,
        buf_col: usize,
        y_offset: f32,
    ) -> (f32, f32) {
        let (row_offset, screen_col) = self.buffer_col_to_screen_pos(buf_col);
        let screen_row = first_screen_row + row_offset;
        let x = screen_col as f32 * self.glyph_width;
        let y = screen_row as f32 * self.line_height - y_offset;
        (x, y)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metrics() -> FontMetrics {
        FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        }
    }

    // ==================== Construction ====================

    #[test]
    fn test_new_basic() {
        // 800px / 8px per char = 100 cols
        let layout = WrapLayout::new(800.0, &test_metrics());
        assert_eq!(layout.cols_per_row(), 100);
        assert_eq!(layout.glyph_width(), 8.0);
        assert_eq!(layout.line_height(), 16.0);
    }

    #[test]
    fn test_new_fractional_width() {
        // 810px / 8px = 101.25, floor to 101
        let layout = WrapLayout::new(810.0, &test_metrics());
        assert_eq!(layout.cols_per_row(), 101);
    }

    #[test]
    fn test_new_narrow_viewport() {
        // 20px / 8px = 2.5, floor to 2
        let layout = WrapLayout::new(20.0, &test_metrics());
        assert_eq!(layout.cols_per_row(), 2);
    }

    #[test]
    fn test_new_minimum_cols() {
        // Very narrow viewport should have at least 1 col
        let layout = WrapLayout::new(4.0, &test_metrics());
        assert_eq!(layout.cols_per_row(), 1);
    }

    #[test]
    fn test_new_zero_width() {
        // Zero width viewport should have at least 1 col
        let layout = WrapLayout::new(0.0, &test_metrics());
        assert_eq!(layout.cols_per_row(), 1);
    }

    // ==================== screen_rows_for_line ====================

    #[test]
    fn test_screen_rows_empty_line() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_rows_for_line(0), 1);
    }

    #[test]
    fn test_screen_rows_short_line() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_rows_for_line(50), 1);
    }

    #[test]
    fn test_screen_rows_exact_fit() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_rows_for_line(100), 1);
    }

    #[test]
    fn test_screen_rows_one_over() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_rows_for_line(101), 2);
    }

    #[test]
    fn test_screen_rows_double() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_rows_for_line(200), 2);
    }

    #[test]
    fn test_screen_rows_multiple() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_rows_for_line(350), 4); // ceil(350/100) = 4
    }

    #[test]
    fn test_screen_rows_narrow_viewport() {
        let layout = WrapLayout::new(80.0, &test_metrics()); // 10 cols
        assert_eq!(layout.screen_rows_for_line(25), 3); // ceil(25/10) = 3
    }

    // ==================== buffer_col_to_screen_pos ====================

    #[test]
    fn test_buffer_to_screen_col_zero() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.buffer_col_to_screen_pos(0), (0, 0));
    }

    #[test]
    fn test_buffer_to_screen_mid_first_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.buffer_col_to_screen_pos(50), (0, 50));
    }

    #[test]
    fn test_buffer_to_screen_end_first_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.buffer_col_to_screen_pos(99), (0, 99));
    }

    #[test]
    fn test_buffer_to_screen_start_second_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.buffer_col_to_screen_pos(100), (1, 0));
    }

    #[test]
    fn test_buffer_to_screen_mid_second_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.buffer_col_to_screen_pos(125), (1, 25));
    }

    #[test]
    fn test_buffer_to_screen_third_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.buffer_col_to_screen_pos(215), (2, 15));
    }

    // ==================== screen_pos_to_buffer_col ====================

    #[test]
    fn test_screen_to_buffer_origin() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_pos_to_buffer_col(0, 0), 0);
    }

    #[test]
    fn test_screen_to_buffer_first_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_pos_to_buffer_col(0, 50), 50);
    }

    #[test]
    fn test_screen_to_buffer_second_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        assert_eq!(layout.screen_pos_to_buffer_col(1, 0), 100);
        assert_eq!(layout.screen_pos_to_buffer_col(1, 25), 125);
    }

    #[test]
    fn test_screen_to_buffer_round_trip() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols

        for buf_col in [0, 1, 50, 99, 100, 101, 150, 199, 200, 500] {
            let (row_off, screen_col) = layout.buffer_col_to_screen_pos(buf_col);
            let round_trip = layout.screen_pos_to_buffer_col(row_off, screen_col);
            assert_eq!(
                round_trip, buf_col,
                "Round trip failed for buf_col={buf_col}"
            );
        }
    }

    // ==================== is_continuation_row ====================

    #[test]
    fn test_is_continuation_first_row() {
        let layout = WrapLayout::new(800.0, &test_metrics());
        assert!(!layout.is_continuation_row(0));
    }

    #[test]
    fn test_is_continuation_second_row() {
        let layout = WrapLayout::new(800.0, &test_metrics());
        assert!(layout.is_continuation_row(1));
    }

    #[test]
    fn test_is_continuation_later_rows() {
        let layout = WrapLayout::new(800.0, &test_metrics());
        assert!(layout.is_continuation_row(2));
        assert!(layout.is_continuation_row(10));
    }

    // ==================== position_for_wrapped ====================

    #[test]
    fn test_position_first_char() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        let (x, y) = layout.position_for_wrapped(0, 0, 0.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_position_mid_first_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        let (x, y) = layout.position_for_wrapped(0, 50, 0.0);
        assert_eq!(x, 400.0); // 50 * 8
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_position_wrapped_to_second_row() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols, 16px line height
        // buf_col 100 wraps to row_offset=1, screen_col=0
        let (x, y) = layout.position_for_wrapped(0, 100, 0.0);
        assert_eq!(x, 0.0); // screen_col 0
        assert_eq!(y, 16.0); // second screen row
    }

    #[test]
    fn test_position_wrapped_to_second_row_mid() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        // buf_col 150 wraps to row_offset=1, screen_col=50
        let (x, y) = layout.position_for_wrapped(0, 150, 0.0);
        assert_eq!(x, 400.0); // 50 * 8
        assert_eq!(y, 16.0); // second screen row
    }

    #[test]
    fn test_position_with_first_screen_row_offset() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        // Buffer line starts at screen row 5, buf_col 0
        let (x, y) = layout.position_for_wrapped(5, 0, 0.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 80.0); // 5 * 16
    }

    #[test]
    fn test_position_with_y_offset() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        let (x, y) = layout.position_for_wrapped(0, 0, 8.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, -8.0); // 0 - 8
    }

    #[test]
    fn test_position_combined() {
        let layout = WrapLayout::new(800.0, &test_metrics()); // 100 cols
        // Buffer line starts at screen row 3, buf_col 125 wraps to row_offset=1, screen_col=25
        // Total screen row = 3 + 1 = 4
        let (x, y) = layout.position_for_wrapped(3, 125, 5.0);
        assert_eq!(x, 200.0); // 25 * 8
        assert_eq!(y, 59.0); // 4 * 16 - 5 = 64 - 5
    }
}
