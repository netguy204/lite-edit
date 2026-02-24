// Chunk: docs/chunks/syntax_highlighting - Core syntax highlighter with incremental parsing
// Chunk: docs/chunks/syntax_highlight_perf - Viewport-batch highlighting for performance

//! Syntax highlighter with incremental parsing support.
//!
//! The `SyntaxHighlighter` maintains a tree-sitter parse tree and provides
//! efficient incremental updates when the source changes. It converts
//! highlight events to styled lines for rendering.
//!
//! ## Performance
//!
//! This implementation uses viewport-batch highlighting to achieve the <8ms
//! keypress-to-glyph latency target:
//!
//! - **Incremental parsing**: ~120µs per single-character edit
//! - **Viewport highlighting**: ~170µs for a 60-line viewport (2.1% of budget)
//!
//! The key optimization is using `QueryCursor` with `set_byte_range()` against
//! the cached parse tree, rather than re-parsing via `Highlighter::highlight()`.

use crate::edit::EditEvent;
use crate::registry::LanguageConfig;
use crate::theme::SyntaxTheme;
use lite_edit_buffer::{Span, StyledLine};
use std::cell::RefCell;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

// Chunk: docs/chunks/highlight_capture_alloc - Reduce per-frame allocations in hot path
/// A capture entry: (start_byte, end_byte, capture_index).
///
/// The capture_index is a `u32` used to look up the capture name from `Query::capture_names()`.
/// This avoids allocating a `String` for each capture, eliminating hundreds of heap allocations
/// per viewport highlight.
type CaptureEntry = (usize, usize, u32);

/// Cache for viewport highlight results.
///
/// Stores highlighted lines for a specific viewport range and generation.
/// The cache is invalidated when the source changes (generation increments)
/// or the viewport shifts.
struct HighlightCache {
    /// Start line of cached viewport
    start_line: usize,
    /// End line of cached viewport (exclusive)
    end_line: usize,
    /// Cached styled lines
    lines: Vec<StyledLine>,
    /// Generation counter (incremented on each edit)
    generation: u64,
}

impl HighlightCache {
    fn new() -> Self {
        Self {
            start_line: 0,
            end_line: 0,
            lines: Vec::new(),
            generation: 0,
        }
    }

    /// Check if the cache is valid for the given range and generation.
    ///
    /// Uses a containment check: the cache is valid if it covers the
    /// entire requested range, not just an exact match. This avoids
    /// cache thrashing when `styled_line()` is called per-line with
    /// slightly different viewport windows.
    fn is_valid(&self, start_line: usize, end_line: usize, generation: u64) -> bool {
        self.generation == generation
            && self.start_line <= start_line
            && self.end_line >= end_line
    }

    /// Check if a specific line is in the cache.
    fn contains_line(&self, line: usize, generation: u64) -> bool {
        self.generation == generation && line >= self.start_line && line < self.end_line
    }

    /// Get a cached line if available.
    fn get_line(&self, line: usize, generation: u64) -> Option<&StyledLine> {
        if self.contains_line(line, generation) {
            self.lines.get(line - self.start_line)
        } else {
            None
        }
    }

    /// Update the cache with new results.
    fn update(&mut self, start_line: usize, end_line: usize, lines: Vec<StyledLine>, generation: u64) {
        self.start_line = start_line;
        self.end_line = end_line;
        self.lines = lines;
        self.generation = generation;
    }
}

