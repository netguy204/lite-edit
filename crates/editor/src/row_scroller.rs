// Subsystem: docs/subsystems/viewport_scroll - Viewport mapping & scroll arithmetic
// Chunk: docs/chunks/row_scroller_extract - RowScroller extraction from Viewport
//!
//! Reusable scroll arithmetic for uniform-height row lists.
//!
//! `RowScroller` encapsulates the fractional pixel scroll logic for any scrollable
//! list of uniform-height rows. It handles:
//!
//! - Tracking scroll position in pixels (for smooth fractional scrolling)
//! - Computing which rows are visible based on scroll offset and viewport height
//! - Clamping scroll position to valid bounds
//! - Converting between row indices and screen positions
//!
//! This is a pure data structure with no platform dependencies, making it fully
//! testable without mocking. It differs from `Viewport` in that it has no knowledge
//! of text buffers, wrapping, or dirty regions — it's just scroll arithmetic.
//!
//! # Example
//!
//! ```ignore
//! use crate::row_scroller::RowScroller;
//!
//! let mut scroller = RowScroller::new(20.0); // 20px row height
//! scroller.update_size(200.0, 100); // 200px viewport = 10 visible rows, 100 total rows
//!
//! // Scroll to show row 5 at the top
//! scroller.scroll_to(5, 100); // 100 total rows
//!
//! // Ensure row 15 is visible (will scroll down if needed)
//! scroller.ensure_visible(15, 100);
//! ```

use std::ops::Range;

/// Scroll state and arithmetic for uniform-height row lists.
///
/// This struct maintains the scroll position and computes visible ranges for any
/// scrollable list where all items have the same height (e.g., text lines in a
/// monospace editor, file picker entries, terminal output lines).
///
/// The scroll position is tracked in floating-point pixels, enabling smooth
/// sub-row scrolling. The integer row index is derived as
/// `(scroll_offset_px / row_height).floor()`.
#[derive(Debug, Clone)]
pub struct RowScroller {
    /// Scroll position in pixels (distance from top of content to top of viewport)
    scroll_offset_px: f32,
    /// Number of rows that fit fully in the viewport
    visible_rows: usize,
    /// Height of each row in pixels
    row_height: f32,
}

impl RowScroller {
    /// Creates a new `RowScroller` with the given row height.
    ///
    /// The scroller starts at scroll offset 0.0 with 0 visible rows.
    /// Call `update_size()` to set the visible row count based on viewport height.
    pub fn new(row_height: f32) -> Self {
        Self {
            scroll_offset_px: 0.0,
            visible_rows: 0,
            row_height,
        }
    }

    /// Returns the row height in pixels.
    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    /// Returns the number of visible rows in the viewport.
    pub fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    /// Returns the first visible row (derived from pixel offset).
    ///
    /// This is computed as `(scroll_offset_px / row_height).floor()`.
    /// The result is the integer row index used for content-to-screen mapping.
    pub fn first_visible_row(&self) -> usize {
        if self.row_height <= 0.0 {
            return 0;
        }
        (self.scroll_offset_px / self.row_height).floor() as usize
    }

    /// Returns the fractional pixel remainder of the scroll position.
    ///
    /// This is the number of pixels the viewport is scrolled past the start
    /// of `first_visible_row()`. Renderers use this to offset all drawn
    /// rows by `-scroll_fraction_px()` in Y, causing the top row to be
    /// partially clipped and content to scroll smoothly between row positions.
    ///
    /// Returns a value in the range `[0.0, row_height)`.
    pub fn scroll_fraction_px(&self) -> f32 {
        if self.row_height <= 0.0 {
            return 0.0;
        }
        self.scroll_offset_px % self.row_height
    }

    /// Returns the raw scroll offset in pixels.
    ///
    /// This is the authoritative scroll state. Use `first_visible_row()` for
    /// the derived integer row index used in content-to-screen mapping.
    pub fn scroll_offset_px(&self) -> f32 {
        self.scroll_offset_px
    }

