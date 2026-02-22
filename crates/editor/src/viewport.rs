// Subsystem: docs/subsystems/viewport_scroll - Viewport mapping & scroll arithmetic
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/viewport_fractional_scroll - Fractional scroll support
// Chunk: docs/chunks/row_scroller_extract - Delegation to RowScroller
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
//!
//! Internally, `Viewport` delegates all uniform-row scroll arithmetic to a
//! `RowScroller`. The `Viewport`-only additions are buffer-specific methods:
//! - `dirty_lines_to_region()` — maps `DirtyLines` to `DirtyRegion`
//! - `ensure_visible_wrapped()` — handles soft-wrapped lines

use std::ops::Range;

use crate::dirty_region::DirtyRegion;
use crate::row_scroller::RowScroller;
use lite_edit_buffer::DirtyLines;

/// A viewport representing the visible portion of a text buffer
///
/// Internally delegates uniform-row scroll arithmetic to a `RowScroller`,
/// adding buffer-specific methods for dirty region mapping and soft-wrapped
/// line handling.
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Inner scroller that handles all uniform-row scroll arithmetic
    scroller: RowScroller,
}

impl Viewport {
    /// Creates a new viewport with the given line height
    ///
    /// The viewport starts at scroll_offset_px 0.0 with 0 visible lines.
    /// Call `update_size()` to set the visible line count based on window height.
    pub fn new(line_height: f32) -> Self {
        Self {
            scroller: RowScroller::new(line_height),
        }
    }

    /// Returns a reference to the inner `RowScroller`.
    ///
    /// This allows downstream code (e.g., `SelectorWidget`) to use `RowScroller`
    /// directly without going through `Viewport`.
    pub fn row_scroller(&self) -> &RowScroller {
        &self.scroller
    }

    /// Returns the line height in pixels
    pub fn line_height(&self) -> f32 {
        self.scroller.row_height()
    }

    /// Returns the number of visible lines in the viewport
    pub fn visible_lines(&self) -> usize {
        self.scroller.visible_rows()
    }

    /// Returns the first visible buffer line (derived from pixel offset)
    ///
    /// This is computed as `(scroll_offset_px / line_height).floor()`.
    /// The result is the integer line index used for buffer-to-screen mapping.
    ///
    /// **Note**: When soft line wrapping is enabled, use `first_visible_screen_row()`
    /// and `buffer_line_for_screen_row()` instead. This method assumes a 1:1 mapping
    /// between buffer lines and screen rows, which is only correct without wrapping.
    pub fn first_visible_line(&self) -> usize {
        self.scroller.first_visible_row()
    }

    // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Wrap-aware scroll position
    /// Returns the first visible screen row (derived from pixel offset)
    ///
    /// When soft line wrapping is enabled, scroll_offset_px tracks position in
    /// **screen row** space, not buffer line space. This method returns that value
    /// directly: `floor(scroll_offset_px / line_height)`.
    ///
    /// Use `buffer_line_for_screen_row()` to find which buffer line contains
    /// a given screen row.
    pub fn first_visible_screen_row(&self) -> usize {
        if self.line_height() <= 0.0 {
            return 0;
        }
        (self.scroll_offset_px() / self.line_height()).floor() as usize
    }

    // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Wrap-aware buffer line lookup
    /// Given a target screen row, finds which buffer line contains it.
    ///
    /// Returns `(buffer_line, screen_row_offset_within_line, cumulative_screen_rows_before_line)`:
    /// - `buffer_line`: The buffer line index that contains the target screen row
    /// - `screen_row_offset`: Which row within that buffer line (0 = first row of the line)
    /// - `cumulative_rows_before`: Total screen rows for all buffer lines before this one
    ///
    /// This is used to correctly map scroll positions in wrapped mode, where
    /// `scroll_offset_px` is set in screen row units by `ensure_visible_wrapped`.
    pub fn buffer_line_for_screen_row<F>(
        target_screen_row: usize,
        line_count: usize,
        wrap_layout: &crate::wrap_layout::WrapLayout,
        line_len_fn: F,
    ) -> (usize, usize, usize)
    where
        F: Fn(usize) -> usize,
    {
        let mut cumulative_screen_rows: usize = 0;

        for buffer_line in 0..line_count {
            let line_len = line_len_fn(buffer_line);
            let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

            // Check if target falls within this buffer line's screen rows
            if cumulative_screen_rows + rows_for_line > target_screen_row {
                let row_offset = target_screen_row - cumulative_screen_rows;
                return (buffer_line, row_offset, cumulative_screen_rows);
            }

            cumulative_screen_rows += rows_for_line;
        }

        // Target is past the end of the document
        let last_line = line_count.saturating_sub(1);
        (last_line, 0, cumulative_screen_rows)
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
        self.scroller.scroll_fraction_px()
    }