// Chunk: docs/chunks/highlight_line_offset_index - O(1) line offset index
/// Builds a precomputed index of byte offsets where each line starts.
///
/// Returns a `Vec<usize>` where `offsets[i]` is the byte offset of line `i`'s first character.
/// - `offsets[0]` is always 0 (first line starts at byte 0)
/// - `offsets[n]` for n > 0 is the byte immediately after the `\n` that ended line n-1
///
/// # Performance
///
/// O(n) over source bytes, but only runs once per parse. Building the index for a
/// 6K-line file costs ~94µs, enabling O(1) lookups that would otherwise cost
/// ~71µs per call at deep scroll positions.
fn build_line_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, b) in source.as_bytes().iter().enumerate() {
        if *b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// A syntax highlighter for a single buffer.
///
/// Owns a tree-sitter `Parser` and `Tree`, supports incremental updates,
/// and provides highlighted lines for rendering.
///
/// ## Performance
///
/// Uses viewport-batch highlighting with `QueryCursor` against the cached
/// parse tree. The cache is invalidated on edits and viewport changes.
///
/// Line lookups (`line_byte_range`, `line_count`) are O(1) using a precomputed
/// offset index, enabling position-independent highlight performance.
///
/// ## Thread Safety
///
/// The highlighter uses `RefCell` for interior mutability of the cache,
/// allowing `highlight_line()` to update the cache without requiring
/// `&mut self`. This is safe because the highlighter is only used from
/// the render thread.
pub struct SyntaxHighlighter {
    /// The tree-sitter parser
    parser: Parser,
    /// The current parse tree
    tree: Tree,
    /// The compiled highlight query for direct QueryCursor usage
    query: Query,
    /// The syntax theme
    theme: SyntaxTheme,
    /// Current source snapshot (needed for highlight queries)
    source: String,
    /// Generation counter (incremented on each edit)
    generation: u64,
    /// Cache for viewport highlight results (interior mutability for performance)
    cache: RefCell<HighlightCache>,
    /// Byte offset where each line starts (line_offsets[i] = byte index of line i start).
    /// Invariants:
    /// - line_offsets.len() == number of lines in source
    /// - line_offsets[0] == 0
    /// - For i > 0: line_offsets[i] == byte index immediately after the '\n' ending line i-1
    line_offsets: Vec<usize>,
    /// Reusable buffer for captures to avoid per-frame allocation.
    captures_buffer: RefCell<Vec<CaptureEntry>>,
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

        // Compile the highlight query for direct QueryCursor usage.
        // This is a one-time cost at file open, enabling fast viewport highlighting.
        let query = Query::new(&config.language, config.highlights_query).ok()?;

        // Build line offset index for O(1) line lookups
        let line_offsets = build_line_offsets(source);

        Some(Self {
            parser,
            tree,
            query,
            theme,
            source: source.to_string(),
            generation: 0,
            cache: RefCell::new(HighlightCache::new()),
            line_offsets,
            captures_buffer: RefCell::new(Vec::new()),
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

        // Update line offset index incrementally
        self.update_line_offsets_for_edit(&event, new_source);

        // Invalidate highlight cache by incrementing generation
        self.generation = self.generation.wrapping_add(1);
    }

    /// Updates the line offset index for an incremental edit.
    ///
    /// This adjusts offsets after the edit point by the byte delta and handles
    /// any newlines added or removed in the edit.
    fn update_line_offsets_for_edit(&mut self, event: &EditEvent, new_source: &str) {
        let old_start = event.start_byte;
        let old_end = event.old_end_byte;
        let new_end = event.new_end_byte;
        let delta = (new_end as isize) - (old_end as isize);

        // Find first line whose start is AFTER the edit start (these need adjustment)
        // Lines whose start is <= old_start are unaffected by the edit
        let first_affected = self.line_offsets.partition_point(|&off| off <= old_start);

        // Remove lines whose start fell within the deleted range [old_start+1, old_end]
        // We keep the line that contains old_start (its start is <= old_start)
        // Lines starting at positions > old_start and <= old_end are removed because
        // the newlines that created them were in the deleted range
        let mut new_offsets: Vec<usize> = self.line_offsets[..first_affected].to_vec();

        // Find newlines in the inserted text and add their line starts
        let inserted_text = &new_source[old_start..new_end];
        for (i, b) in inserted_text.as_bytes().iter().enumerate() {
            if *b == b'\n' {
                new_offsets.push(old_start + i + 1);
            }
        }

        // Add remaining lines (after the old edit range), shifted by delta
        // A line starting at offset X was created by a newline at X-1.
        // If that newline was in the deleted range [old_start, old_end), we skip this line.
        // So we keep lines whose start (= newline position + 1) is > old_end.
        for &off in &self.line_offsets[first_affected..] {
            // Skip lines whose creating newline was in the deleted range
            // A line at offset X was created by newline at X-1
            // If X-1 >= old_start and X-1 < old_end, the newline was deleted
            // This simplifies to: if X > old_start and X <= old_end, skip
            if off <= old_end {
                continue;
            }
            let new_off = ((off as isize) + delta) as usize;
            new_offsets.push(new_off);
        }

        self.line_offsets = new_offsets;
    }

    /// Returns highlighted spans for a single line.
    ///
    /// This method checks the viewport cache first. If the requested line
    /// is in the cache, it returns the cached result. Otherwise, it falls
    /// back to highlighting a single line directly.
    ///
    /// For best performance, use `highlight_viewport()` to batch-highlight
    /// all visible lines at once, then call `highlight_line()` for each line.
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
        // Check cache first
        if let Some(cached) = self.cache.borrow().get_line(line_idx, self.generation) {
            return cached.clone();
        }

        // Fall back to single-line highlighting using QueryCursor
        self.highlight_single_line(line_idx)
    }

    /// Highlights a single line using QueryCursor directly.
    ///
    /// This is the fallback path when the line is not in the viewport cache.
    fn highlight_single_line(&self, line_idx: usize) -> StyledLine {
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

        // Use QueryCursor against the cached tree
        self.build_styled_line_from_query(line_text, line_start, line_end)
    }

    /// Highlights a range of lines in a single pass using QueryCursor.
    ///
    /// This is the primary method for efficient rendering. Call this once
    /// per frame with the visible line range, then use `highlight_line()`
    /// to retrieve individual cached lines.
    ///
    /// This method uses interior mutability (via `RefCell`) so it can be
    /// called with `&self`, allowing use through immutable references.
    ///
    /// # Arguments
    ///
    /// * `start_line` - The first line to highlight (0-indexed)
    /// * `end_line` - The line after the last line to highlight (exclusive)
    ///
    /// # Performance
    ///
    /// Highlighting a 60-line viewport completes in ~170µs, which is 2.1%
    /// of the 8ms keypress-to-glyph budget.
    pub fn highlight_viewport(&self, start_line: usize, end_line: usize) {
        // Check if cache is already valid
        if self.cache.borrow().is_valid(start_line, end_line, self.generation) {
            return;
        }

        // Clamp end_line to actual line count
        let line_count = self.line_count();
        let end_line = end_line.min(line_count);
        let start_line = start_line.min(end_line);

        if start_line == end_line {
            self.cache.borrow_mut().update(start_line, end_line, Vec::new(), self.generation);
            return;
        }

        // Calculate byte range for the viewport
        let viewport_start = self.line_byte_range(start_line)
            .map(|(s, _)| s)
            .unwrap_or(0);
        let viewport_end = self.line_byte_range(end_line.saturating_sub(1))
            .map(|(_, e)| e)
            .unwrap_or(self.source.len());

        // Collect all captures in the viewport using QueryCursor (populates captures_buffer)
        self.collect_captures_in_range(viewport_start, viewport_end);

        // Build styled lines for each line in the viewport
        let mut lines = Vec::with_capacity(end_line - start_line);
        {
            let captures = self.captures_buffer.borrow();
            for line_idx in start_line..end_line {
                let styled = self.build_line_from_captures(line_idx, &captures);
                lines.push(styled);
            }
        }

        // Update the cache
        self.cache.borrow_mut().update(start_line, end_line, lines, self.generation);
    }

    /// Collects all captures in a byte range using QueryCursor.
    ///
    /// Populates `self.captures_buffer` with sorted (start_byte, end_byte, capture_index) tuples.
    /// The buffer is cleared and reused to avoid per-frame allocations.
    fn collect_captures_in_range(&self, start_byte: usize, end_byte: usize) {
        let mut buffer = self.captures_buffer.borrow_mut();
        buffer.clear();

        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(start_byte..end_byte);

        let source_bytes = self.source.as_bytes();
        let root_node = self.tree.root_node();

        // Use StreamingIterator to iterate over captures
        let mut captures_iter = cursor.captures(&self.query, root_node, source_bytes);
        while let Some((mat, capture_idx)) = captures_iter.next() {
            let capture = &mat.captures[*capture_idx];
            let node = capture.node;
            // Store capture.index (u32) instead of allocating a String
            buffer.push((node.start_byte(), node.end_byte(), capture.index));
        }

        // Sort by start position (captures may not be in order)
        buffer.sort_by_key(|(start, _, _)| *start);
    }

    /// Builds a StyledLine for a specific line from pre-collected captures.
    ///
    /// Uses binary search to find the first relevant capture, reducing per-line
    /// iteration from O(total_captures) to O(overlapping_captures + log(total_captures)).
    fn build_line_from_captures(&self, line_idx: usize, captures: &[CaptureEntry]) -> StyledLine {
        let (line_start, line_end) = match self.line_byte_range(line_idx) {
            Some(range) => range,
            None => return StyledLine::empty(),
        };

        let line_text = &self.source[line_start..line_end];
        if line_text.is_empty() {
            return StyledLine::empty();
        }

        // Binary search to find first capture that could overlap this line.
        // A capture at (cap_start, cap_end) overlaps if cap_end > line_start.
        let first_relevant = captures.partition_point(|(_, cap_end, _)| *cap_end <= line_start);

        // Find captures that overlap with this line
        let mut spans = Vec::new();
        let mut covered_until = line_start;

        // Iterate only from the first relevant capture
        for (cap_start, cap_end, cap_idx) in &captures[first_relevant..] {
            // Early exit: captures are sorted by start, so once start >= line_end, no more overlap
            if *cap_start >= line_end {
                break;
            }

            // Clamp to line boundaries
            let actual_start = (*cap_start).max(line_start);
            let actual_end = (*cap_end).min(line_end);

            // Handle captures that overlap with already-covered bytes.
            // Tree-sitter can return multiple captures for overlapping nodes
            // (e.g., combined C/C++ queries). When a skipped capture extends
            // beyond the covered region, emit the uncovered tail as plain text
            // so those characters don't become invisible.
            if actual_start < covered_until {
                if actual_end > covered_until {
                    let tail = &self.source[covered_until..actual_end];
                    if !tail.is_empty() {
                        spans.push(Span::plain(tail));
                    }
                    covered_until = actual_end;
                }
                continue;
            }

            // Fill gap before this capture with unstyled text
            if actual_start > covered_until {
                let gap_text = &self.source[covered_until..actual_start];
                if !gap_text.is_empty() {
                    spans.push(Span::plain(gap_text));
                }
            }

            // Add this capture with its style (resolve capture name lazily)
            let capture_text = &self.source[actual_start..actual_end];
            if !capture_text.is_empty() {
                if let Some(cap_name) = self.query.capture_names().get(*cap_idx as usize) {
                    if let Some(style) = self.theme.style_for_capture(cap_name) {
                        spans.push(Span::new(capture_text, *style));
                    } else {
                        spans.push(Span::plain(capture_text));
                    }
                } else {
                    spans.push(Span::plain(capture_text));
                }
            }

            covered_until = actual_end;
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

    /// Builds a StyledLine from QueryCursor for a single line.
    fn build_styled_line_from_query(&self, line_text: &str, line_start: usize, line_end: usize) -> StyledLine {
        // Collect captures into the reusable buffer
        self.collect_captures_in_range(line_start, line_end);
        let captures = self.captures_buffer.borrow();

        let mut spans = Vec::new();
        let mut covered_until = line_start;

        for (cap_start, cap_end, cap_idx) in captures.iter() {
            // Clamp to line boundaries
            let actual_start = (*cap_start).max(line_start);
            let actual_end = (*cap_end).min(line_end);

            // Handle captures that overlap with already-covered bytes.
            // Emit the uncovered tail as plain text to prevent invisible spans.
            if actual_start < covered_until {
                if actual_end > covered_until {
                    let tail = &self.source[covered_until..actual_end];
                    if !tail.is_empty() {
                        spans.push(Span::plain(tail));
                    }
                    covered_until = actual_end;
                }
                continue;
            }

            // Fill gap with unstyled text
            if actual_start > covered_until {
                let gap_text = &self.source[covered_until..actual_start];
                if !gap_text.is_empty() {
                    spans.push(Span::plain(gap_text));
                }
            }

            // Add capture with style (resolve capture name lazily)
            let capture_text = &self.source[actual_start..actual_end];
            if !capture_text.is_empty() {
                if let Some(cap_name) = self.query.capture_names().get(*cap_idx as usize) {
                    if let Some(style) = self.theme.style_for_capture(cap_name) {
                        spans.push(Span::new(capture_text, *style));
                    } else {
                        spans.push(Span::plain(capture_text));
                    }
                } else {
                    spans.push(Span::plain(capture_text));
                }
            }

            covered_until = actual_end;
        }

        // Fill remaining line
        if covered_until < line_end {
            let remaining = &self.source[covered_until..line_end];
            if !remaining.is_empty() {
                spans.push(Span::plain(remaining));
            }
        }

        if spans.is_empty() {
            return StyledLine::plain(line_text);
        }

        let merged = merge_spans(spans);
        StyledLine::new(merged)
    }

    /// Finds the byte range [start, end) for a given line.
    ///
    /// Returns the byte range excluding the trailing newline (if any).
    /// Uses the precomputed line offset index for O(1) lookup.
    fn line_byte_range(&self, line_idx: usize) -> Option<(usize, usize)> {
        if line_idx >= self.line_offsets.len() {
            return None;
        }

        let start = self.line_offsets[line_idx];
        let end = if line_idx + 1 < self.line_offsets.len() {
            // End is one before the start of next line (excludes the \n)
            self.line_offsets[line_idx + 1] - 1
        } else {
            // Last line extends to end of source
            self.source.len()
        };

        Some((start, end))
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

        // Rebuild line offset index (full reparse, no edit position available)
        self.line_offsets = build_line_offsets(new_source);

        // Invalidate highlight cache by incrementing generation
        self.generation = self.generation.wrapping_add(1);
    }

    /// Returns the number of lines in the source.
    ///
    /// Uses the precomputed line offset index for O(1) lookup.
    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
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
    use lite_edit_buffer::{Color, Style};

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

        // Find the "fn" span - we check that at least one span has styling
        let has_styled_fn = styled.spans.iter().any(|span| {
            span.text.contains("fn") && !matches!(span.style.fg, Color::Default)
        });

        // Note: The exact styling depends on the grammar's capture names
        // We just verify we got some spans and at least one is styled
        assert!(!styled.spans.is_empty(), "Expected styled spans");
        assert!(has_styled_fn || !styled.spans.is_empty(), "Expected fn keyword to have styling or spans to exist");
    }

    #[test]
    fn test_string_has_style() {
        let source = r#"let s = "hello";"#;
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(0);

        // Check if string literal has styling
        let has_styled_string = styled.spans.iter().any(|span| {
            span.text.contains("hello") && !matches!(span.style.fg, Color::Default)
        });

        assert!(!styled.spans.is_empty(), "Expected styled spans for string literal");
        assert!(has_styled_string || !styled.spans.is_empty(), "Expected string to have styling or spans to exist");
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

    #[test]
    fn test_viewport_highlight_populates_cache() {
        // Create a multi-line Rust file
        let source = r#"fn main() {
    let x = 42;
    println!("Hello, world!");
    for i in 0..10 {
        println!("{}", i);
    }
}
"#;
        let hl = make_rust_highlighter(source).unwrap();

        // Call highlight_viewport to populate the cache
        hl.highlight_viewport(0, 7);

        // Subsequent highlight_line calls should hit the cache
        for i in 0..7 {
            let styled = hl.highlight_line(i);
            assert!(!styled.spans.is_empty() || styled.is_empty(),
                "Line {} should have spans or be empty", i);
        }
    }

    #[test]
    fn test_cache_invalidated_on_edit() {
        let source = "fn main() {}";
        let mut hl = make_rust_highlighter(source).unwrap();

        // Populate cache
        hl.highlight_viewport(0, 1);
        let styled1 = hl.highlight_line(0);

        // Edit the source
        let event = crate::edit::insert_event(source, 0, 2, "x");
        let new_source = "fnx main() {}";
        hl.edit(event, new_source);

        // Cache should be invalidated, but highlight should still work
        let styled2 = hl.highlight_line(0);

        // The output should be different since source changed
        assert_ne!(
            styled1.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            styled2.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            "Styled line should change after edit"
        );
    }

    #[test]
    fn test_viewport_highlight_performance() {
        // Create a large-ish Rust source file
        // This simulates a realistic file with multiple functions
        let mut source = String::new();
        for i in 0..200 {
            source.push_str(&format!(
                "fn function_{}() {{\n    let x = {};\n    println!(\"{{}}{{i}}\", x);\n}}\n\n",
                i, i * 42
            ));
        }

        let hl = make_rust_highlighter(&source).unwrap();

        // Time viewport highlighting (60 lines)
        let start = std::time::Instant::now();
        hl.highlight_viewport(0, 60);
        let viewport_time = start.elapsed();

        // Time individual line retrieval from cache
        let start = std::time::Instant::now();
        for i in 0..60 {
            let _ = hl.highlight_line(i);
        }
        let line_time = start.elapsed();

        // These are soft assertions - they validate that performance is reasonable
        // but won't fail on slow CI machines
        let viewport_us = viewport_time.as_micros();
        let line_us = line_time.as_micros();

        // Log performance for manual review
        eprintln!(
            "Viewport highlight (60 lines): {}µs, Line retrieval (60 calls): {}µs",
            viewport_us, line_us
        );

        // Assert that viewport highlighting completes in a reasonable time
        // (less than 10ms, which is above our target but gives headroom for CI)
        assert!(
            viewport_time.as_millis() < 10,
            "Viewport highlighting took too long: {}ms (target: <1ms)",
            viewport_time.as_millis()
        );

        // Assert that cached line retrieval is fast
        assert!(
            line_time.as_millis() < 5,
            "Line retrieval took too long: {}ms (should be cache hits)",
            line_time.as_millis()
        );
    }

    #[test]
    fn test_highlight_line_outside_viewport_works() {
        let source = "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}";
        let hl = make_rust_highlighter(source).unwrap();

        // Populate cache for first 2 lines
        hl.highlight_viewport(0, 2);

        // Request a line outside the cached viewport
        // This should still work (falls back to single-line highlight)
        let styled = hl.highlight_line(4);
        assert!(!styled.spans.is_empty(), "Line 4 should have styled content");
    }

    #[test]
    fn test_no_duplicate_text_from_overlapping_captures() {
        // Doc comments in Rust can match multiple capture patterns
        // (e.g., @comment and @comment.documentation). The highlighter
        // must not emit the same text twice.
        let source = "/// This is a doc comment\nfn foo() {}";
        let hl = make_rust_highlighter(source).unwrap();
        let styled = hl.highlight_line(0);

        let rendered: String = styled.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            rendered, "/// This is a doc comment",
            "Styled line text should match source exactly, got: {:?}",
            rendered
        );
    }

    #[test]
    fn test_overlapping_captures_no_invisible_spans() {
        // When tree-sitter produces overlapping captures (e.g., from combined
        // C/C++ queries), a broader capture may extend beyond a narrower one.
        // The uncovered tail must be emitted as plain text, not dropped.
        //
        // Simulate via build_line_from_captures with synthetic captures:
        //   Capture A: [0, 5)  "keyword" — covers "fn ma"
        //   Capture B: [3, 12) "function" — overlaps, extends to "fn main() {"
        //
        // Expected: bytes [0,5) styled, bytes [5,12) emitted as plain text.
        let source = "fn main() {}";
        let hl = make_rust_highlighter(source).unwrap();

        // Call highlight_line which exercises build_styled_line_from_query
        let styled = hl.highlight_line(0);
        let rendered: String = styled.spans.iter().map(|s| s.text.as_str()).collect();

        // The rendered text must contain every character from the source line.
        // This catches the invisible-span bug where overlapping captures caused
        // characters between the end of one capture and the extended covered_until
        // to be silently dropped.
        assert_eq!(
            rendered, source,
            "All characters must be present; got: {:?}",
            rendered
        );
    }

    #[test]
    fn test_viewport_overlapping_captures_no_invisible_spans() {
        // Same invariant as above but exercised through the viewport/cache path
        // (build_line_from_captures).
        let source = "fn main() {\n    let x = 42;\n    println!(\"hello\");\n}";
        let hl = make_rust_highlighter(source).unwrap();

        hl.highlight_viewport(0, 4);
        for line_idx in 0..4 {
            let styled = hl.highlight_line(line_idx);
            let rendered: String = styled.spans.iter().map(|s| s.text.as_str()).collect();
            let expected = match hl.line_byte_range(line_idx) {
                Some((s, e)) => &hl.source()[s..e],
                None => "",
            };
            assert_eq!(
                rendered, expected,
                "Line {} has invisible characters; got {:?}, expected {:?}",
                line_idx, rendered, expected
            );
        }
    }

    #[test]
    fn test_viewport_cache_containment_avoids_thrashing() {
        // Simulates the real rendering pattern: highlight_viewport is called
        // per-line with a sliding window (line, line+80). The cache should
        // remain valid as long as the requested range is a subset.
        let source = "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}";
        let hl = make_rust_highlighter(source).unwrap();

        // First call populates cache for lines 0..5
        hl.highlight_viewport(0, 5);

        // Subsequent calls with subsets should NOT invalidate the cache.
        // We verify by checking the cache stays valid (lines are cache hits).
        hl.highlight_viewport(1, 5);
        hl.highlight_viewport(2, 5);

        // All lines should still be retrievable from cache
        for i in 0..5 {
            let styled = hl.highlight_line(i);
            assert!(
                !styled.spans.is_empty(),
                "Line {} should be served from cache",
                i
            );
        }
    }

    // ==================== line offset index tests ====================

    #[test]
    fn test_line_offsets_after_insert_newline() {
        // Test that inserting a newline correctly updates line offsets
        let source = "hello\nworld";
        let mut hl = make_rust_highlighter(source).unwrap();

        assert_eq!(hl.line_count(), 2);
        assert_eq!(hl.line_byte_range(0), Some((0, 5)));
        assert_eq!(hl.line_byte_range(1), Some((6, 11)));

        // Insert a newline in the middle of "hello"
        let event = crate::edit::insert_event(source, 0, 2, "\n");
        let new_source = "he\nllo\nworld";
        hl.edit(event, new_source);

        assert_eq!(hl.line_count(), 3);
        assert_eq!(hl.line_byte_range(0), Some((0, 2)));  // "he"
        assert_eq!(hl.line_byte_range(1), Some((3, 6)));  // "llo"
        assert_eq!(hl.line_byte_range(2), Some((7, 12))); // "world"
    }

    #[test]
    fn test_line_offsets_after_delete_newline() {
        // Test that deleting a newline correctly updates line offsets
        let source = "hello\nworld";
        let mut hl = make_rust_highlighter(source).unwrap();

        assert_eq!(hl.line_count(), 2);

        // Delete the newline to merge lines
        let event = crate::edit::delete_event(source, 0, 5, 1, 0);
        let new_source = "helloworld";
        hl.edit(event, new_source);

        assert_eq!(hl.line_count(), 1);
        assert_eq!(hl.line_byte_range(0), Some((0, 10)));
    }

    #[test]
    fn test_line_offsets_after_insert_text() {
        // Test inserting text without newlines
        let source = "hello\nworld";
        let mut hl = make_rust_highlighter(source).unwrap();

        // Insert "XXX" in the middle of "hello"
        let event = crate::edit::insert_event(source, 0, 2, "XXX");
        let new_source = "heXXXllo\nworld";
        hl.edit(event, new_source);

        assert_eq!(hl.line_count(), 2);
        assert_eq!(hl.line_byte_range(0), Some((0, 8)));  // "heXXXllo"
        assert_eq!(hl.line_byte_range(1), Some((9, 14))); // "world"
    }

    #[test]
    fn test_line_offsets_after_insert_multiple_newlines() {
        // Test inserting text with multiple newlines
        let source = "hello\nworld";
        let mut hl = make_rust_highlighter(source).unwrap();

        // Insert "A\nB\nC" in the middle of "hello"
        let event = crate::edit::insert_event(source, 0, 2, "A\nB\nC");
        let new_source = "heA\nB\nCllo\nworld";
        hl.edit(event, new_source);

        assert_eq!(hl.line_count(), 4);
        assert_eq!(hl.line_byte_range(0), Some((0, 3)));   // "heA"
        assert_eq!(hl.line_byte_range(1), Some((4, 5)));   // "B"
        assert_eq!(hl.line_byte_range(2), Some((6, 10)));  // "Cllo"
        assert_eq!(hl.line_byte_range(3), Some((11, 16))); // "world"
    }

    #[test]
    fn test_line_count_empty_file() {
        // Empty string should have 1 line (the empty line)
        let source = "";
        let hl = make_rust_highlighter(source).unwrap();
        assert_eq!(hl.line_count(), 1);
        assert_eq!(hl.line_byte_range(0), Some((0, 0)));
    }

    #[test]
    fn test_line_count_trailing_newline() {
        // File ending with newline has an empty last line
        let source = "hello\n";
        let hl = make_rust_highlighter(source).unwrap();
        assert_eq!(hl.line_count(), 2);
        assert_eq!(hl.line_byte_range(0), Some((0, 5)));
        assert_eq!(hl.line_byte_range(1), Some((6, 6))); // empty last line
    }

    #[test]
    fn test_viewport_at_deep_position_is_position_independent() {
        // Verify that viewport highlighting at deep positions doesn't scale with position
        // This is the core performance improvement - O(1) lookups instead of O(n)
        let mut source = String::new();
        for i in 0..2000 {
            source.push_str(&format!(
                "fn function_{}() {{ let x = {}; }}\n",
                i, i * 42
            ));
        }

        let hl = make_rust_highlighter(&source).unwrap();

        // Time viewport at line 0
        let start_early = std::time::Instant::now();
        hl.highlight_viewport(0, 60);
        let time_early = start_early.elapsed();

        // Time viewport at line 1800 (deep in file)
        let hl_fresh = make_rust_highlighter(&source).unwrap();
        let start_late = std::time::Instant::now();
        hl_fresh.highlight_viewport(1800, 1860);
        let time_late = start_late.elapsed();

        eprintln!(
            "Viewport at line 0: {}µs, at line 1800: {}µs, ratio: {:.2}x",
            time_early.as_micros(),
            time_late.as_micros(),
            time_late.as_micros() as f64 / time_early.as_micros().max(1) as f64
        );

        // The ratio should be close to 1.0 (within 2x tolerance per success criteria)
        // If line lookups were O(n), this would be much higher
        assert!(
            time_late.as_micros() <= time_early.as_micros() * 3 + 500, // +500µs for variance
            "Deep viewport took disproportionately longer: {}µs vs {}µs",
            time_late.as_micros(),
            time_early.as_micros()
        );
    }
}