    /// Sets the scroll offset in pixels, with clamping to valid bounds.
    ///
    /// The offset is clamped to `[0.0, max_offset_px]` where:
    /// `max_offset_px = (row_count - visible_rows) * row_height`
    ///
    /// This ensures the viewport doesn't scroll past the start or end of the content.
    pub fn set_scroll_offset_px(&mut self, px: f32, row_count: usize) {
        let max_rows = row_count.saturating_sub(self.visible_rows);
        let max_offset_px = max_rows as f32 * self.row_height;
        self.scroll_offset_px = px.clamp(0.0, max_offset_px);
    }

    /// Updates the viewport size based on height in pixels.
    ///
    /// This recomputes `visible_rows` = floor(height_px / row_height) and re-clamps
    /// `scroll_offset_px` to the new valid bounds.
    // Chunk: docs/chunks/resize_click_alignment - Re-clamp scroll offset on resize
    pub fn update_size(&mut self, height_px: f32, row_count: usize) {
        self.visible_rows = if self.row_height > 0.0 {
            (height_px / self.row_height).floor() as usize
        } else {
            0
        };
        // Re-clamp scroll offset to new valid bounds
        self.set_scroll_offset_px(self.scroll_offset_px, row_count);
    }

    /// Returns the range of rows visible in the viewport.
    ///
    /// The range is `[first_visible_row, min(first_visible_row + visible_rows + 1, row_count))`.
    /// The `+1` accounts for the partially-visible row at the bottom when scrolled
    /// to a fractional position.
    ///
    /// If the content has fewer rows than would fill the viewport, the range ends
    /// at the content's end.
    pub fn visible_range(&self, row_count: usize) -> Range<usize> {
        let first_row = self.first_visible_row();
        let start = first_row;
        // Add 1 to visible_rows to account for partially visible row at bottom
        // when scrolled to a fractional position
        let end = (first_row + self.visible_rows + 1).min(row_count);
        start..end
    }

    /// Scrolls the viewport to show the given row at the top.
    ///
    /// The scroll offset is set to align the target row at the top of the viewport.
    /// The offset is clamped to valid pixel bounds to prevent scrolling past the
    /// end of the content.
    ///
    /// This snaps to a whole-row boundary (pixel offset is a multiple of row_height).
    pub fn scroll_to(&mut self, row: usize, row_count: usize) {
        let target_px = row as f32 * self.row_height;
        self.set_scroll_offset_px(target_px, row_count);
    }

    /// Ensures a row is visible, scrolling if necessary.
    ///
    /// Returns `true` if scrolling occurred, `false` if the row was already visible.
    /// This is useful for keeping a selected item or cursor in view.
    ///
    /// When scrolling is needed, this snaps to a whole-row boundary, ensuring
    /// clean alignment after cursor-following operations.
    pub fn ensure_visible(&mut self, row: usize, row_count: usize) -> bool {
        self.ensure_visible_with_margin(row, row_count, 0)
    }

    // Chunk: docs/chunks/find_strip_scroll_clearance - Margin support for overlays
    /// Ensures a row is visible, with additional bottom margin.
    ///
    /// Like `ensure_visible`, but treats the viewport as if it had
    /// `bottom_margin_rows` fewer rows at the bottom. This is useful when
    /// an overlay (like the find strip) occludes the bottom of the viewport.
    ///
    /// When scrolling is needed, this snaps to a whole-row boundary.
    ///
    /// Returns `true` if scrolling occurred, `false` if the row was already visible.
    pub fn ensure_visible_with_margin(
        &mut self,
        row: usize,
        row_count: usize,
        bottom_margin_rows: usize,
    ) -> bool {
        let old_offset_px = self.scroll_offset_px;
        let first_row = self.first_visible_row();

        // Compute effective visible rows, accounting for margin.
        // Clamp to at least 1 to avoid edge cases with very small viewports.
        let effective_visible = self.visible_rows.saturating_sub(bottom_margin_rows).max(1);

        if row < first_row {
            // Row is above viewport - scroll up to put row at top
            // Snap to whole-row boundary
            let target_px = row as f32 * self.row_height;
            self.set_scroll_offset_px(target_px, row_count);
        } else if row >= first_row + effective_visible {
            // Row is below effective viewport - scroll down
            // Put the row at the effective bottom of the viewport
            let new_row = row.saturating_sub(effective_visible.saturating_sub(1));
            // Snap to whole-row boundary
            let target_px = new_row as f32 * self.row_height;
            self.set_scroll_offset_px(target_px, row_count);
        }

        self.scroll_offset_px != old_offset_px
    }

