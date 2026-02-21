// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/viewport_fractional_scroll - Fractional scroll support
//!
//! Viewport abstraction for mapping buffer coordinates to screen coordinates
//!
//! The viewport is the mapping layer between the text buffer and the screen.
//! It determines which buffer lines are visible on screen based on:
//! - `scroll_offset_px`: The scroll position in pixels (fractional for smooth scrolling)
//! - `visible_lines`: How many lines fit in the viewport (computed from window height)
//!
//! The viewport converts buffer-space operations to screen-space:
//! - `visible_range()` returns which buffer lines are on screen
//! - `buffer_line_to_screen_line()` maps a buffer line to a screen line
//! - `dirty_lines_to_region()` converts buffer DirtyLines to screen DirtyRegion
//!
//! The scroll position is tracked as floating-point pixels internally, enabling
//! smooth trackpad scrolling. The integer line index (`first_visible_line()`) is
//! derived as `(scroll_offset_px / line_height).floor()`, and the fractional
//! remainder (`scroll_fraction_px()`) is applied as a Y translation in the renderer.

use std::ops::Range;

use crate::dirty_region::DirtyRegion;
use lite_edit_buffer::DirtyLines;

/// A viewport representing the visible portion of a text buffer
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Scroll position in pixels (private - use accessor methods)
    /// This is the distance from the top of the document to the top of the viewport.
    scroll_offset_px: f32,
    /// Number of lines that fit in the viewport
    visible_lines: usize,
    /// Line height in pixels (used to compute visible_lines from window height)
    line_height: f32,
}

impl Viewport {
    /// Creates a new viewport with the given line height
    ///
    /// The viewport starts at scroll_offset_px 0.0 with 0 visible lines.
    /// Call `update_size()` to set the visible line count based on window height.
    pub fn new(line_height: f32) -> Self {
        Self {
            scroll_offset_px: 0.0,
            visible_lines: 0,
            line_height,
        }
    }

    /// Returns the line height in pixels
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Returns the number of visible lines in the viewport
    pub fn visible_lines(&self) -> usize {
        self.visible_lines
    }

    /// Returns the first visible buffer line (derived from pixel offset)
    ///
    /// This is computed as `(scroll_offset_px / line_height).floor()`.
    /// The result is the integer line index used for buffer-to-screen mapping.
    pub fn first_visible_line(&self) -> usize {
        if self.line_height <= 0.0 {
            return 0;
        }
        (self.scroll_offset_px / self.line_height).floor() as usize
    }

    /// Returns the fractional pixel remainder of the scroll position
    ///
    /// This is the number of pixels the viewport is scrolled past the start
    /// of `first_visible_line()`. The renderer uses this to offset all drawn
    /// lines by `-scroll_fraction_px()` in Y, causing the top line to be
    /// partially clipped and content to scroll smoothly between line positions.
    ///
    /// Returns a value in the range `[0.0, line_height)`.
    pub fn scroll_fraction_px(&self) -> f32 {
        if self.line_height <= 0.0 {
            return 0.0;
        }
        self.scroll_offset_px % self.line_height
    }

    /// Returns the raw scroll offset in pixels
    ///
    /// This is the authoritative scroll state. Use `first_visible_line()` for
    /// the derived integer line index used in buffer-to-screen mapping.
    pub fn scroll_offset_px(&self) -> f32 {
        self.scroll_offset_px
    }

    /// Sets the scroll offset in pixels, with clamping to valid bounds
    ///
    /// The offset is clamped to `[0.0, max_offset_px]` where:
    /// `max_offset_px = (buffer_line_count - visible_lines) * line_height`
    ///
    /// This ensures the viewport doesn't scroll past the start or end of the document.
    pub fn set_scroll_offset_px(&mut self, px: f32, buffer_line_count: usize) {
        let max_lines = buffer_line_count.saturating_sub(self.visible_lines);
        let max_offset_px = max_lines as f32 * self.line_height;
        self.scroll_offset_px = px.clamp(0.0, max_offset_px);
    }

