// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
//!
//! Viewport abstraction for mapping buffer coordinates to screen coordinates
//!
//! The viewport is the mapping layer between the text buffer and the screen.
//! It determines which buffer lines are visible on screen based on:
//! - `scroll_offset`: The first visible buffer line (0-indexed)
//! - `visible_lines`: How many lines fit in the viewport (computed from window height)
//!
//! The viewport converts buffer-space operations to screen-space:
//! - `visible_range()` returns which buffer lines are on screen
//! - `buffer_line_to_screen_line()` maps a buffer line to a screen line
//! - `dirty_lines_to_region()` converts buffer DirtyLines to screen DirtyRegion

use std::ops::Range;

use crate::dirty_region::DirtyRegion;
use lite_edit_buffer::DirtyLines;

/// A viewport representing the visible portion of a text buffer
#[derive(Debug, Clone)]
pub struct Viewport {
    /// First visible buffer line (0-indexed)
    pub scroll_offset: usize,
    /// Number of lines that fit in the viewport
    visible_lines: usize,
    /// Line height in pixels (used to compute visible_lines from window height)
    line_height: f32,
}

impl Viewport {
    /// Creates a new viewport with the given line height
    ///
    /// The viewport starts at scroll_offset 0 with 0 visible lines.
    /// Call `update_size()` to set the visible line count based on window height.
    pub fn new(line_height: f32) -> Self {
        Self {
            scroll_offset: 0,
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
    /// The range is `[scroll_offset, min(scroll_offset + visible_lines, buffer_line_count))`.
    /// If the buffer has fewer lines than would fill the viewport, the range ends at the buffer's end.
    pub fn visible_range(&self, buffer_line_count: usize) -> Range<usize> {
        let start = self.scroll_offset;
        let end = (self.scroll_offset + self.visible_lines).min(buffer_line_count);
        start..end
    }

    /// Scrolls the viewport to show the given buffer line at the top
    ///
    /// The scroll offset is clamped to valid bounds:
    /// - Minimum: 0
    /// - Maximum: max(0, buffer_line_count - visible_lines)
    ///
    /// This ensures the viewport doesn't scroll past the end of the buffer.
    pub fn scroll_to(&mut self, line: usize, buffer_line_count: usize) {
        let max_offset = buffer_line_count.saturating_sub(self.visible_lines);
        self.scroll_offset = line.min(max_offset);
    }

    /// Ensures a buffer line is visible, scrolling if necessary
    ///
    /// Returns `true` if scrolling occurred, `false` if the line was already visible.
    /// This is useful for keeping the cursor in view.
    pub fn ensure_visible(&mut self, line: usize, buffer_line_count: usize) -> bool {
        let old_offset = self.scroll_offset;

        if line < self.scroll_offset {
            // Line is above viewport - scroll up
            self.scroll_offset = line;
        } else if line >= self.scroll_offset + self.visible_lines {
            // Line is below viewport - scroll down
            // Put the line at the bottom of the viewport
            let new_offset = line.saturating_sub(self.visible_lines.saturating_sub(1));
            let max_offset = buffer_line_count.saturating_sub(self.visible_lines);
            self.scroll_offset = new_offset.min(max_offset);
        }

        self.scroll_offset != old_offset
    }

    /// Converts a buffer line index to a screen line index
    ///
    /// Returns `Some(screen_line)` if the buffer line is visible in the viewport,
    /// or `None` if the line is outside the viewport.
    pub fn buffer_line_to_screen_line(&self, buffer_line: usize) -> Option<usize> {
        if buffer_line >= self.scroll_offset
            && buffer_line < self.scroll_offset + self.visible_lines
        {
            Some(buffer_line - self.scroll_offset)
        } else {
            None
        }
    }

    /// Converts a screen line index to a buffer line index
    ///
    /// Returns the buffer line index corresponding to the given screen line.
    pub fn screen_line_to_buffer_line(&self, screen_line: usize) -> usize {
        self.scroll_offset + screen_line
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
        let visible_start = self.scroll_offset;
        let visible_end = (self.scroll_offset + self.visible_lines).min(buffer_line_count);

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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic construction ====================

    #[test]
    fn test_new() {
        let vp = Viewport::new(16.0);
        assert_eq!(vp.scroll_offset, 0);
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

    // ==================== visible_range ====================

    #[test]
    fn test_visible_range_at_start() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let range = vp.visible_range(100);
        assert_eq!(range, 0..10);
    }

    #[test]
    fn test_visible_range_scrolled() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 20;

        let range = vp.visible_range(100);
        assert_eq!(range, 20..30);
    }

    #[test]
    fn test_visible_range_at_end() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 95;

        let range = vp.visible_range(100);
        assert_eq!(range, 95..100);
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
        assert_eq!(vp.scroll_offset, 20);
    }

    #[test]
    fn test_scroll_to_clamped_to_max() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(95, 100);
        assert_eq!(vp.scroll_offset, 90); // max is 100 - 10 = 90
    }

    #[test]
    fn test_scroll_to_beyond_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(200, 100);
        assert_eq!(vp.scroll_offset, 90);
    }

    #[test]
    fn test_scroll_to_small_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        vp.scroll_to(5, 5); // buffer smaller than viewport
        assert_eq!(vp.scroll_offset, 0); // can't scroll at all
    }

    // ==================== ensure_visible ====================

    #[test]
    fn test_ensure_visible_already_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let scrolled = vp.ensure_visible(5, 100);
        assert!(!scrolled);
        assert_eq!(vp.scroll_offset, 0);
    }

    #[test]
    fn test_ensure_visible_scroll_up() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 20;

        let scrolled = vp.ensure_visible(10, 100);
        assert!(scrolled);
        assert_eq!(vp.scroll_offset, 10);
    }

    #[test]
    fn test_ensure_visible_scroll_down() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0); // 10 visible lines

        let scrolled = vp.ensure_visible(25, 100);
        assert!(scrolled);
        // Should put line 25 at the bottom: 25 - 9 = 16
        assert_eq!(vp.scroll_offset, 16);
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

    // ==================== buffer_line_to_screen_line ====================

    #[test]
    fn test_buffer_to_screen_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 10;

        assert_eq!(vp.buffer_line_to_screen_line(10), Some(0));
        assert_eq!(vp.buffer_line_to_screen_line(15), Some(5));
        assert_eq!(vp.buffer_line_to_screen_line(19), Some(9));
    }

    #[test]
    fn test_buffer_to_screen_above() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 10;

        assert_eq!(vp.buffer_line_to_screen_line(5), None);
        assert_eq!(vp.buffer_line_to_screen_line(9), None);
    }

    #[test]
    fn test_buffer_to_screen_below() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 10;

        assert_eq!(vp.buffer_line_to_screen_line(20), None);
        assert_eq!(vp.buffer_line_to_screen_line(100), None);
    }

    // ==================== screen_line_to_buffer_line ====================

    #[test]
    fn test_screen_to_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0);
        vp.scroll_offset = 10;

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
        vp.scroll_offset = 20;

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
        vp.scroll_offset = 5;

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
        vp.scroll_offset = 20;

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
        vp.scroll_offset = 20;

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
}