    /// Converts a row index to a visible offset (screen position).
    ///
    /// Returns `Some(offset)` if the row is visible in the viewport,
    /// or `None` if the row is outside the viewport.
    ///
    /// The offset is the distance in rows from the top of the viewport.
    pub fn row_to_visible_offset(&self, row: usize) -> Option<usize> {
        let first_row = self.first_visible_row();
        // Use visible_rows + 1 to account for partially visible bottom row
        if row >= first_row && row < first_row + self.visible_rows + 1 {
            Some(row - first_row)
        } else {
            None
        }
    }

    /// Converts a visible offset (screen position) to a row index.
    ///
    /// The offset is the distance in rows from the top of the viewport.
    /// Returns `first_visible_row() + offset`.
    pub fn visible_offset_to_row(&self, offset: usize) -> usize {
        self.first_visible_row() + offset
    }

    /// Sets scroll offset directly without clamping (for internal use).
    ///
    /// This is used by `Viewport::ensure_visible_wrapped` which does its own
    /// clamping based on wrapped line counts rather than buffer line counts.
    pub(crate) fn set_scroll_offset_unclamped(&mut self, px: f32) {
        self.scroll_offset_px = px;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic construction ====================

    #[test]
    fn test_new() {
        let scroller = RowScroller::new(16.0);
        assert_eq!(scroller.scroll_offset_px(), 0.0);
        assert_eq!(scroller.first_visible_row(), 0);
        assert_eq!(scroller.visible_rows(), 0);
        assert_eq!(scroller.row_height(), 16.0);
    }

    #[test]
    fn test_new_zero_height() {
        let scroller = RowScroller::new(0.0);
        assert_eq!(scroller.row_height(), 0.0);
        assert_eq!(scroller.first_visible_row(), 0);
        assert_eq!(scroller.scroll_fraction_px(), 0.0);
    }

    // ==================== update_size ====================

    #[test]
    fn test_update_size() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        assert_eq!(scroller.visible_rows(), 10); // 160 / 16 = 10
    }