    /// Updates the viewport size based on window height in pixels
    ///
    /// This recomputes `visible_lines` = floor(window_height / line_height).
    pub fn update_size(&mut self, window_height: f32) {
        self.visible_lines = if self.line_height > 0.0 {
            (window_height / self.line_height).floor() as usize
        } else {
            0
        };
    }

    /// Returns the range of buffer lines visible in the viewport
    ///
    /// The range is `[first_visible_line, min(first_visible_line + visible_lines, buffer_line_count))`.
    /// If the buffer has fewer lines than would fill the viewport, the range ends at the buffer's end.
    ///
    /// Note: When scrolled to a fractional position, this range includes the partially-visible
    /// top line. The renderer handles partial visibility via `scroll_fraction_px()`.
    pub fn visible_range(&self, buffer_line_count: usize) -> Range<usize> {
        let first_line = self.first_visible_line();
        let start = first_line;
        // Add 1 to visible_lines to account for partially visible line at bottom
        // when scrolled to a fractional position
        let end = (first_line + self.visible_lines + 1).min(buffer_line_count);
        start..end
    }

    /// Scrolls the viewport to show the given buffer line at the top
    ///
    /// The scroll offset is set to align the target line at the top of the viewport.
    /// The offset is clamped to valid pixel bounds to prevent scrolling past the
    /// end of the buffer.
    ///
    /// This snaps to a whole-line boundary (pixel offset is a multiple of line_height).
    pub fn scroll_to(&mut self, line: usize, buffer_line_count: usize) {
        let target_px = line as f32 * self.line_height;
        self.set_scroll_offset_px(target_px, buffer_line_count);
    }

    /// Ensures a buffer line is visible, scrolling if necessary
    ///
    /// Returns `true` if scrolling occurred, `false` if the line was already visible.
    /// This is useful for keeping the cursor in view.
    ///
    /// When scrolling is needed, this snaps to a whole-line boundary, ensuring
    /// clean alignment after cursor-following operations.
    pub fn ensure_visible(&mut self, line: usize, buffer_line_count: usize) -> bool {
        let old_offset_px = self.scroll_offset_px;
        let first_line = self.first_visible_line();

        if line < first_line {
            // Line is above viewport - scroll up to put line at top
            // Snap to whole-line boundary
            let target_px = line as f32 * self.line_height;
            self.set_scroll_offset_px(target_px, buffer_line_count);
        } else if line >= first_line + self.visible_lines {
            // Line is below viewport - scroll down
            // Put the line at the bottom of the viewport
            let new_line = line.saturating_sub(self.visible_lines.saturating_sub(1));
            // Snap to whole-line boundary
            let target_px = new_line as f32 * self.line_height;
            self.set_scroll_offset_px(target_px, buffer_line_count);
        }

        self.scroll_offset_px != old_offset_px
    }

    /// Converts a buffer line index to a screen line index
    ///
    /// Returns `Some(screen_line)` if the buffer line is visible in the viewport,
    /// or `None` if the line is outside the viewport.
    ///
    /// Note: This uses `first_visible_line()` for the offset, so the returned
    /// screen line is an integer index. The fractional offset is handled separately
    /// by the renderer via `scroll_fraction_px()`.
    pub fn buffer_line_to_screen_line(&self, buffer_line: usize) -> Option<usize> {
        let first_line = self.first_visible_line();
        // Use visible_lines + 1 to account for partially visible bottom line
        if buffer_line >= first_line && buffer_line < first_line + self.visible_lines + 1 {
            Some(buffer_line - first_line)
        } else {
            None
        }
    }

    /// Converts a screen line index to a buffer line index
    ///
    /// Returns the buffer line index corresponding to the given screen line.
    pub fn screen_line_to_buffer_line(&self, screen_line: usize) -> usize {
        self.first_visible_line() + screen_line
    }

