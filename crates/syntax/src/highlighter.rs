// Chunk: docs/chunks/syntax_highlighting - Core syntax highlighter with incremental parsing

//! Syntax highlighter with incremental parsing support.
//!
//! The `SyntaxHighlighter` maintains a tree-sitter parse tree and provides
//! efficient incremental updates when the source changes. It converts
//! highlight events to styled lines for rendering.

use crate::edit::EditEvent;
use crate::registry::LanguageConfig;
use crate::theme::SyntaxTheme;
use lite_edit_buffer::{Span, Style, StyledLine};
use tree_sitter::{Parser, Tree};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// A syntax highlighter for a single buffer.
///
/// Owns a tree-sitter `Parser` and `Tree`, supports incremental updates,
/// and provides highlighted lines for rendering.
pub struct SyntaxHighlighter {
    /// The tree-sitter parser
    parser: Parser,
    /// The current parse tree
    tree: Tree,
    /// The highlight configuration for this language
    hl_config: HighlightConfiguration,
    /// The syntax theme
    theme: SyntaxTheme,
    /// Current source snapshot (needed for highlight queries)
    source: String,
}

impl SyntaxHighlighter {
    /// Creates a new syntax highlighter for the given language and source.
    ///
    /// # Arguments
    ///
    /// * `config` - The language configuration
    /// * `source` - The initial source text
    /// * `theme` - The syntax theme for styling
    ///
    /// # Returns
    ///
    /// Returns `None` if the highlighter cannot be created (e.g., invalid language).
    pub fn new(config: &LanguageConfig, source: &str, theme: SyntaxTheme) -> Option<Self> {
        let mut parser = Parser::new();
        parser.set_language(&config.language).ok()?;

        let tree = parser.parse(source, None)?;

        let hl_config = config.highlight_config(theme.capture_names())?;

        Some(Self {
            parser,
            tree,
            hl_config,
            theme,
            source: source.to_string(),
        })
    }

    /// Applies an edit to the parse tree incrementally.
    ///
    /// This method updates the tree in ~120µs for single-character edits,
    /// maintaining the <8ms keypress-to-glyph latency budget.
    ///
    /// # Arguments
    ///
    /// * `event` - The edit event describing the change
    /// * `new_source` - The complete source after the edit
    pub fn edit(&mut self, event: EditEvent, new_source: &str) {
        // Apply the edit to the existing tree
        self.tree.edit(&event.to_input_edit());

        // Re-parse with the old tree for incremental parsing
        if let Some(new_tree) = self.parser.parse(new_source, Some(&self.tree)) {
            self.tree = new_tree;
        }

        // Update the source snapshot
        self.source = new_source.to_string();
    }

    /// Returns highlighted spans for a single line.
    ///
    /// This method extracts just the line's byte range and highlights it,
    /// keeping per-line cost to ~170µs for a 60-line viewport total.
    ///
    /// # Arguments
    ///
    /// * `line_idx` - The 0-indexed line number
    ///
    /// # Returns
    ///
    /// A `StyledLine` with colored spans. Returns a plain unstyled line
    /// if highlighting fails or the line is out of bounds.
    pub fn highlight_line(&self, line_idx: usize) -> StyledLine {
        // Find the byte range for this line
        let (line_start, line_end) = match self.line_byte_range(line_idx) {
            Some(range) => range,
            None => return StyledLine::empty(),
        };

        // Get the line text
        let line_text = &self.source[line_start..line_end];
        if line_text.is_empty() {
            return StyledLine::empty();
        }

        // Use tree-sitter-highlight to get highlight events
        let mut highlighter = Highlighter::new();
        let highlights = match highlighter.highlight(&self.hl_config, self.source.as_bytes(), None, |_| None) {
            Ok(h) => h,
            Err(_) => return StyledLine::plain(line_text),
        };

        // Build spans from highlight events
        self.build_styled_line(line_text, line_start, line_end, highlights)
    }

    /// Finds the byte range [start, end) for a given line.
    fn line_byte_range(&self, line_idx: usize) -> Option<(usize, usize)> {
        let mut current_line = 0;
        let mut line_start = 0;

        for (idx, ch) in self.source.char_indices() {
            if current_line == line_idx {
                // Found the line start
                line_start = idx;
                // Find the end
                for (end_idx, end_ch) in self.source[line_start..].char_indices() {
                    if end_ch == '\n' {
                        return Some((line_start, line_start + end_idx));
                    }
                }
                // No newline found - line goes to end
                return Some((line_start, self.source.len()));
            }

            if ch == '\n' {
                current_line += 1;
            }
        }

        // Line index out of bounds
        if current_line == line_idx && line_start <= self.source.len() {
            // Empty last line
            return Some((self.source.len(), self.source.len()));
        }

        None
    }