    #[test]
    fn test_update_size_fractional() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(170.0, 100); // 170 / 16 = 10.625
        assert_eq!(scroller.visible_rows(), 10); // floor
    }

    #[test]
    fn test_update_size_zero_height() {
        let mut scroller = RowScroller::new(0.0);
        scroller.update_size(160.0, 100);
        assert_eq!(scroller.visible_rows(), 0);
    }

    // ==================== Getters ====================

    #[test]
    fn test_row_height_getter() {
        let scroller = RowScroller::new(20.0);
        assert_eq!(scroller.row_height(), 20.0);
    }

    #[test]
    fn test_visible_rows_getter() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(80.0, 100);
        assert_eq!(scroller.visible_rows(), 5);
    }

    #[test]
    fn test_scroll_offset_px_getter() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(123.5, 100);
        assert!((scroller.scroll_offset_px() - 123.5).abs() < 0.001);
    }

    // ==================== first_visible_row ====================

    #[test]
    fn test_first_visible_row_at_start() {
        let scroller = RowScroller::new(16.0);
        assert_eq!(scroller.first_visible_row(), 0);
    }

    #[test]
    fn test_first_visible_row_scrolled() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(32.0, 100); // 2 rows * 16px
        assert_eq!(scroller.first_visible_row(), 2);
    }

    #[test]
    fn test_first_visible_row_fractional_scroll() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(25.0, 100); // 1.5625 rows
        assert_eq!(scroller.first_visible_row(), 1); // floor(25/16) = 1
    }

    // ==================== scroll_fraction_px ====================

    #[test]
    fn test_scroll_fraction_px_at_boundary() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(32.0, 100); // exactly 2 rows
        assert!((scroller.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_scroll_fraction_px_mid_row() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(25.0, 100); // 25 % 16 = 9
        assert!((scroller.scroll_fraction_px() - 9.0).abs() < 0.001);
    }

    #[test]
    fn test_scroll_fraction_px_accumulated() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(50.0, 100); // 50 % 16 = 2
        assert_eq!(scroller.first_visible_row(), 3);
        assert!((scroller.scroll_fraction_px() - 2.0).abs() < 0.001);
    }

    // ==================== set_scroll_offset_px ====================

    #[test]
    fn test_set_scroll_offset_px_basic() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(100.0, 100);
        assert!((scroller.scroll_offset_px() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_set_scroll_offset_px_clamp_negative() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.set_scroll_offset_px(-10.0, 100);
        assert_eq!(scroller.scroll_offset_px(), 0.0);
    }

    #[test]
    fn test_set_scroll_offset_px_clamp_max() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        // max offset = (100 - 10) * 16 = 1440 pixels
        scroller.set_scroll_offset_px(2000.0, 100);
        assert!((scroller.scroll_offset_px() - 1440.0).abs() < 0.001);
    }

    #[test]
    fn test_set_scroll_offset_px_small_buffer() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        // With only 5 rows, max offset = 0 (can't scroll)
        scroller.set_scroll_offset_px(100.0, 5);
        assert_eq!(scroller.scroll_offset_px(), 0.0);
    }

    // ==================== visible_range ====================

    #[test]
    fn test_visible_range_at_start() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        let range = scroller.visible_range(100);
        // Includes extra row for partial visibility
        assert_eq!(range, 0..11);
    }

    #[test]
    fn test_visible_range_scrolled() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(20, 100);
        let range = scroller.visible_range(100);
        assert_eq!(range, 20..31);
    }

    #[test]
    fn test_visible_range_at_end() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        scroller.scroll_to(95, 100); // Clamped to 90
        let range = scroller.visible_range(100);
        assert_eq!(range, 90..100);
    }

    #[test]
    fn test_visible_range_small_buffer() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        let range = scroller.visible_range(5);
        assert_eq!(range, 0..5);
    }

    #[test]
    fn test_visible_range_empty_buffer() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        let range = scroller.visible_range(0);
        assert_eq!(range, 0..0);
    }

    // ==================== scroll_to ====================

    #[test]
    fn test_scroll_to_valid() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(20, 100);
        assert_eq!(scroller.first_visible_row(), 20);
        assert!((scroller.scroll_offset_px() - 320.0).abs() < 0.001); // 20 * 16
    }

    #[test]
    fn test_scroll_to_clamped() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        scroller.scroll_to(95, 100); // Should clamp to 90
        assert_eq!(scroller.first_visible_row(), 90);
    }

    #[test]
    fn test_scroll_to_beyond_buffer() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(200, 100);
        assert_eq!(scroller.first_visible_row(), 90);
    }

    #[test]
    fn test_scroll_to_small_buffer() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(5, 5); // Buffer smaller than viewport
        assert_eq!(scroller.first_visible_row(), 0); // Can't scroll
    }

    // ==================== ensure_visible ====================

    #[test]
    fn test_ensure_visible_already_visible() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        let scrolled = scroller.ensure_visible(5, 100);
        assert!(!scrolled);
        assert_eq!(scroller.first_visible_row(), 0);
    }

    #[test]
    fn test_ensure_visible_scroll_up() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(20, 100);
        let scrolled = scroller.ensure_visible(10, 100);
        assert!(scrolled);
        assert_eq!(scroller.first_visible_row(), 10);
    }

    #[test]
    fn test_ensure_visible_scroll_down() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        let scrolled = scroller.ensure_visible(25, 100);
        assert!(scrolled);
        // Should put row 25 at the bottom: 25 - 9 = 16
        assert_eq!(scroller.first_visible_row(), 16);
    }

    #[test]
    fn test_ensure_visible_at_boundary() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows, showing 0..10
        // Row 9 is the last fully visible row
        let scrolled = scroller.ensure_visible(9, 100);
        assert!(!scrolled);
        // Row 10 is just beyond visible
        let scrolled = scroller.ensure_visible(10, 100);
        assert!(scrolled);
    }

    #[test]
    fn test_ensure_visible_snaps_to_whole_row() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        // Start with fractional scroll position
        scroller.set_scroll_offset_px(25.0, 100); // Row 1, fraction 9px
        assert_eq!(scroller.first_visible_row(), 1);
        // ensure_visible should snap to whole-row boundary
        let scrolled = scroller.ensure_visible(15, 100);
        assert!(scrolled);
        assert!((scroller.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    // ==================== row_to_visible_offset ====================

    #[test]
    fn test_row_to_visible_offset_visible() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(10, 100);
        assert_eq!(scroller.row_to_visible_offset(10), Some(0));
        assert_eq!(scroller.row_to_visible_offset(15), Some(5));
        assert_eq!(scroller.row_to_visible_offset(19), Some(9));
    }

    #[test]
    fn test_row_to_visible_offset_above() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(10, 100);
        assert_eq!(scroller.row_to_visible_offset(5), None);
        assert_eq!(scroller.row_to_visible_offset(9), None);
    }

    #[test]
    fn test_row_to_visible_offset_below() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(10, 100);
        // Row 21 is beyond visible_rows + 1 (for partial visibility)
        assert_eq!(scroller.row_to_visible_offset(22), None);
        assert_eq!(scroller.row_to_visible_offset(100), None);
    }

    // ==================== visible_offset_to_row ====================

    #[test]
    fn test_visible_offset_to_row() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        scroller.scroll_to(10, 100);
        assert_eq!(scroller.visible_offset_to_row(0), 10);
        assert_eq!(scroller.visible_offset_to_row(5), 15);
        assert_eq!(scroller.visible_offset_to_row(9), 19);
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_sub_line_delta_accumulates_without_row_change() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        // Scroll 5 pixels (less than row_height of 16)
        scroller.set_scroll_offset_px(5.0, 100);
        assert_eq!(scroller.first_visible_row(), 0);
        assert!((scroller.scroll_fraction_px() - 5.0).abs() < 0.001);
        // Scroll another 5 pixels (total 10)
        scroller.set_scroll_offset_px(10.0, 100);
        assert_eq!(scroller.first_visible_row(), 0);
        assert!((scroller.scroll_fraction_px() - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_crossing_row_boundary() {
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100);
        // Scroll exactly one row
        scroller.set_scroll_offset_px(16.0, 100);
        assert_eq!(scroller.first_visible_row(), 1);
        assert!((scroller.scroll_fraction_px() - 0.0).abs() < 0.001);
        // Scroll a bit more
        scroller.set_scroll_offset_px(20.0, 100);
        assert_eq!(scroller.first_visible_row(), 1);
        assert!((scroller.scroll_fraction_px() - 4.0).abs() < 0.001);
        // Cross to row 2
        scroller.set_scroll_offset_px(32.0, 100);
        assert_eq!(scroller.first_visible_row(), 2);
        assert!((scroller.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    // Chunk: docs/chunks/resize_click_alignment - Regression test for resize clamping
    #[test]
    fn test_resize_clamps_scroll_offset() {
        // Scenario: viewport resizes to show more rows, reducing max_offset_px.
        // If scroll_offset_px is not re-clamped, first_visible_row() will be
        // larger than the renderer's actual first row, causing click misalignment.
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows

        // Scroll to near-max for a 100-row buffer:
        // max_offset_px = (100 - 10) * 16 = 1440 pixels
        scroller.scroll_to(90, 100); // scroll_offset_px = 1440.0
        assert_eq!(scroller.first_visible_row(), 90);
        assert!((scroller.scroll_offset_px() - 1440.0).abs() < 0.001);

        // Now simulate a resize that INCREASES viewport height (e.g., going fullscreen).
        // 320px / 16px = 20 visible rows
        // New max_offset_px = (100 - 20) * 16 = 1280 pixels
        scroller.update_size(320.0, 100);

        // After resize, scroll_offset_px should be clamped to new max (1280)
        assert!((scroller.scroll_offset_px() - 1280.0).abs() < 0.001);
        // And first_visible_row() should reflect the clamped position
        assert_eq!(scroller.first_visible_row(), 80); // 1280 / 16 = 80
        assert_eq!(scroller.visible_rows(), 20);
    }

    // =========================================================================
    // Chunk: docs/chunks/find_strip_scroll_clearance - ensure_visible_with_margin tests
    // =========================================================================

    #[test]
    fn test_ensure_visible_with_margin_zero_margin_same_as_ensure_visible() {
        // With margin=0, ensure_visible_with_margin should behave identically to ensure_visible
        let mut scroller1 = RowScroller::new(16.0);
        let mut scroller2 = RowScroller::new(16.0);
        scroller1.update_size(160.0, 100); // 10 visible rows
        scroller2.update_size(160.0, 100);

        // Test scrolling down
        let scrolled1 = scroller1.ensure_visible(25, 100);
        let scrolled2 = scroller2.ensure_visible_with_margin(25, 100, 0);

        assert_eq!(scrolled1, scrolled2);
        assert_eq!(scroller1.first_visible_row(), scroller2.first_visible_row());
        assert!((scroller1.scroll_offset_px() - scroller2.scroll_offset_px()).abs() < 0.001);
    }

    #[test]
    fn test_ensure_visible_with_margin_scrolls_earlier() {
        // With margin=1, a row that would be at position 9 (last visible) should trigger scroll
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows, showing 0..10

        // Row 9 would be at the last visible position without margin
        // With margin=1, effective visible rows = 9, so row 9 is beyond effective viewport
        let scrolled = scroller.ensure_visible_with_margin(9, 100, 1);

        // Should have scrolled to put row 9 at effective bottom (position 8)
        // new_row = 9 - (9 - 1) = 1
        assert!(scrolled);
        assert_eq!(scroller.first_visible_row(), 1);
    }

    #[test]
    fn test_ensure_visible_with_margin_row_above_effective_bottom() {
        // Row that's above the effective bottom should not trigger scroll
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows

        // Row 8 is at position 8 (within effective 9-row viewport with margin=1)
        let scrolled = scroller.ensure_visible_with_margin(8, 100, 1);
        assert!(!scrolled);
        assert_eq!(scroller.first_visible_row(), 0);
    }

    #[test]
    fn test_ensure_visible_with_margin_larger_margin() {
        // Test with margin=2
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows

        // With margin=2, effective visible = 8
        // Row 8 should trigger scroll (it's >= first + 8)
        let scrolled = scroller.ensure_visible_with_margin(8, 100, 2);

        assert!(scrolled);
        // new_row = 8 - (8 - 1) = 1
        assert_eq!(scroller.first_visible_row(), 1);
    }

    #[test]
    fn test_ensure_visible_with_margin_clamps_to_min_one() {
        // With a very small viewport (2 rows) and margin=2, effective should be clamped to 1
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(32.0, 100); // 2 visible rows

        // Row 1 should trigger scroll because effective visible = max(2-2, 1) = 1
        let scrolled = scroller.ensure_visible_with_margin(1, 100, 2);

        assert!(scrolled);
        // new_row = 1 - (1 - 1) = 1
        assert_eq!(scroller.first_visible_row(), 1);
    }

    #[test]
    fn test_ensure_visible_with_margin_scroll_up_unaffected() {
        // Scrolling up should not be affected by bottom margin
        let mut scroller = RowScroller::new(16.0);
        scroller.update_size(160.0, 100); // 10 visible rows
        scroller.scroll_to(20, 100);

        // Row 10 is above viewport - should scroll up to put it at top
        let scrolled = scroller.ensure_visible_with_margin(10, 100, 1);

        assert!(scrolled);
        assert_eq!(scroller.first_visible_row(), 10);
    }
}