    /// Converts buffer-space `DirtyLines` to screen-space `DirtyRegion`
    ///
    /// This maps dirty buffer lines to dirty screen lines, accounting for scroll offset.
    /// Lines outside the viewport produce `DirtyRegion::None`.
    /// `FromLineToEnd` that touches the visible range produces `FullViewport`.
    pub fn dirty_lines_to_region(
        &self,
        dirty: &DirtyLines,
        buffer_line_count: usize,
    ) -> DirtyRegion {
        let first_line = self.first_visible_line();
        let visible_start = first_line;
        let visible_end = (first_line + self.visible_lines).min(buffer_line_count);

        match dirty {
            DirtyLines::None => DirtyRegion::None,

            DirtyLines::Single(line) => {
                if *line >= visible_start && *line < visible_end {
                    DirtyRegion::single_line(*line - visible_start)
                } else {
                    DirtyRegion::None
                }
            }

            DirtyLines::Range { from, to } => {
                // Compute intersection with visible range
                let dirty_start = (*from).max(visible_start);
                let dirty_end = (*to).min(visible_end);

                if dirty_start < dirty_end {
                    DirtyRegion::line_range(
                        dirty_start - visible_start,
                        dirty_end - visible_start,
                    )
                } else {
                    DirtyRegion::None
                }
            }

            DirtyLines::FromLineToEnd(line) => {
                if *line < visible_end {
                    // The dirty region extends into (or past) the viewport
                    // We need to re-render from the start of the dirty area (or viewport start)
                    // to the end of the viewport
                    if *line <= visible_start {
                        // Dirty region covers the entire viewport
                        DirtyRegion::FullViewport
                    } else {
                        // Dirty region starts within the viewport
                        let screen_start = *line - visible_start;
                        let screen_end = visible_end - visible_start;
                        if screen_start == 0 {
                            DirtyRegion::FullViewport
                        } else {
                            DirtyRegion::line_range(screen_start, screen_end)
                        }
                    }
                } else {
                    // Dirty region is entirely below the viewport
                    DirtyRegion::None
                }
            }
        }
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(16.0) // Sensible default line height
    }
}

// Backwards compatibility: provide a way to get/set scroll_offset as usize for
// code that doesn't need sub-pixel precision.
impl Viewport {
    /// Returns the first visible line (for backwards compatibility)
    ///
    /// This is an alias for `first_visible_line()`. New code should use
    /// `first_visible_line()` directly to make the fractional scroll semantics clear.
    #[inline]
    pub fn scroll_offset(&self) -> usize {
        self.first_visible_line()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic construction ====================

    #[test]
    fn test_new() {
        let vp = Viewport::new(16.0);
        assert_eq!(vp.scroll_offset_px, 0.0);
        assert_eq!(vp.first_visible_line(), 0);
        assert_eq!(vp.visible_lines, 0);
        assert_eq!(vp.line_height, 16.0);
    }

    // ==================== update_size ====================

    #[test]
    fn test_update_size() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        assert_eq!(vp.visible_lines, 10); // 160 / 16 = 10
    }