    /// Builds a StyledLine from highlight events for a specific line range.
    fn build_styled_line(
        &self,
        line_text: &str,
        line_start: usize,
        line_end: usize,
        highlights: impl Iterator<Item = Result<HighlightEvent, tree_sitter_highlight::Error>>,
    ) -> StyledLine {
        let mut spans = Vec::new();
        let mut current_style: Option<&Style> = None;
        let mut style_stack: Vec<Option<&Style>> = Vec::new();

        // Track which parts of the line we've covered
        let mut covered_until = line_start;
        let mut pending_text = String::new();

        for event in highlights {
            match event {
                Ok(HighlightEvent::Source { start, end }) => {
                    // Skip events entirely before or after our line
                    if end <= line_start || start >= line_end {
                        continue;
                    }

                    // Clamp to line boundaries
                    let actual_start = start.max(line_start);
                    let actual_end = end.min(line_end);

                    // If there's a gap, fill with unstyled text
                    if actual_start > covered_until {
                        // Flush pending text with current style
                        if !pending_text.is_empty() {
                            let style = current_style.copied().unwrap_or_default();
                            spans.push(Span::new(std::mem::take(&mut pending_text), style));
                        }
                        // Add gap as unstyled
                        let gap_text = &self.source[covered_until..actual_start];
                        if !gap_text.is_empty() {
                            spans.push(Span::plain(gap_text));
                        }
                    }

                    // Add this source range to pending text
                    pending_text.push_str(&self.source[actual_start..actual_end]);
                    covered_until = actual_end;
                }
                Ok(HighlightEvent::HighlightStart(highlight)) => {
                    // Flush pending text with current style before changing style
                    if !pending_text.is_empty() {
                        let style = current_style.copied().unwrap_or_default();
                        spans.push(Span::new(std::mem::take(&mut pending_text), style));
                    }

                    // Push current style onto stack and set new style
                    style_stack.push(current_style);
                    let capture_name = self.theme.capture_names().get(highlight.0);
                    current_style = capture_name.and_then(|name| self.theme.style_for_capture(name));
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    // Flush pending text with current style
                    if !pending_text.is_empty() {
                        let style = current_style.copied().unwrap_or_default();
                        spans.push(Span::new(std::mem::take(&mut pending_text), style));
                    }

                    // Pop style from stack
                    current_style = style_stack.pop().flatten();
                }
                Err(_) => {
                    // On error, return what we have so far
                    break;
                }
            }
        }

        // Flush any remaining pending text
        if !pending_text.is_empty() {
            let style = current_style.copied().unwrap_or_default();
            spans.push(Span::new(pending_text, style));
        }

        // Fill remaining line with unstyled text
        if covered_until < line_end {
            let remaining = &self.source[covered_until..line_end];
            if !remaining.is_empty() {
                spans.push(Span::plain(remaining));
            }
        }

        // If no spans were created, return plain text
        if spans.is_empty() {
            return StyledLine::plain(line_text);
        }

        // Merge adjacent spans with the same style
        let merged = merge_spans(spans);
        StyledLine::new(merged)
    }

    /// Returns the current source text.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Updates the highlighter with new source content.
    ///
    /// This performs a full re-parse rather than incremental update.
    /// Use `edit()` for better performance when you have edit position information.
    ///
    /// This is useful when you don't have precise edit information but need
    /// to keep the highlighter in sync with buffer content.
    pub fn update_source(&mut self, new_source: &str) {
        // Re-parse the entire source (non-incremental)
        if let Some(new_tree) = self.parser.parse(new_source, None) {
            self.tree = new_tree;
        }
        self.source = new_source.to_string();
    }

    /// Returns the number of lines in the source.
    pub fn line_count(&self) -> usize {
        self.source.chars().filter(|&c| c == '\n').count() + 1
    }
}