    /// Returns the raw scroll offset in pixels
    ///
    /// This is the authoritative scroll state. Use `first_visible_line()` for
    /// the derived integer line index used in buffer-to-screen mapping.
    pub fn scroll_offset_px(&self) -> f32 {
        self.scroller.scroll_offset_px()
    }

    /// Sets the scroll offset in pixels, with clamping to valid bounds
    ///
    /// The offset is clamped to `[0.0, max_offset_px]` where:
    /// `max_offset_px = (buffer_line_count - visible_lines) * line_height`
    ///
    /// This ensures the viewport doesn't scroll past the start or end of the document.
    pub fn set_scroll_offset_px(&mut self, px: f32, buffer_line_count: usize) {
        self.scroller.set_scroll_offset_px(px, buffer_line_count);
    }

    /// Updates the viewport size based on window height in pixels.
    ///
    /// This recomputes `visible_lines` = floor(window_height / line_height) and
    /// re-clamps the scroll offset to the new valid bounds.
    // Chunk: docs/chunks/resize_click_alignment - Re-clamp scroll offset on resize
    pub fn update_size(&mut self, window_height: f32, buffer_line_count: usize) {
        self.scroller.update_size(window_height, buffer_line_count);
    }

    /// Returns the range of buffer lines visible in the viewport
    ///
    /// The range is `[first_visible_line, min(first_visible_line + visible_lines, buffer_line_count))`.
    /// If the buffer has fewer lines than would fill the viewport, the range ends at the buffer's end.
    ///
    /// Note: When scrolled to a fractional position, this range includes the partially-visible
    /// top line. The renderer handles partial visibility via `scroll_fraction_px()`.
    pub fn visible_range(&self, buffer_line_count: usize) -> Range<usize> {
        self.scroller.visible_range(buffer_line_count)
    }

    /// Scrolls the viewport to show the given buffer line at the top
    ///
    /// The scroll offset is set to align the target line at the top of the viewport.
    /// The offset is clamped to valid pixel bounds to prevent scrolling past the
    /// end of the buffer.
    ///
    /// This snaps to a whole-line boundary (pixel offset is a multiple of line_height).
    pub fn scroll_to(&mut self, line: usize, buffer_line_count: usize) {
        self.scroller.scroll_to(line, buffer_line_count);
    }

    /// Ensures a buffer line is visible, scrolling if necessary
    ///
    /// Returns `true` if scrolling occurred, `false` if the line was already visible.
    /// This is useful for keeping the cursor in view.
    ///
    /// When scrolling is needed, this snaps to a whole-line boundary, ensuring
    /// clean alignment after cursor-following operations.
    pub fn ensure_visible(&mut self, line: usize, buffer_line_count: usize) -> bool {
        self.scroller.ensure_visible(line, buffer_line_count)
    }

    // Chunk: docs/chunks/find_strip_scroll_clearance - Margin support for overlays
    /// Ensures a buffer line is visible, with additional bottom margin.
    ///
    /// Like `ensure_visible`, but treats the viewport as if it had
    /// `bottom_margin_lines` fewer rows at the bottom. This is useful when
    /// an overlay (like the find strip) occludes the bottom of the viewport.
    ///
    /// When scrolling is needed, this snaps to a whole-line boundary.
    ///
    /// Returns `true` if scrolling occurred, `false` if the line was already visible.
    pub fn ensure_visible_with_margin(
        &mut self,
        line: usize,
        buffer_line_count: usize,
        bottom_margin_lines: usize,
    ) -> bool {
        self.scroller
            .ensure_visible_with_margin(line, buffer_line_count, bottom_margin_lines)
    }

