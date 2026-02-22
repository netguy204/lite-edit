// Chunk: docs/chunks/syntax_highlighting - Highlighted buffer view wrapper

//! Wrapper that provides syntax-highlighted buffer view.
//!
//! This module provides `HighlightedBufferView`, which wraps a `TextBuffer`
//! and optional `SyntaxHighlighter` to implement `BufferView` with syntax
//! highlighting support.

use lite_edit_buffer::{BufferView, CursorInfo, DirtyLines, Position, StyledLine, TextBuffer};
use lite_edit_syntax::SyntaxHighlighter;

/// A view over TextBuffer that applies syntax highlighting.
///
/// This wrapper implements `BufferView` by delegating most methods to the
/// underlying `TextBuffer`, but overrides `styled_line()` to use the
/// highlighter when available.
pub struct HighlightedBufferView<'a> {
    /// The underlying text buffer
    buffer: &'a TextBuffer,
    /// The optional syntax highlighter
    highlighter: Option<&'a SyntaxHighlighter>,
}

impl<'a> HighlightedBufferView<'a> {
    /// Creates a new highlighted buffer view.
    pub fn new(buffer: &'a TextBuffer, highlighter: Option<&'a SyntaxHighlighter>) -> Self {
        Self { buffer, highlighter }
    }
}

impl<'a> BufferView for HighlightedBufferView<'a> {
    fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.buffer.line_count() {
            return None;
        }

        match self.highlighter {
            Some(hl) => {
                // Use highlighter to get styled line
                Some(hl.highlight_line(line))
            }
            None => {
                // No highlighter - return plain text
                let content = self.buffer.line_content(line);
                Some(StyledLine::plain(content))
            }
        }
    }

    fn line_len(&self, line: usize) -> usize {
        self.buffer.line_len(line)
    }

    fn take_dirty(&mut self) -> DirtyLines {
        // We can't actually drain dirty state through an immutable reference.
        // This is only called during mutable operations, and we delegate to
        // the actual buffer in those contexts.
        DirtyLines::None
    }

    fn is_editable(&self) -> bool {
        self.buffer.is_editable()
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        self.buffer.cursor_info()
    }

    fn selection_range(&self) -> Option<(Position, Position)> {
        self.buffer.selection_range()
    }
}

/// Mutable version of highlighted buffer view for rendering with dirty tracking.
pub struct HighlightedBufferViewMut<'a> {
    /// The underlying text buffer (mutable for take_dirty)
    buffer: &'a mut TextBuffer,
    /// The optional syntax highlighter
    highlighter: Option<&'a SyntaxHighlighter>,
}

impl<'a> HighlightedBufferViewMut<'a> {
    /// Creates a new mutable highlighted buffer view.
    pub fn new(buffer: &'a mut TextBuffer, highlighter: Option<&'a SyntaxHighlighter>) -> Self {
        Self { buffer, highlighter }
    }
}

impl<'a> BufferView for HighlightedBufferViewMut<'a> {
    fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.buffer.line_count() {
            return None;
        }

        match self.highlighter {
            Some(hl) => {
                // Use highlighter to get styled line
                Some(hl.highlight_line(line))
            }
            None => {
                // No highlighter - return plain text
                let content = self.buffer.line_content(line);
                Some(StyledLine::plain(content))
            }
        }
    }

    fn line_len(&self, line: usize) -> usize {
        self.buffer.line_len(line)
    }

    fn take_dirty(&mut self) -> DirtyLines {
        self.buffer.take_dirty()
    }

    fn is_editable(&self) -> bool {
        self.buffer.is_editable()
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        self.buffer.cursor_info()
    }

    fn selection_range(&self) -> Option<(Position, Position)> {
        self.buffer.selection_range()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlighted_view_without_highlighter() {
        let buffer = TextBuffer::from_str("hello\nworld");
        let view = HighlightedBufferView::new(&buffer, None);

        assert_eq!(view.line_count(), 2);
        assert!(view.is_editable());

        let styled = view.styled_line(0).unwrap();
        assert_eq!(styled.spans.len(), 1);
        assert_eq!(styled.spans[0].text, "hello");
    }

    #[test]
    fn test_highlighted_view_line_out_of_bounds() {
        let buffer = TextBuffer::from_str("hello");
        let view = HighlightedBufferView::new(&buffer, None);

        assert!(view.styled_line(10).is_none());
    }

    #[test]
    fn test_highlighted_view_delegates_line_len() {
        let buffer = TextBuffer::from_str("hello\nworld");
        let view = HighlightedBufferView::new(&buffer, None);

        assert_eq!(view.line_len(0), 5);
        assert_eq!(view.line_len(1), 5);
    }
}