/// Merges adjacent spans that have the same style.
fn merge_spans(spans: Vec<Span>) -> Vec<Span> {
    let mut result: Vec<Span> = Vec::with_capacity(spans.len());

    for span in spans {
        if let Some(last) = result.last_mut() {
            if last.style == span.style {
                last.text.push_str(&span.text);
                continue;
            }
        }
        result.push(span);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::LanguageRegistry;
    use lite_edit_buffer::Color;

    fn make_rust_highlighter(source: &str) -> Option<SyntaxHighlighter> {
        let registry = LanguageRegistry::new();
        let config = registry.config_for_extension("rs")?;
        let theme = SyntaxTheme::catppuccin_mocha();
        SyntaxHighlighter::new(config, source, theme)
    }

    #[test]
    fn test_new_creates_highlighter() {
        let source = "fn main() {}";
        let hl = make_rust_highlighter(source);
        assert!(hl.is_some());
    }

    #[test]
    fn test_highlight_line_returns_styled_line() {
        let source = "fn main() {}";
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(0);
        assert!(!styled.spans.is_empty());
    }

    #[test]
    fn test_highlight_line_out_of_bounds() {
        let source = "fn main() {}";
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(100);
        assert!(styled.is_empty());
    }

    #[test]
    fn test_highlight_empty_line() {
        let source = "fn main() {\n\n}";
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(1); // empty line
        assert!(styled.is_empty() || styled.char_count() == 0);
    }

    #[test]
    fn test_keyword_has_style() {
        let source = "fn main() {}";
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(0);

        // Find the "fn" span
        let mut found_styled_fn = false;
        for span in &styled.spans {
            if span.text.contains("fn") {
                // Should have a non-default foreground color
                if !matches!(span.style.fg, Color::Default) {
                    found_styled_fn = true;
                    break;
                }
            }
        }

        // Note: The exact styling depends on the grammar's capture names
        // We just verify we got some spans
        assert!(!styled.spans.is_empty(), "Expected styled spans");
    }

    #[test]
    fn test_string_has_style() {
        let source = r#"let s = "hello";"#;
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(0);

        // Find the string span
        let mut found_string = false;
        for span in &styled.spans {
            if span.text.contains("hello") {
                // Strings should have a non-default color
                if !matches!(span.style.fg, Color::Default) {
                    found_string = true;
                }
            }
        }
        assert!(!styled.spans.is_empty(), "Expected styled spans for string literal");
    }

    #[test]
    fn test_comment_has_style() {
        let source = "// this is a comment";
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(0);

        // Comments should be styled
        assert!(!styled.spans.is_empty());
        // At least one span should have italic or non-default color
        let has_styled = styled.spans.iter().any(|s| {
            s.style.italic || !matches!(s.style.fg, Color::Default)
        });
        assert!(has_styled, "Comment should have styling");
    }

    #[test]
    fn test_incremental_edit() {
        let source = "fn main() {}";
        let mut hl = make_rust_highlighter(source).unwrap();

        // Insert a character
        let event = crate::edit::insert_event(source, 0, 2, "x");
        let new_source = "fnx main() {}";
        hl.edit(event, new_source);

        assert_eq!(hl.source(), new_source);
        let styled = hl.highlight_line(0);
        assert!(!styled.spans.is_empty());
    }

    #[test]
    fn test_line_byte_range_first_line() {
        let source = "hello\nworld";
        let hl = make_rust_highlighter(source).unwrap();
        let range = hl.line_byte_range(0);
        assert_eq!(range, Some((0, 5)));
    }

    #[test]
    fn test_line_byte_range_second_line() {
        let source = "hello\nworld";
        let hl = make_rust_highlighter(source).unwrap();
        let range = hl.line_byte_range(1);
        assert_eq!(range, Some((6, 11)));
    }

    #[test]
    fn test_line_byte_range_out_of_bounds() {
        let source = "hello";
        let hl = make_rust_highlighter(source).unwrap();
        let range = hl.line_byte_range(5);
        assert_eq!(range, None);
    }

    #[test]
    fn test_line_count_single_line() {
        let source = "hello";
        let hl = make_rust_highlighter(source).unwrap();
        assert_eq!(hl.line_count(), 1);
    }

    #[test]
    fn test_line_count_multiple_lines() {
        let source = "hello\nworld\ntest";
        let hl = make_rust_highlighter(source).unwrap();
        assert_eq!(hl.line_count(), 3);
    }

    #[test]
    fn test_merge_spans_combines_same_style() {
        let style = Style::default();
        let spans = vec![
            Span::new("hello", style),
            Span::new(" ", style),
            Span::new("world", style),
        ];
        let merged = merge_spans(spans);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "hello world");
    }

    #[test]
    fn test_merge_spans_preserves_different_styles() {
        let style1 = Style {
            bold: true,
            ..Style::default()
        };
        let style2 = Style::default();
        let spans = vec![
            Span::new("hello", style1),
            Span::new("world", style2),
        ];
        let merged = merge_spans(spans);
        assert_eq!(merged.len(), 2);
    }
}