    // Chunk: docs/chunks/line_wrap_rendering - Wrap-aware cursor visibility
    /// Ensures a cursor position is visible with soft line wrapping.
    ///
    /// This accounts for the fact that a buffer line may wrap to multiple screen rows.
    /// We need to ensure the specific screen row containing the cursor column is visible.
    ///
    /// Returns `true` if scrolling occurred, `false` if the cursor was already visible.
    ///
    /// # Arguments
    /// * `cursor_line` - The buffer line containing the cursor
    /// * `cursor_col` - The buffer column of the cursor
    /// * `first_visible_line` - The first visible buffer line
    /// * `line_count` - Total number of buffer lines
    /// * `wrap_layout` - The wrap layout for calculating screen rows
    /// * `line_len_fn` - Closure to get the character count of a buffer line
    pub fn ensure_visible_wrapped<F>(
        &mut self,
        cursor_line: usize,
        cursor_col: usize,
        first_visible_line: usize,
        line_count: usize,
        wrap_layout: &crate::wrap_layout::WrapLayout,
        line_len_fn: F,
    ) -> bool
    where
        F: Fn(usize) -> usize,
    {
        let old_offset_px = self.scroll_offset_px();
        let line_height = self.line_height();
        let visible_lines = self.visible_lines();

        // Calculate the cumulative screen row of the cursor
        // We need to know: "what screen row (from viewport top) is the cursor on?"
        let mut cumulative_screen_row: usize = 0;

        // First, calculate screen rows for lines before the cursor line
        for buffer_line in first_visible_line..cursor_line.min(line_count) {
            let line_len = line_len_fn(buffer_line);
            cumulative_screen_row += wrap_layout.screen_rows_for_line(line_len);
        }

        // Now add the row offset within the cursor's line
        let (cursor_row_offset, _) = wrap_layout.buffer_col_to_screen_pos(cursor_col);

        // If cursor is before first_visible_line, we need to scroll up
        if cursor_line < first_visible_line {
            // Calculate how many screen rows from the absolute start
            let mut abs_screen_row: usize = 0;
            for buffer_line in 0..cursor_line.min(line_count) {
                let line_len = line_len_fn(buffer_line);
                abs_screen_row += wrap_layout.screen_rows_for_line(line_len);
            }
            abs_screen_row += cursor_row_offset;

            // Scroll to put cursor's screen row at the top
            let target_px = abs_screen_row as f32 * line_height;
            // Use a reasonable max based on wrapping
            // For simplicity, use a large value; proper clamping happens in set_scroll_offset_px
            let max_screen_rows = self.compute_total_screen_rows(line_count, wrap_layout, &line_len_fn);
            let max_offset_px = max_screen_rows.saturating_sub(visible_lines) as f32 * line_height;
            self.set_scroll_offset_px_direct(target_px.clamp(0.0, max_offset_px));
        } else {
            // Cursor is at or after first_visible_line
            let cursor_screen_row = cumulative_screen_row + cursor_row_offset;

            if cursor_screen_row >= visible_lines {
                // Cursor is below viewport - scroll down
                // Put the cursor's screen row at the bottom of the viewport
                let new_top_row = cursor_screen_row.saturating_sub(visible_lines.saturating_sub(1));
                let target_px = new_top_row as f32 * line_height;
                let max_screen_rows = self.compute_total_screen_rows(line_count, wrap_layout, &line_len_fn);
                let max_offset_px = max_screen_rows.saturating_sub(visible_lines) as f32 * line_height;
                self.set_scroll_offset_px_direct(target_px.clamp(0.0, max_offset_px));
            }
            // else: cursor is visible, no scroll needed
        }

        self.scroll_offset_px() != old_offset_px
    }

    /// Sets scroll offset directly without clamping (for internal use in wrap handling)
    fn set_scroll_offset_px_direct(&mut self, px: f32) {
        // Access the inner scroller's field directly via a helper
        // This is needed for ensure_visible_wrapped which does its own clamping
        self.scroller.set_scroll_offset_unclamped(px);
    }