    #[test]
    fn test_update_size_fractional() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(170.0); // 170 / 16 = 10.625
        assert_eq!(vp.visible_lines, 10); // floor
    }

    #[test]
    fn test_update_size_zero_height() {
        let mut vp = Viewport::new(0.0);
        vp.update_size(160.0);
        assert_eq!(vp.visible_lines, 0);
    }

    // ==================== Fractional scroll tests ====================

    #[test]
    fn test_sub_line_delta_accumulates_without_line_change() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        // Scroll 5 pixels (less than line_height of 16)
        vp.set_scroll_offset_px(5.0, 100);
        assert_eq!(vp.first_visible_line(), 0); // Still on line 0
        assert!((vp.scroll_fraction_px() - 5.0).abs() < 0.001);

        // Scroll another 5 pixels (total 10)
        vp.set_scroll_offset_px(10.0, 100);
        assert_eq!(vp.first_visible_line(), 0); // Still on line 0
        assert!((vp.scroll_fraction_px() - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_crossing_line_boundary_advances_first_visible_line() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        // Scroll exactly one line worth of pixels
        vp.set_scroll_offset_px(16.0, 100);
        assert_eq!(vp.first_visible_line(), 1);
        assert!((vp.scroll_fraction_px() - 0.0).abs() < 0.001);

        // Scroll a bit more
        vp.set_scroll_offset_px(20.0, 100);
        assert_eq!(vp.first_visible_line(), 1);
        assert!((vp.scroll_fraction_px() - 4.0).abs() < 0.001);

        // Cross to line 2
        vp.set_scroll_offset_px(32.0, 100);
        assert_eq!(vp.first_visible_line(), 2);
        assert!((vp.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_fractional_remainder_correct_after_accumulated_deltas() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        // Simulate accumulated deltas
        vp.set_scroll_offset_px(50.0, 100); // 50 = 3 * 16 + 2
        assert_eq!(vp.first_visible_line(), 3);
        assert!((vp.scroll_fraction_px() - 2.0).abs() < 0.001);

        vp.set_scroll_offset_px(99.0, 100); // 99 = 6 * 16 + 3
        assert_eq!(vp.first_visible_line(), 6);
        assert!((vp.scroll_fraction_px() - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_clamping_at_start() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        // Try to scroll to negative pixels
        vp.set_scroll_offset_px(-10.0, 100);
        assert_eq!(vp.scroll_offset_px(), 0.0);
        assert_eq!(vp.first_visible_line(), 0);
    }

    #[test]
    fn test_clamping_at_end() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        // max offset = (100 - 10) * 16 = 1440 pixels
        vp.set_scroll_offset_px(2000.0, 100);
        assert!((vp.scroll_offset_px() - 1440.0).abs() < 0.001);
        assert_eq!(vp.first_visible_line(), 90);
    }

    #[test]
    fn test_scroll_offset_px_getter() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        vp.set_scroll_offset_px(123.5, 100);
        assert!((vp.scroll_offset_px() - 123.5).abs() < 0.001);
    }

    // ==================== visible_range ====================

    #[test]
    fn test_visible_range_at_start() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let range = vp.visible_range(100);
        // With 10 visible lines, range should include an extra line for partial visibility
        assert_eq!(range, 0..11);
    }

    #[test]
    fn test_visible_range_scrolled() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(20, 100); // Sets scroll_offset_px to 320.0

        let range = vp.visible_range(100);
        assert_eq!(range, 20..31); // Includes extra line for partial visibility
    }

    #[test]
    fn test_visible_range_at_end() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        // Scroll to line 95 - but clamped to max of 90
        vp.scroll_to(95, 100);

        let range = vp.visible_range(100);
        // Clamped to 90, so range is 90..100 (capped at buffer end)
        assert_eq!(range, 90..100);
    }

    #[test]
    fn test_visible_range_buffer_smaller_than_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let range = vp.visible_range(5);
        assert_eq!(range, 0..5);
    }

    // ==================== scroll_to ====================

    #[test]
    fn test_scroll_to_valid() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(20, 100);
        assert_eq!(vp.first_visible_line(), 20);
        assert!((vp.scroll_offset_px() - 320.0).abs() < 0.001); // 20 * 16
    }

    #[test]
    fn test_scroll_to_clamped_to_max() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(95, 100);
        assert_eq!(vp.first_visible_line(), 90); // max is 100 - 10 = 90
    }

    #[test]
    fn test_scroll_to_beyond_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(200, 100);
        assert_eq!(vp.first_visible_line(), 90);
    }

    #[test]
    fn test_scroll_to_small_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(5, 5); // buffer smaller than viewport
        assert_eq!(vp.first_visible_line(), 0); // can't scroll at all
    }

    // ==================== ensure_visible ====================

    #[test]
    fn test_ensure_visible_already_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let scrolled = vp.ensure_visible(5, 100);
        assert!(!scrolled);
        assert_eq!(vp.first_visible_line(), 0);
    }

    #[test]
    fn test_ensure_visible_scroll_up() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(20, 100);

        let scrolled = vp.ensure_visible(10, 100);
        assert!(scrolled);
        assert_eq!(vp.first_visible_line(), 10);
    }

    #[test]
    fn test_ensure_visible_scroll_down() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let scrolled = vp.ensure_visible(25, 100);
        assert!(scrolled);
        // Should put line 25 at the bottom: 25 - 9 = 16
        assert_eq!(vp.first_visible_line(), 16);
    }

    #[test]
    fn test_ensure_visible_at_boundary() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines, showing 0..10

        // Line 9 is the last visible line (0-indexed)
        let scrolled = vp.ensure_visible(9, 100);
        assert!(!scrolled);

        // Line 10 is just beyond visible
        let scrolled = vp.ensure_visible(10, 100);
        assert!(scrolled);
    }

    #[test]
    fn test_ensure_visible_snaps_to_whole_line() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        // Start with fractional scroll position
        vp.set_scroll_offset_px(25.0, 100); // Line 1, fraction 9px
        assert_eq!(vp.first_visible_line(), 1);

        // ensure_visible should snap to whole-line boundary
        let scrolled = vp.ensure_visible(15, 100);
        assert!(scrolled);
        // After scrolling, should be on a clean line boundary
        assert!((vp.scroll_fraction_px() - 0.0).abs() < 0.001);
    }

    // ==================== buffer_line_to_screen_line ====================

    #[test]
    fn test_buffer_to_screen_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(10, 100);

        assert_eq!(vp.buffer_line_to_screen_line(10), Some(0));
        assert_eq!(vp.buffer_line_to_screen_line(15), Some(5));
        assert_eq!(vp.buffer_line_to_screen_line(19), Some(9));
    }

    #[test]
    fn test_buffer_to_screen_above() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(10, 100);

        assert_eq!(vp.buffer_line_to_screen_line(5), None);
        assert_eq!(vp.buffer_line_to_screen_line(9), None);
    }

    #[test]
    fn test_buffer_to_screen_below() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(10, 100);

        // Line 21 is beyond visible_lines + 1 (for partial visibility)
        assert_eq!(vp.buffer_line_to_screen_line(22), None);
        assert_eq!(vp.buffer_line_to_screen_line(100), None);
    }

    // ==================== screen_line_to_buffer_line ====================

    #[test]
    fn test_screen_to_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(10, 100);

        assert_eq!(vp.screen_line_to_buffer_line(0), 10);
        assert_eq!(vp.screen_line_to_buffer_line(5), 15);
        assert_eq!(vp.screen_line_to_buffer_line(9), 19);
    }

    // ==================== dirty_lines_to_region ====================

    #[test]
    fn test_dirty_none() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        let region = vp.dirty_lines_to_region(&DirtyLines::None, 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_single_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let region = vp.dirty_lines_to_region(&DirtyLines::Single(5), 100);
        assert_eq!(region, DirtyRegion::Lines { from: 5, to: 6 });
    }

    #[test]
    fn test_dirty_single_above_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(20, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Single(5), 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_single_below_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let region = vp.dirty_lines_to_region(&DirtyLines::Single(50), 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_range_fully_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 3, to: 7 }, 100);
        assert_eq!(region, DirtyRegion::Lines { from: 3, to: 7 });
    }

    #[test]
    fn test_dirty_range_partial_above() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(5, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 3, to: 8 }, 100);
        assert_eq!(region, DirtyRegion::Lines { from: 0, to: 3 }); // 5..8 -> screen 0..3
    }

    #[test]
    fn test_dirty_range_partial_below() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // visible 0..10

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 5, to: 15 }, 100);
        assert_eq!(region, DirtyRegion::Lines { from: 5, to: 10 });
    }

    #[test]
    fn test_dirty_range_outside_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(20, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 5, to: 10 }, 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_from_line_to_end_inside_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // visible 0..10

        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(5), 100);
        assert_eq!(region, DirtyRegion::Lines { from: 5, to: 10 });
    }

    #[test]
    fn test_dirty_from_line_to_end_at_viewport_start() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);

        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(0), 100);
        assert_eq!(region, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_dirty_from_line_to_end_above_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(20, 100);

        // If dirty starts at line 5 and we're scrolled to 20, entire viewport is dirty
        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(5), 100);
        assert_eq!(region, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_dirty_from_line_to_end_below_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // visible 0..10

        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(50), 100);
        assert_eq!(region, DirtyRegion::None);
    }

    // ==================== scroll_offset compatibility ====================

    #[test]
    fn test_scroll_offset_compat() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_to(20, 100);

        // scroll_offset() should return first_visible_line()
        assert_eq!(vp.scroll_offset(), 20);
        assert_eq!(vp.scroll_offset(), vp.first_visible_line());
    }
}
