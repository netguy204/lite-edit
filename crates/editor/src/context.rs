// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
//!
//! Editor context providing mutable access to core state.
//!
//! Focus targets mutate state through this context. It provides access to
//! the buffer, viewport, and dirty region accumulator.

use crate::dirty_region::DirtyRegion;
use crate::viewport::Viewport;
use lite_edit_buffer::{DirtyLines, TextBuffer};

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
}

impl<'a> EditorContext<'a> {
    /// Creates a new EditorContext from mutable references.
    pub fn new(
        buffer: &'a mut TextBuffer,
        viewport: &'a mut Viewport,
        dirty_region: &'a mut DirtyRegion,
    ) -> Self {
        Self {
            buffer,
            viewport,
            dirty_region,
        }
    }

    /// Marks lines as dirty, converting buffer-space DirtyLines to screen-space DirtyRegion.
    ///
    /// This merges the new dirty region into the accumulated dirty region.
    pub fn mark_dirty(&mut self, dirty_lines: DirtyLines) {
        let line_count = self.buffer.line_count();
        let screen_dirty = self.viewport.dirty_lines_to_region(&dirty_lines, line_count);
        self.dirty_region.merge(screen_dirty);
    }

    /// Ensures the cursor is visible, scrolling if necessary.
    ///
    /// If scrolling occurs, marks the full viewport as dirty.
    pub fn ensure_cursor_visible(&mut self) {
        let cursor_line = self.buffer.cursor_position().line;
        let line_count = self.buffer.line_count();

        if self.viewport.ensure_visible(cursor_line, line_count) {
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

    #[test]
    fn test_mark_dirty_single_line() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0); // 10 visible lines
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            ctx.mark_dirty(DirtyLines::Single(0));
        }

        assert_eq!(dirty, DirtyRegion::Lines { from: 0, to: 1 });
    }

    #[test]
    fn test_mark_dirty_merges() {
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo\nbar");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            ctx.mark_dirty(DirtyLines::Single(0));
            ctx.mark_dirty(DirtyLines::Single(2));
        }

        assert_eq!(dirty, DirtyRegion::Lines { from: 0, to: 3 });
    }

    #[test]
    fn test_ensure_cursor_visible_no_scroll() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
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
        viewport.update_size(160.0); // 10 visible lines
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            ctx.ensure_cursor_visible();
        }

        // Should have scrolled and marked full viewport dirty
        assert_eq!(dirty, DirtyRegion::FullViewport);
        assert!(viewport.scroll_offset > 0);
    }

    #[test]
    fn test_mark_cursor_dirty() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        buffer.set_cursor(Position::new(1, 0));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            ctx.mark_cursor_dirty();
        }

        assert_eq!(dirty, DirtyRegion::Lines { from: 1, to: 2 });
    }
}