    // Chunk: docs/chunks/scroll_bottom_deadzone - Wrap-aware scroll clamping
    /// Sets the scroll offset in pixels, with clamping based on total screen rows.
    ///
    /// Unlike `set_scroll_offset_px` which clamps based on buffer line count,
    /// this method computes the maximum scroll position using total screen rows
    /// (accounting for wrapped lines). This ensures consistent scroll bounds
    /// when line wrapping is enabled.
    ///
    /// The offset is clamped to `[0.0, max_offset_px]` where:
    /// `max_offset_px = (total_screen_rows - visible_rows) * line_height`
    pub fn set_scroll_offset_px_wrapped<F>(
        &mut self,
        px: f32,
        line_count: usize,
        wrap_layout: &crate::wrap_layout::WrapLayout,
        line_len_fn: F,
    ) where
        F: Fn(usize) -> usize,
    {
        let total_screen_rows = self.compute_total_screen_rows(line_count, wrap_layout, &line_len_fn);
        let max_rows = total_screen_rows.saturating_sub(self.visible_lines());
        let max_offset_px = max_rows as f32 * self.line_height();
        self.scroller.set_scroll_offset_unclamped(px.clamp(0.0, max_offset_px));
    }

    /// Helper: computes total screen rows for all buffer lines
    fn compute_total_screen_rows<F>(
        &self,
        line_count: usize,
        wrap_layout: &crate::wrap_layout::WrapLayout,
        line_len_fn: F,
    ) -> usize
    where
        F: Fn(usize) -> usize,
    {
        let mut total = 0;
        for line in 0..line_count {
            total += wrap_layout.screen_rows_for_line(line_len_fn(line));
        }
        total
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
        self.scroller.row_to_visible_offset(buffer_line)
    }

    /// Converts a screen line index to a buffer line index
    ///
    /// Returns the buffer line index corresponding to the given screen line.
    pub fn screen_line_to_buffer_line(&self, screen_line: usize) -> usize {
        self.scroller.visible_offset_to_row(screen_line)
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
        let visible_end = (first_line + self.visible_lines()).min(buffer_line_count);

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
        assert_eq!(vp.scroll_offset_px(), 0.0);
        assert_eq!(vp.first_visible_line(), 0);
        assert_eq!(vp.visible_lines(), 0);
        assert_eq!(vp.line_height(), 16.0);
    }

    // ==================== update_size ====================

