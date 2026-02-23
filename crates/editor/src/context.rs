// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/line_wrap_rendering - Wrap layout for hit-testing
//!
//! Editor context providing mutable access to core state.
//!
//! Focus targets mutate state through this context. It provides access to
//! the buffer, viewport, dirty region accumulator, and font metrics for
//! pixel-to-position conversion. With line wrapping, the context also includes
//! the viewport width for creating WrapLayout instances for hit-testing.

use crate::dirty_region::DirtyRegion;
use crate::font::FontMetrics;
use crate::viewport::Viewport;
use crate::wrap_layout::WrapLayout;
use lite_edit_buffer::{DirtyLines, TextBuffer};

// Chunk: docs/chunks/mouse_click_cursor - Font metrics (char_width, line_height) and view_height for pixel-to-position conversion
/// Context providing mutable access to editor state.
///
/// Focus targets receive this context in their `handle_*` methods and use it
/// to mutate the buffer, adjust the viewport, and accumulate dirty regions.
///
/// The context is borrowed for the duration of event handling, ensuring
/// safe mutable access to all components.
pub struct EditorContext<'a> {
    /// The text buffer being edited
    pub buffer: &'a mut TextBuffer,
    /// The viewport (scroll state, visible line range)
    pub viewport: &'a mut Viewport,
    /// Accumulated dirty region for this event batch
    pub dirty_region: &'a mut DirtyRegion,
    /// Font metrics for pixel-to-position conversion (char_width, line_height)
    pub font_metrics: FontMetrics,
    /// View height in pixels (for y-coordinate flipping)
    pub view_height: f32,
    /// Viewport width in pixels (for line wrapping calculations)
    pub view_width: f32,
}

impl<'a> EditorContext<'a> {
    /// Creates a new EditorContext from mutable references.
    ///
    /// # Arguments
    /// * `buffer` - The text buffer being edited
    /// * `viewport` - The viewport (scroll state, visible line range)
    /// * `dirty_region` - Accumulated dirty region for this event batch
    /// * `font_metrics` - Font metrics for pixel-to-position conversion
    /// * `view_height` - View height in pixels (for y-coordinate flipping)
    /// * `view_width` - View width in pixels (for line wrapping calculations)
    pub fn new(
        buffer: &'a mut TextBuffer,
        viewport: &'a mut Viewport,
        dirty_region: &'a mut DirtyRegion,
        font_metrics: FontMetrics,
        view_height: f32,
        view_width: f32,
    ) -> Self {
        Self {
            buffer,
            viewport,
            dirty_region,
            font_metrics,
            view_height,
            view_width,
        }
    }

    // Chunk: docs/chunks/line_wrap_rendering - Create WrapLayout for hit-testing
    /// Creates a WrapLayout for the current viewport width and font metrics.
    ///
    /// This is used by hit-testing code to convert screen positions to buffer positions.
    pub fn wrap_layout(&self) -> WrapLayout {
        WrapLayout::new(self.view_width, &self.font_metrics)
    }

    // Chunk: docs/chunks/dirty_region_wrap_aware - Wrap-aware dirty region conversion
    /// Marks lines as dirty, converting buffer-space DirtyLines to screen-space DirtyRegion.
    ///
    /// This uses wrap-aware conversion to correctly handle soft line wrapping,
    /// where buffer line indices can be much smaller than screen row indices.
    /// The method computes cumulative screen rows for each dirty buffer line
    /// and compares against the viewport's screen-row-based scroll position.
    ///
    /// This merges the new dirty region into the accumulated dirty region.
    pub fn mark_dirty(&mut self, dirty_lines: DirtyLines) {
        let line_count = self.buffer.line_count();
        let wrap_layout = self.wrap_layout();

        // Capture line lengths to avoid borrowing conflicts
        let line_lens: Vec<usize> = (0..line_count)
            .map(|line| self.buffer.line_len(line))
            .collect();

        let screen_dirty = self.viewport.dirty_lines_to_region_wrapped(
            &dirty_lines,
            line_count,
            &wrap_layout,
            |line| line_lens.get(line).copied().unwrap_or(0),
        );
        self.dirty_region.merge(screen_dirty);
    }

    // Chunk: docs/chunks/line_wrap_rendering - Wrap-aware cursor visibility
    /// Ensures the cursor is visible, scrolling if necessary.
    ///
    /// With line wrapping, a buffer line may span multiple screen rows, so we need
    /// to ensure the specific screen row containing the cursor is visible.
    ///
    /// If scrolling occurs, marks the full viewport as dirty.
    pub fn ensure_cursor_visible(&mut self) {
        let cursor_pos = self.buffer.cursor_position();
        let line_count = self.buffer.line_count();
        let wrap_layout = self.wrap_layout();
        let first_visible_line = self.viewport.first_visible_line();

        // Capture line lengths to avoid borrowing conflicts
        let line_lens: Vec<usize> = (0..line_count)
            .map(|line| self.buffer.line_len(line))
            .collect();

        if self.viewport.ensure_visible_wrapped(
            cursor_pos.line,
            cursor_pos.col,
            first_visible_line,
            line_count,
            &wrap_layout,
            |line| line_lens.get(line).copied().unwrap_or(0),
        ) {
            // Viewport scrolled - mark full viewport dirty
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
    }

    /// Marks the cursor line as dirty (e.g., for cursor blink).
    pub fn mark_cursor_dirty(&mut self) {
        let cursor_line = self.buffer.cursor_position().line;
        self.mark_dirty(DirtyLines::Single(cursor_line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lite_edit_buffer::Position;

    /// Creates test font metrics with known values
    fn test_font_metrics() -> FontMetrics {
        FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        }
    }

    #[test]
    fn test_mark_dirty_single_line() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0, 100); // 10 visible lines
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
                800.0,
            );
            ctx.mark_dirty(DirtyLines::Single(0));
        }

        assert_eq!(dirty, DirtyRegion::Lines { from: 0, to: 1 });
    }

    #[test]
    fn test_mark_dirty_merges() {
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo\nbar");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0, 100);
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
                800.0,
            );
            ctx.mark_dirty(DirtyLines::Single(0));
            ctx.mark_dirty(DirtyLines::Single(2));
        }

        assert_eq!(dirty, DirtyRegion::Lines { from: 0, to: 3 });
    }

    #[test]
    fn test_ensure_cursor_visible_no_scroll() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0, 100);
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
                800.0,
            );
            ctx.ensure_cursor_visible();
        }

        // Cursor at (0, 0) is already visible - no scroll, no dirty
        assert_eq!(dirty, DirtyRegion::None);
    }

    #[test]
    fn test_ensure_cursor_visible_scrolls() {
        // Create a buffer with many lines
        let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        buffer.set_cursor(Position::new(45, 0)); // Move cursor near end

        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0, 100); // 10 visible lines
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
                800.0,
            );
            ctx.ensure_cursor_visible();
        }

        // Should have scrolled and marked full viewport dirty
        assert_eq!(dirty, DirtyRegion::FullViewport);
        assert!(viewport.first_visible_line() > 0);
    }

    #[test]
    fn test_mark_cursor_dirty() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        buffer.set_cursor(Position::new(1, 0));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0, 100);
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
                800.0,
            );
            ctx.mark_cursor_dirty();
        }

        assert_eq!(dirty, DirtyRegion::Lines { from: 1, to: 2 });
    }
}
