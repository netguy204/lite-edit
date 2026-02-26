// Chunk: docs/chunks/syntax_highlighting - Highlighted buffer view wrapper
// Chunk: docs/chunks/syntax_highlight_perf - Viewport-batch highlighting for performance

//! Wrapper that provides syntax-highlighted buffer view.
//!
//! This module provides `HighlightedBufferView`, which wraps a `TextBuffer`
//! and optional `SyntaxHighlighter` to implement `BufferView` with syntax
//! highlighting support.
//!
//! ## Performance
//!
//! When `styled_line()` is called, the view triggers viewport-batch highlighting
//! to populate the highlighter's cache. This ensures that all visible lines are
//! highlighted in a single pass using `QueryCursor`, rather than re-parsing the
//! entire file for each line.

use lite_edit_buffer::{BufferView, CursorInfo, DirtyLines, Position, StyledLine, TextBuffer};
use lite_edit_syntax::SyntaxHighlighter;

/// Default viewport size for batch highlighting.
///
/// When `styled_line()` is called, we pre-highlight this many lines starting
/// from the requested line to populate the cache. This is typically larger than
/// a screen's worth of lines to handle scrolling without re-highlighting.
const DEFAULT_VIEWPORT_LINES: usize = 80;

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

    // Chunk: docs/chunks/highlight_text_source - Buffer is source of truth for text
    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.buffer.line_count() {
            return None;
        }

        // Always read text from the buffer (authoritative source of truth)
        let line_text = self.buffer.line_content(line);

        match self.highlighter {
            Some(hl) => {
                // Pre-populate the highlighter's viewport cache for batch efficiency.
                // This is called once per frame, and the cache will serve
                // subsequent lines without re-highlighting.
                let end_line = (line + DEFAULT_VIEWPORT_LINES).min(self.buffer.line_count());
                hl.highlight_viewport(line, end_line);

                // Get styled spans using the buffer's text (not the highlighter's source).
                // This ensures the rendered text is always correct even if the highlighter
                // is stale. The worst case is slightly outdated syntax colors.
                let spans = hl.highlight_spans_for_line(line, &line_text);
                Some(StyledLine::new(spans))
            }
            None => {
                // No highlighter - return plain text
                Some(StyledLine::plain(line_text))
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

    // Chunk: docs/chunks/highlight_text_source - Buffer is source of truth for text
    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if line >= self.buffer.line_count() {
            return None;
        }

        // Always read text from the buffer (authoritative source of truth)
        let line_text = self.buffer.line_content(line);

        match self.highlighter {
            Some(hl) => {
                // Pre-populate the highlighter's viewport cache for batch efficiency.
                let end_line = (line + DEFAULT_VIEWPORT_LINES).min(self.buffer.line_count());
                hl.highlight_viewport(line, end_line);

                // Get styled spans using the buffer's text (not the highlighter's source).
                // This ensures the rendered text is always correct even if the highlighter
                // is stale. The worst case is slightly outdated syntax colors.
                let spans = hl.highlight_spans_for_line(line, &line_text);
                Some(StyledLine::new(spans))
            }
            None => {
                // No highlighter - return plain text
                Some(StyledLine::plain(line_text))
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

    // Chunk: docs/chunks/highlight_text_source - Integration test for stale highlighter
    #[test]
    fn test_styled_line_shows_buffer_content_when_highlighter_stale() {
        use lite_edit_syntax::{LanguageRegistry, SyntaxHighlighter, SyntaxTheme};

        // Create a buffer and highlighter from the same initial source
        let initial_source = "fn main() {}";
        let mut buffer = TextBuffer::from_str(initial_source);

        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("rs").expect("Rust config");
        let theme = SyntaxTheme::catppuccin_mocha();
        let highlighter = SyntaxHighlighter::new(config, initial_source, theme)
            .expect("Should create highlighter");

        // Modify the buffer WITHOUT syncing the highlighter
        // (simulates the bug scenario where handle_insert_text didn't sync)
        buffer.insert_str(" /* edited */");

        // The buffer now has different content than the highlighter
        // Buffer: "fn main() {} /* edited */"
        // Highlighter source: "fn main() {}"

        // Create the view with the stale highlighter
        let view = HighlightedBufferView::new(&buffer, Some(&highlighter));

        // Get the styled line - this is the key assertion
        let styled = view.styled_line(0).unwrap();
        let rendered: String = styled.spans.iter().map(|s| s.text.as_str()).collect();

        // The rendered text MUST match the buffer's content, NOT the highlighter's source
        let expected = buffer.line_content(0);
        assert_eq!(
            rendered, expected,
            "Rendered text should match buffer content even when highlighter is stale.\n\
             Got: {:?}\n\
             Expected (from buffer): {:?}\n\
             Highlighter source: {:?}",
            rendered, expected, highlighter.source()
        );
    }

    // Chunk: docs/chunks/highlight_text_source - Verify correct styling when synced
    #[test]
    fn test_styled_line_has_correct_styling_when_synced() {
        use lite_edit_syntax::{LanguageRegistry, SyntaxHighlighter, SyntaxTheme};
        use lite_edit_buffer::Color;

        let source = "fn main() {}";
        let buffer = TextBuffer::from_str(source);

        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("rs").expect("Rust config");
        let theme = SyntaxTheme::catppuccin_mocha();
        let highlighter = SyntaxHighlighter::new(config, source, theme)
            .expect("Should create highlighter");

        let view = HighlightedBufferView::new(&buffer, Some(&highlighter));
        let styled = view.styled_line(0).unwrap();

        // Text should match
        let rendered: String = styled.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(rendered, source, "Rendered text should match source");

        // Should have syntax styling for the fn keyword
        let has_styled_fn = styled.spans.iter().any(|s| {
            s.text == "fn" && !matches!(s.style.fg, Color::Default)
        });
        assert!(has_styled_fn, "fn keyword should have syntax highlighting");
    }
}