    #[test]
    fn test_update_size() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        assert_eq!(vp.visible_lines(), 10); // 160 / 16 = 10
    }

    #[test]
    fn test_update_size_fractional() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(170.0, 100); // 170 / 16 = 10.625
        assert_eq!(vp.visible_lines(), 10); // floor
    }

    #[test]
    fn test_update_size_zero_height() {
        let mut vp = Viewport::new(0.0);
        vp.update_size(160.0, 100);
        assert_eq!(vp.visible_lines(), 0);
    }

    // ==================== Fractional scroll tests ====================

    #[test]
    fn test_sub_line_delta_accumulates_without_line_change() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

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
        vp.update_size(160.0, 100);

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
        vp.update_size(160.0, 100);

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
        vp.update_size(160.0, 100);

        // Try to scroll to negative pixels
        vp.set_scroll_offset_px(-10.0, 100);
        assert_eq!(vp.scroll_offset_px(), 0.0);
        assert_eq!(vp.first_visible_line(), 0);
    }

    #[test]
    fn test_clamping_at_end() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        // max offset = (100 - 10) * 16 = 1440 pixels
        vp.set_scroll_offset_px(2000.0, 100);
        assert!((vp.scroll_offset_px() - 1440.0).abs() < 0.001);
        assert_eq!(vp.first_visible_line(), 90);
    }

    #[test]
    fn test_scroll_offset_px_getter() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);

        vp.set_scroll_offset_px(123.5, 100);
        assert!((vp.scroll_offset_px() - 123.5).abs() < 0.001);
    }

    // ==================== visible_range ====================

    #[test]
    fn test_visible_range_at_start() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        let range = vp.visible_range(100);
        // With 10 visible lines, range should include an extra line for partial visibility
        assert_eq!(range, 0..11);
    }

    #[test]
    fn test_visible_range_scrolled() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(20, 100); // Sets scroll_offset_px to 320.0

        let range = vp.visible_range(100);
        assert_eq!(range, 20..31); // Includes extra line for partial visibility
    }

    #[test]
    fn test_visible_range_at_end() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        // Scroll to line 95 - but clamped to max of 90
        vp.scroll_to(95, 100);

        let range = vp.visible_range(100);
        // Clamped to 90, so range is 90..100 (capped at buffer end)
        assert_eq!(range, 90..100);
    }

    #[test]
    fn test_visible_range_buffer_smaller_than_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        let range = vp.visible_range(5);
        assert_eq!(range, 0..5);
    }

    // ==================== scroll_to ====================

    #[test]
    fn test_scroll_to_valid() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        vp.scroll_to(20, 100);
        assert_eq!(vp.first_visible_line(), 20);
        assert!((vp.scroll_offset_px() - 320.0).abs() < 0.001); // 20 * 16
    }

    #[test]
    fn test_scroll_to_clamped_to_max() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        vp.scroll_to(95, 100);
        assert_eq!(vp.first_visible_line(), 90); // max is 100 - 10 = 90
    }

    #[test]
    fn test_scroll_to_beyond_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        vp.scroll_to(200, 100);
        assert_eq!(vp.first_visible_line(), 90);
    }

    #[test]
    fn test_scroll_to_small_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        vp.scroll_to(5, 5); // buffer smaller than viewport
        assert_eq!(vp.first_visible_line(), 0); // can't scroll at all
    }

    // ==================== ensure_visible ====================

    #[test]
    fn test_ensure_visible_already_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        let scrolled = vp.ensure_visible(5, 100);
        assert!(!scrolled);
        assert_eq!(vp.first_visible_line(), 0);
    }

    #[test]
    fn test_ensure_visible_scroll_up() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(20, 100);

        let scrolled = vp.ensure_visible(10, 100);
        assert!(scrolled);
        assert_eq!(vp.first_visible_line(), 10);
    }

    #[test]
    fn test_ensure_visible_scroll_down() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        let scrolled = vp.ensure_visible(25, 100);
        assert!(scrolled);
        // Should put line 25 at the bottom: 25 - 9 = 16
        assert_eq!(vp.first_visible_line(), 16);
    }

    #[test]
    fn test_ensure_visible_at_boundary() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines, showing 0..10

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
        vp.update_size(160.0, 100);

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
        vp.update_size(160.0, 100);
        vp.scroll_to(10, 100);

        assert_eq!(vp.buffer_line_to_screen_line(10), Some(0));
        assert_eq!(vp.buffer_line_to_screen_line(15), Some(5));
        assert_eq!(vp.buffer_line_to_screen_line(19), Some(9));
    }

    #[test]
    fn test_buffer_to_screen_above() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(10, 100);

        assert_eq!(vp.buffer_line_to_screen_line(5), None);
        assert_eq!(vp.buffer_line_to_screen_line(9), None);
    }

    #[test]
    fn test_buffer_to_screen_below() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(10, 100);

        // Line 21 is beyond visible_lines + 1 (for partial visibility)
        assert_eq!(vp.buffer_line_to_screen_line(22), None);
        assert_eq!(vp.buffer_line_to_screen_line(100), None);
    }

    // ==================== screen_line_to_buffer_line ====================

    #[test]
    fn test_screen_to_buffer() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(10, 100);

        assert_eq!(vp.screen_line_to_buffer_line(0), 10);
        assert_eq!(vp.screen_line_to_buffer_line(5), 15);
        assert_eq!(vp.screen_line_to_buffer_line(9), 19);
    }

    // ==================== dirty_lines_to_region ====================

    #[test]
    fn test_dirty_none() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::None, 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_single_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        let region = vp.dirty_lines_to_region(&DirtyLines::Single(5), 100);
        assert_eq!(region, DirtyRegion::Lines { from: 5, to: 6 });
    }

    #[test]
    fn test_dirty_single_above_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(20, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Single(5), 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_single_below_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        let region = vp.dirty_lines_to_region(&DirtyLines::Single(50), 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_range_fully_visible() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 3, to: 7 }, 100);
        assert_eq!(region, DirtyRegion::Lines { from: 3, to: 7 });
    }

    #[test]
    fn test_dirty_range_partial_above() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(5, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 3, to: 8 }, 100);
        assert_eq!(region, DirtyRegion::Lines { from: 0, to: 3 }); // 5..8 -> screen 0..3
    }

    #[test]
    fn test_dirty_range_partial_below() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // visible 0..10

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 5, to: 15 }, 100);
        assert_eq!(region, DirtyRegion::Lines { from: 5, to: 10 });
    }

    #[test]
    fn test_dirty_range_outside_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(20, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::Range { from: 5, to: 10 }, 100);
        assert_eq!(region, DirtyRegion::None);
    }

    #[test]
    fn test_dirty_from_line_to_end_inside_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // visible 0..10

        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(5), 100);
        assert_eq!(region, DirtyRegion::Lines { from: 5, to: 10 });
    }

    #[test]
    fn test_dirty_from_line_to_end_at_viewport_start() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);

        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(0), 100);
        assert_eq!(region, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_dirty_from_line_to_end_above_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(20, 100);

        // If dirty starts at line 5 and we're scrolled to 20, entire viewport is dirty
        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(5), 100);
        assert_eq!(region, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_dirty_from_line_to_end_below_viewport() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // visible 0..10

        let region = vp.dirty_lines_to_region(&DirtyLines::FromLineToEnd(50), 100);
        assert_eq!(region, DirtyRegion::None);
    }

    // ==================== scroll_offset compatibility ====================

    #[test]
    fn test_scroll_offset_compat() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);
        vp.scroll_to(20, 100);

        // scroll_offset() should return first_visible_line()
        assert_eq!(vp.scroll_offset(), 20);
        assert_eq!(vp.scroll_offset(), vp.first_visible_line());
    }

    // ==================== Wrap-aware scroll position tests ====================
    // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Tests for wrap-aware methods

    #[test]
    fn test_first_visible_screen_row() {
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);

        // Initially at 0
        assert_eq!(vp.first_visible_screen_row(), 0);

        // Set scroll to 3 screen rows (use large buffer line count for no clamping)
        vp.set_scroll_offset_px(48.0, 100); // 3 * 16
        assert_eq!(vp.first_visible_screen_row(), 3);

        // Test with fractional position
        vp.set_scroll_offset_px(50.0, 100); // 3 * 16 + 2
        assert_eq!(vp.first_visible_screen_row(), 3);
    }

    #[test]
    fn test_buffer_line_for_screen_row_no_wrapping() {
        // With no wrapping, each buffer line = 1 screen row
        // Line lengths are all <= wrap width
        let wrap_layout = crate::wrap_layout::WrapLayout::new(800.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        // 10 lines, each with 10 characters (fits in 100 cols)
        let line_lens = vec![10usize; 10];

        // Screen row 0 -> buffer line 0
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            0, 10, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 0);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 0);

        // Screen row 5 -> buffer line 5
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            5, 10, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 5);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 5);
    }

    #[test]
    fn test_buffer_line_for_screen_row_with_wrapping() {
        // 80 cols per screen row
        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        // Line lengths:
        // Line 0: 40 chars (1 screen row)
        // Line 1: 150 chars (2 screen rows)
        // Line 2: 200 chars (3 screen rows)
        // Line 3: 50 chars (1 screen row)
        let line_lens = vec![40, 150, 200, 50];

        // Screen row 0 -> buffer line 0, row offset 0
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            0, 4, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 0);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 0);

        // Screen row 1 -> buffer line 1, row offset 0 (first row of wrapped line)
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            1, 4, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 1);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 1);

        // Screen row 2 -> buffer line 1, row offset 1 (second row of wrapped line)
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            2, 4, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 1);
        assert_eq!(row_off, 1);
        assert_eq!(cumulative, 1);

        // Screen row 3 -> buffer line 2, row offset 0
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            3, 4, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 2);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 3);

        // Screen row 5 -> buffer line 2, row offset 2 (third row of line 2)
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            5, 4, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 2);
        assert_eq!(row_off, 2);
        assert_eq!(cumulative, 3);

        // Screen row 6 -> buffer line 3, row offset 0
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            6, 4, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 3);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 6);
    }

    #[test]
    fn test_buffer_line_for_screen_row_past_end() {
        let wrap_layout = crate::wrap_layout::WrapLayout::new(800.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let line_lens = vec![10, 10, 10]; // 3 lines, 1 screen row each

        // Screen row 10 is past end -> returns last buffer line
        let (buf_line, row_off, _) = Viewport::buffer_line_for_screen_row(
            10, 3, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 2); // Last buffer line
        assert_eq!(row_off, 0);
    }

    #[test]
    fn test_cursor_on_unwrapped_line_with_wrapped_lines_above() {
        // This tests the scenario from the success criteria:
        // Cursor on a line that does not wrap, with wrapped lines above it.
        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        // Line 0: 200 chars (3 screen rows) - wrapped line above cursor
        // Line 1: 50 chars (1 screen row) - cursor is here, unwrapped
        let line_lens = vec![200, 50];

        // If scroll is at screen row 0, the cursor on line 1 should be at screen row 3
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            3, 2, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 1);
        assert_eq!(row_off, 0);
        assert_eq!(cumulative, 3);
    }

    #[test]
    fn test_cursor_on_continuation_row() {
        // This tests the scenario from the success criteria:
        // Cursor on the continuation row (second or later screen row) of a wrapped buffer line.
        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        // Line 0: 200 chars (3 screen rows) - cursor could be on row 1 or 2 within this line
        let line_lens = vec![200];

        // Screen row 1 is the second row of line 0
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            1, 1, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 0);
        assert_eq!(row_off, 1); // This is the continuation row
        assert_eq!(cumulative, 0);

        // Screen row 2 is the third row of line 0
        let (buf_line, row_off, cumulative) = Viewport::buffer_line_for_screen_row(
            2, 1, &wrap_layout, |line| line_lens[line]
        );
        assert_eq!(buf_line, 0);
        assert_eq!(row_off, 2); // This is also a continuation row
        assert_eq!(cumulative, 0);
    }

    // =========================================================================
    // Chunk: docs/chunks/find_strip_scroll_clearance - ensure_visible_with_margin tests
    // =========================================================================

    #[test]
    fn test_ensure_visible_with_margin_delegates_to_scroller() {
        // Verify that Viewport.ensure_visible_with_margin delegates properly to RowScroller
        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible lines

        // With margin=1, line 9 should trigger scroll
        let scrolled = vp.ensure_visible_with_margin(9, 100, 1);
        assert!(scrolled);
        assert_eq!(vp.first_visible_line(), 1);
    }

    #[test]
    fn test_ensure_visible_with_margin_zero_same_as_ensure_visible() {
        let mut vp1 = Viewport::new(16.0);
        let mut vp2 = Viewport::new(16.0);
        vp1.update_size(160.0, 100);
        vp2.update_size(160.0, 100);

        let scrolled1 = vp1.ensure_visible(15, 100);
        let scrolled2 = vp2.ensure_visible_with_margin(15, 100, 0);

        assert_eq!(scrolled1, scrolled2);
        assert_eq!(vp1.first_visible_line(), vp2.first_visible_line());
    }

    // =========================================================================
    // Chunk: docs/chunks/scroll_bottom_deadzone - Wrap-aware scroll clamping tests
    // =========================================================================

    #[test]
    fn test_set_scroll_offset_px_wrapped_clamps_to_screen_rows() {
        // Test that set_scroll_offset_px_wrapped clamps based on total screen rows,
        // not buffer line count.
        //
        // Scenario: 5 buffer lines, where some wrap to multiple screen rows.
        // Total screen rows > buffer lines, so max_offset_px should be larger
        // than what set_scroll_offset_px would compute.

        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100); // 10 visible rows

        // Line lengths:
        // Line 0: 40 chars (1 screen row)
        // Line 1: 160 chars (2 screen rows at 80 cols)
        // Line 2: 240 chars (3 screen rows)
        // Line 3: 160 chars (2 screen rows)
        // Line 4: 40 chars (1 screen row)
        // Total: 5 buffer lines, 9 screen rows
        let line_lens = vec![40, 160, 240, 160, 40];

        // With 10 visible rows and only 9 total screen rows, max_offset should be 0
        // (can't scroll at all when content fits in viewport)
        vp.set_scroll_offset_px_wrapped(100.0, 5, &wrap_layout, |line| line_lens[line]);
        assert_eq!(vp.scroll_offset_px(), 0.0);
    }

    #[test]
    fn test_set_scroll_offset_px_wrapped_allows_scroll_for_large_content() {
        // Test that when content has more screen rows than viewport,
        // scrolling is allowed up to the correct max.

        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let mut vp = Viewport::new(16.0);
        vp.update_size(80.0, 100); // 5 visible rows

        // 10 buffer lines, each 160 chars (2 screen rows at 80 cols)
        // Total: 20 screen rows
        let line_lens = vec![160; 10];

        // max_offset_px = (20 - 5) * 16 = 240
        vp.set_scroll_offset_px_wrapped(300.0, 10, &wrap_layout, |line| line_lens[line]);
        assert!((vp.scroll_offset_px() - 240.0).abs() < 0.001);
    }

    #[test]
    fn test_scroll_at_max_wrapped_responds_immediately() {
        // This is the key regression test for the scroll deadzone bug.
        //
        // Scenario: Scroll to max position using wrap-aware clamping, then
        // scroll back up by 1px. The offset should decrease immediately.

        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let mut vp = Viewport::new(16.0);
        vp.update_size(80.0, 100); // 5 visible rows

        // 10 buffer lines, each 160 chars (2 screen rows at 80 cols)
        // Total: 20 screen rows, max_offset = (20 - 5) * 16 = 240
        let line_lens = vec![160; 10];

        // Scroll to max
        vp.set_scroll_offset_px_wrapped(1000.0, 10, &wrap_layout, |line| line_lens[line]);
        let max_offset = vp.scroll_offset_px();
        assert!((max_offset - 240.0).abs() < 0.001);

        // Now scroll back up by 1px
        let new_offset = max_offset - 1.0;
        vp.set_scroll_offset_px_wrapped(new_offset, 10, &wrap_layout, |line| line_lens[line]);

        // Should respond immediately - offset should be 239.0
        assert!(
            (vp.scroll_offset_px() - 239.0).abs() < 0.001,
            "Expected offset 239.0, got {}. Scroll should respond immediately at max position.",
            vp.scroll_offset_px()
        );
    }

    #[test]
    fn test_set_scroll_offset_px_wrapped_no_wrapping_matches_buffer_lines() {
        // When no lines wrap, total_screen_rows == buffer_lines,
        // so the behavior should match set_scroll_offset_px.

        let wrap_layout = crate::wrap_layout::WrapLayout::new(800.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let mut vp1 = Viewport::new(16.0);
        let mut vp2 = Viewport::new(16.0);
        vp1.update_size(160.0, 100); // 10 visible rows
        vp2.update_size(160.0, 100);

        // 20 buffer lines, each 40 chars (fits in 100 cols, no wrapping)
        let line_lens = vec![40; 20];

        // Test that both methods clamp to the same max
        vp1.set_scroll_offset_px(1000.0, 20);
        vp2.set_scroll_offset_px_wrapped(1000.0, 20, &wrap_layout, |line| line_lens[line]);

        assert!(
            (vp1.scroll_offset_px() - vp2.scroll_offset_px()).abs() < 0.001,
            "Without wrapping, both methods should clamp to same max. Got {} vs {}",
            vp1.scroll_offset_px(),
            vp2.scroll_offset_px()
        );
    }

    #[test]
    fn test_set_scroll_offset_px_wrapped_clamps_negative_to_zero() {
        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let mut vp = Viewport::new(16.0);
        vp.update_size(160.0, 100);

        let line_lens = vec![160; 10];

        vp.set_scroll_offset_px_wrapped(-100.0, 10, &wrap_layout, |line| line_lens[line]);
        assert_eq!(vp.scroll_offset_px(), 0.0);
    }

    #[test]
    fn test_wrapped_max_offset_greater_than_unwrapped() {
        // Verify that with wrapped content, the max scroll position
        // is greater than what buffer-line-based clamping would allow.

        let wrap_layout = crate::wrap_layout::WrapLayout::new(640.0, &crate::font::FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        });

        let mut vp_wrapped = Viewport::new(16.0);
        let mut vp_unwrapped = Viewport::new(16.0);
        vp_wrapped.update_size(80.0, 100); // 5 visible rows
        vp_unwrapped.update_size(80.0, 100);

        // 10 buffer lines, each 160 chars (2 screen rows at 80 cols)
        // Total screen rows: 20
        // Unwrapped max = (10 - 5) * 16 = 80
        // Wrapped max = (20 - 5) * 16 = 240
        let line_lens = vec![160; 10];

        vp_unwrapped.set_scroll_offset_px(1000.0, 10);
        vp_wrapped.set_scroll_offset_px_wrapped(1000.0, 10, &wrap_layout, |line| line_lens[line]);

        assert!(
            vp_wrapped.scroll_offset_px() > vp_unwrapped.scroll_offset_px(),
            "Wrapped content should allow more scrolling. Wrapped: {}, Unwrapped: {}",
            vp_wrapped.scroll_offset_px(),
            vp_unwrapped.scroll_offset_px()
        );
        assert!((vp_unwrapped.scroll_offset_px() - 80.0).abs() < 0.001);
        assert!((vp_wrapped.scroll_offset_px() - 240.0).abs() < 0.001);
    }
}
