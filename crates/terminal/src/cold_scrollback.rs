// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
//! Cold scrollback storage for terminal scrollback history.
//!
//! This module implements a tiered storage system for terminal scrollback that
//! caps memory usage regardless of history length. As lines scroll off the
//! in-memory buffer, they are serialized to a temp file. When the user scrolls
//! into the cold region, lines are paged from disk.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────┐
//! │   Viewport (40 lines)   │  alacritty_terminal grid (always in memory)
//! ├─────────────────────────┤
//! │ Recent cache (~2K lines) │  alacritty_terminal scrollback (in memory)
//! ├─────────────────────────┤
//! │  Cold scrollback (file)  │  This module — StyledLines on disk
//! └─────────────────────────┘
//! ```
//!
//! # On-disk format
//!
//! Lines are stored sequentially with a compact binary format:
//! - Each line is prefixed with its total byte length (u32)
//! - Spans are serialized with their text (length-prefixed UTF-8) and style
//! - Colors use variable-length encoding (1-4 bytes)
//!
//! A typical 120-char line compresses from 2,880 bytes (cell grid) to ~150-200 bytes.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::time::Instant;

use lite_edit_buffer::{Color, NamedColor, Span, Style, StyledLine, UnderlineStyle};

// =============================================================================
// Serialization Format
// =============================================================================

/// Serializes a StyledLine to a compact binary format.
///
/// Format:
/// ```text
/// u32: total_length (bytes for entire record, including this field)
/// u16: num_spans
/// For each span:
///   u16: text_length (bytes)
///   [u8]: UTF-8 text data
///   u8: style_flags bitfield
///        bit 0: bold
///        bit 1: italic
///        bit 2: dim
///        bit 3: strikethrough
///        bit 4: inverse
///        bit 5: hidden
///        bit 6: has_underline (underline != None)
///        bit 7: has_fg_color (fg != Default)
///   u8: extended_flags (if style_flags != 0 or has colors)
///        bit 0: has_bg_color (bg != Default)
///        bit 1: has_underline_color
///   [underline_style]: u8 (if has_underline)
///   [color]: fg_color (if has_fg_color)
///   [color]: bg_color (if has_bg_color)
///   [color]: underline_color (if has_underline_color)
/// ```
pub fn serialize_styled_line(line: &StyledLine) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);

    // Reserve space for total length (will fill in at the end)
    buf.extend_from_slice(&[0u8; 4]);

    // Number of spans
    let num_spans = line.spans.len().min(u16::MAX as usize) as u16;
    buf.extend_from_slice(&num_spans.to_le_bytes());

    for span in &line.spans {
        serialize_span(&mut buf, span);
    }

    // Fill in total length
    let total_len = buf.len() as u32;
    buf[0..4].copy_from_slice(&total_len.to_le_bytes());

    buf
}

fn serialize_span(buf: &mut Vec<u8>, span: &Span) {
    // Text length and data
    let text_bytes = span.text.as_bytes();
    let text_len = text_bytes.len().min(u16::MAX as usize) as u16;
    buf.extend_from_slice(&text_len.to_le_bytes());
    buf.extend_from_slice(&text_bytes[..text_len as usize]);

    let style = &span.style;

    // Build style flags
    let has_underline = style.underline != UnderlineStyle::None;
    let has_fg = style.fg != Color::Default;
    let has_bg = style.bg != Color::Default;
    let has_underline_color = style.underline_color.is_some();

    let style_flags: u8 = (style.bold as u8)
        | ((style.italic as u8) << 1)
        | ((style.dim as u8) << 2)
        | ((style.strikethrough as u8) << 3)
        | ((style.inverse as u8) << 4)
        | ((style.hidden as u8) << 5)
        | ((has_underline as u8) << 6)
        | ((has_fg as u8) << 7);

    buf.push(style_flags);

    // Extended flags (only if needed)
    let extended_flags: u8 = (has_bg as u8) | ((has_underline_color as u8) << 1);
    buf.push(extended_flags);

    // Underline style
    if has_underline {
        buf.push(underline_style_to_u8(style.underline));
    }

    // Colors
    if has_fg {
        serialize_color(buf, &style.fg);
    }
    if has_bg {
        serialize_color(buf, &style.bg);
    }
    if has_underline_color {
        serialize_color(buf, style.underline_color.as_ref().unwrap());
    }
}

fn underline_style_to_u8(style: UnderlineStyle) -> u8 {
    match style {
        UnderlineStyle::None => 0,
        UnderlineStyle::Single => 1,
        UnderlineStyle::Double => 2,
        UnderlineStyle::Curly => 3,
        UnderlineStyle::Dotted => 4,
        UnderlineStyle::Dashed => 5,
    }
}

fn u8_to_underline_style(byte: u8) -> UnderlineStyle {
    match byte {
        1 => UnderlineStyle::Single,
        2 => UnderlineStyle::Double,
        3 => UnderlineStyle::Curly,
        4 => UnderlineStyle::Dotted,
        5 => UnderlineStyle::Dashed,
        _ => UnderlineStyle::None,
    }
}

/// Color encoding (variable 1-4 bytes):
/// - 0x00: Default
/// - 0x01 + idx: Indexed(idx) [2 bytes total]
/// - 0x02 + r + g + b: Rgb [4 bytes total]
/// - 0x10-0x1F: Named colors [1 byte]
fn serialize_color(buf: &mut Vec<u8>, color: &Color) {
    match color {
        Color::Default => buf.push(0x00),
        Color::Indexed(idx) => {
            buf.push(0x01);
            buf.push(*idx);
        }
        Color::Rgb { r, g, b } => {
            buf.push(0x02);
            buf.push(*r);
            buf.push(*g);
            buf.push(*b);
        }
        Color::Named(named) => {
            buf.push(0x10 + named_color_to_u8(*named));
        }
    }
}

fn named_color_to_u8(color: NamedColor) -> u8 {
    match color {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
    }
}

fn u8_to_named_color(byte: u8) -> NamedColor {
    match byte {
        0 => NamedColor::Black,
        1 => NamedColor::Red,
        2 => NamedColor::Green,
        3 => NamedColor::Yellow,
        4 => NamedColor::Blue,
        5 => NamedColor::Magenta,
        6 => NamedColor::Cyan,
        7 => NamedColor::White,
        8 => NamedColor::BrightBlack,
        9 => NamedColor::BrightRed,
        10 => NamedColor::BrightGreen,
        11 => NamedColor::BrightYellow,
        12 => NamedColor::BrightBlue,
        13 => NamedColor::BrightMagenta,
        14 => NamedColor::BrightCyan,
        _ => NamedColor::BrightWhite,
    }
}

/// Deserializes a StyledLine from the binary format.
///
/// Returns an error if the data is malformed.
pub fn deserialize_styled_line(data: &[u8]) -> io::Result<StyledLine> {
    if data.len() < 6 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data too short for StyledLine header",
        ));
    }

    let mut offset = 0;

    // Total length (we already have the data, but validate)
    let total_len = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
    offset += 4;

    if data.len() < total_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data shorter than declared length",
        ));
    }

    // Number of spans
    let num_spans = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()) as usize;
    offset += 2;

    let mut spans = Vec::with_capacity(num_spans);

    for _ in 0..num_spans {
        let (span, bytes_read) = deserialize_span(&data[offset..])?;
        spans.push(span);
        offset += bytes_read;
    }

    Ok(StyledLine::new(spans))
}

fn deserialize_span(data: &[u8]) -> io::Result<(Span, usize)> {
    if data.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data too short for span header",
        ));
    }

    let mut offset = 0;

    // Text length and data
    let text_len = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()) as usize;
    offset += 2;

    if data.len() < offset + text_len + 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data too short for span text",
        ));
    }

    let text = String::from_utf8_lossy(&data[offset..offset + text_len]).to_string();
    offset += text_len;

    // Style flags
    let style_flags = data[offset];
    offset += 1;

    let extended_flags = data[offset];
    offset += 1;

    let bold = (style_flags & 0x01) != 0;
    let italic = (style_flags & 0x02) != 0;
    let dim = (style_flags & 0x04) != 0;
    let strikethrough = (style_flags & 0x08) != 0;
    let inverse = (style_flags & 0x10) != 0;
    let hidden = (style_flags & 0x20) != 0;
    let has_underline = (style_flags & 0x40) != 0;
    let has_fg = (style_flags & 0x80) != 0;

    let has_bg = (extended_flags & 0x01) != 0;
    let has_underline_color = (extended_flags & 0x02) != 0;

    // Underline style
    let underline = if has_underline {
        let style = u8_to_underline_style(data[offset]);
        offset += 1;
        style
    } else {
        UnderlineStyle::None
    };

    // Colors
    let (fg, fg_bytes) = if has_fg {
        let (c, b) = deserialize_color(&data[offset..])?;
        (c, b)
    } else {
        (Color::Default, 0)
    };
    offset += fg_bytes;

    let (bg, bg_bytes) = if has_bg {
        let (c, b) = deserialize_color(&data[offset..])?;
        (c, b)
    } else {
        (Color::Default, 0)
    };
    offset += bg_bytes;

    let (underline_color, uc_bytes) = if has_underline_color {
        let (c, b) = deserialize_color(&data[offset..])?;
        (Some(c), b)
    } else {
        (None, 0)
    };
    offset += uc_bytes;

    let style = Style {
        fg,
        bg,
        bold,
        italic,
        dim,
        underline,
        underline_color,
        strikethrough,
        inverse,
        hidden,
    };

    Ok((Span::new(text, style), offset))
}

fn deserialize_color(data: &[u8]) -> io::Result<(Color, usize)> {
    if data.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No data for color",
        ));
    }

    let tag = data[0];

    match tag {
        0x00 => Ok((Color::Default, 1)),
        0x01 => {
            if data.len() < 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Indexed color missing index",
                ));
            }
            Ok((Color::Indexed(data[1]), 2))
        }
        0x02 => {
            if data.len() < 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "RGB color missing components",
                ));
            }
            Ok((
                Color::Rgb {
                    r: data[1],
                    g: data[2],
                    b: data[3],
                },
                4,
            ))
        }
        0x10..=0x1F => {
            let named = u8_to_named_color(tag - 0x10);
            Ok((Color::Named(named), 1))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unknown color tag: {}", tag),
        )),
    }
}

// =============================================================================
// ColdScrollback - On-disk storage
// =============================================================================

/// Manages the on-disk cold scrollback storage.
///
/// Lines are appended sequentially. An in-memory index maps line numbers
/// to file offsets for random access.
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
pub struct ColdScrollback {
    /// The underlying file.
    file: File,
    /// Index of line offsets: line_offsets[i] = byte offset of line i in file.
    line_offsets: Vec<u64>,
    /// Total number of lines stored.
    line_count: usize,
    /// Current write position in the file.
    write_pos: u64,
}

impl ColdScrollback {
    /// Creates a new cold scrollback store with a temp file.
    pub fn new() -> io::Result<Self> {
        let file = tempfile::tempfile()?;

        Ok(Self {
            file,
            line_offsets: Vec::new(),
            line_count: 0,
            write_pos: 0,
        })
    }

    /// Appends a line to cold storage.
    pub fn append(&mut self, line: &StyledLine) -> io::Result<()> {
        let data = serialize_styled_line(line);

        // Record the offset before writing
        self.line_offsets.push(self.write_pos);

        // Seek to write position and write
        self.file.seek(SeekFrom::Start(self.write_pos))?;
        self.file.write_all(&data)?;

        self.write_pos += data.len() as u64;
        self.line_count += 1;

        Ok(())
    }

    /// Reads a line by index from cold storage.
    pub fn get(&mut self, line: usize) -> io::Result<StyledLine> {
        if line >= self.line_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Line index {} out of bounds (count: {})", line, self.line_count),
            ));
        }

        let offset = self.line_offsets[line];

        // Seek to the line's position
        self.file.seek(SeekFrom::Start(offset))?;

        // Read the length prefix
        let mut len_buf = [0u8; 4];
        self.file.read_exact(&mut len_buf)?;
        let total_len = u32::from_le_bytes(len_buf) as usize;

        // Read the full record
        let mut data = vec![0u8; total_len];
        data[0..4].copy_from_slice(&len_buf);
        self.file.read_exact(&mut data[4..])?;

        deserialize_styled_line(&data)
    }

    /// Returns the number of lines stored.
    #[allow(dead_code)]
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// Reads a range of lines for page cache loading.
    ///
    /// Returns lines in the range [start, min(start + count, line_count)).
    pub fn get_range(&mut self, start: usize, count: usize) -> io::Result<Vec<StyledLine>> {
        let end = (start + count).min(self.line_count);
        let mut lines = Vec::with_capacity(end - start);

        for i in start..end {
            lines.push(self.get(i)?);
        }

        Ok(lines)
    }
}

// =============================================================================
// PageCache - LRU cache for cold reads
// =============================================================================

/// A page of cached cold scrollback lines.
struct CachePage {
    /// First line index in this page.
    start_line: usize,
    /// Cached lines.
    lines: Vec<StyledLine>,
    /// Last access timestamp for eviction.
    last_access: Instant,
    /// Approximate memory size of this page.
    size_bytes: usize,
}

impl CachePage {
    fn new(start_line: usize, lines: Vec<StyledLine>) -> Self {
        let size_bytes: usize = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.len()).sum::<usize>() + 64)
            .sum();

        Self {
            start_line,
            lines,
            last_access: Instant::now(),
            size_bytes,
        }
    }

    fn get(&mut self, line: usize) -> Option<&StyledLine> {
        let offset = line.checked_sub(self.start_line)?;
        if offset < self.lines.len() {
            self.last_access = Instant::now();
            Some(&self.lines[offset])
        } else {
            None
        }
    }
}

/// Page cache for cold scrollback reads.
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
pub struct PageCache {
    /// Cached pages, keyed by page number (start_line / page_size).
    pages: HashMap<usize, CachePage>,
    /// Maximum cache size in bytes (approximate).
    max_bytes: usize,
    /// Current estimated size.
    current_bytes: usize,
    /// Page size in lines.
    page_size: usize,
}

impl PageCache {
    /// Creates a new page cache.
    ///
    /// # Arguments
    ///
    /// * `max_bytes` - Maximum approximate memory usage for cached pages
    /// * `page_size` - Number of lines per page
    pub fn new(max_bytes: usize, page_size: usize) -> Self {
        Self {
            pages: HashMap::new(),
            max_bytes,
            current_bytes: 0,
            page_size,
        }
    }

    /// Gets a line from cache or loads the page from cold storage.
    ///
    /// Returns a clone of the line (since we can't return a reference into
    /// the mutable cold scrollback).
    pub fn get(&mut self, line: usize, cold: &mut ColdScrollback) -> io::Result<StyledLine> {
        let page_num = line / self.page_size;

        // Check if page is in cache
        if let Some(page) = self.pages.get_mut(&page_num) {
            if let Some(styled_line) = page.get(line) {
                return Ok(styled_line.clone());
            }
        }

        // Page not in cache, load it
        let page_start = page_num * self.page_size;
        let lines = cold.get_range(page_start, self.page_size)?;

        if lines.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Line {} not found in cold storage", line),
            ));
        }

        let page = CachePage::new(page_start, lines);
        let page_size_bytes = page.size_bytes;

        // Evict old pages if needed
        while self.current_bytes + page_size_bytes > self.max_bytes && !self.pages.is_empty() {
            self.evict_oldest();
        }

        // Insert the new page
        self.current_bytes += page_size_bytes;
        self.pages.insert(page_num, page);

        // Get the line from the newly loaded page
        let page = self.pages.get_mut(&page_num).unwrap();
        page.get(line)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Line not in loaded page"))
    }

    /// Evicts the oldest accessed page.
    fn evict_oldest(&mut self) {
        if let Some((&oldest_page_num, _)) = self
            .pages
            .iter()
            .min_by_key(|(_, page)| page.last_access)
        {
            if let Some(removed) = self.pages.remove(&oldest_page_num) {
                self.current_bytes = self.current_bytes.saturating_sub(removed.size_bytes);
            }
        }
    }

    /// Clears all cached pages.
    pub fn invalidate(&mut self) {
        self.pages.clear();
        self.current_bytes = 0;
    }

    /// Returns the current cache size in bytes.
    #[cfg(test)]
    pub fn size_bytes(&self) -> usize {
        self.current_bytes
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Serialization Tests ====================

    #[test]
    fn test_serialize_empty_line() {
        let line = StyledLine::empty();
        let data = serialize_styled_line(&line);
        let decoded = deserialize_styled_line(&data).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_serialize_plain_text() {
        let line = StyledLine::plain("Hello, world!");
        let data = serialize_styled_line(&line);
        let decoded = deserialize_styled_line(&data).unwrap();

        assert_eq!(decoded.spans.len(), 1);
        assert_eq!(decoded.spans[0].text, "Hello, world!");
        assert_eq!(decoded.spans[0].style, Style::default());
    }

    #[test]
    fn test_serialize_styled_text() {
        let style = Style {
            bold: true,
            italic: true,
            dim: true,
            strikethrough: true,
            inverse: true,
            hidden: true,
            underline: UnderlineStyle::Curly,
            ..Style::default()
        };
        let line = StyledLine::new(vec![Span::new("styled", style)]);
        let data = serialize_styled_line(&line);
        let decoded = deserialize_styled_line(&data).unwrap();

        assert_eq!(decoded.spans.len(), 1);
        let decoded_style = &decoded.spans[0].style;
        assert!(decoded_style.bold);
        assert!(decoded_style.italic);
        assert!(decoded_style.dim);
        assert!(decoded_style.strikethrough);
        assert!(decoded_style.inverse);
        assert!(decoded_style.hidden);
        assert_eq!(decoded_style.underline, UnderlineStyle::Curly);
    }

    #[test]
    fn test_serialize_multiple_spans() {
        let line = StyledLine::new(vec![
            Span::plain("normal"),
            Span::new(
                "bold",
                Style {
                    bold: true,
                    ..Style::default()
                },
            ),
            Span::plain("more normal"),
        ]);
        let data = serialize_styled_line(&line);
        let decoded = deserialize_styled_line(&data).unwrap();

        assert_eq!(decoded.spans.len(), 3);
        assert_eq!(decoded.spans[0].text, "normal");
        assert!(!decoded.spans[0].style.bold);
        assert_eq!(decoded.spans[1].text, "bold");
        assert!(decoded.spans[1].style.bold);
        assert_eq!(decoded.spans[2].text, "more normal");
    }

    #[test]
    fn test_serialize_colors() {
        // Test all color types
        let line = StyledLine::new(vec![
            Span::new(
                "named",
                Style {
                    fg: Color::Named(NamedColor::Red),
                    bg: Color::Named(NamedColor::Blue),
                    ..Style::default()
                },
            ),
            Span::new(
                "indexed",
                Style {
                    fg: Color::Indexed(42),
                    ..Style::default()
                },
            ),
            Span::new(
                "rgb",
                Style {
                    fg: Color::Rgb {
                        r: 100,
                        g: 150,
                        b: 200,
                    },
                    ..Style::default()
                },
            ),
            Span::new(
                "underline_color",
                Style {
                    underline: UnderlineStyle::Single,
                    underline_color: Some(Color::Named(NamedColor::Yellow)),
                    ..Style::default()
                },
            ),
        ]);

        let data = serialize_styled_line(&line);
        let decoded = deserialize_styled_line(&data).unwrap();

        assert_eq!(decoded.spans.len(), 4);

        // Named colors
        assert_eq!(decoded.spans[0].style.fg, Color::Named(NamedColor::Red));
        assert_eq!(decoded.spans[0].style.bg, Color::Named(NamedColor::Blue));

        // Indexed color
        assert_eq!(decoded.spans[1].style.fg, Color::Indexed(42));

        // RGB color
        assert_eq!(
            decoded.spans[2].style.fg,
            Color::Rgb {
                r: 100,
                g: 150,
                b: 200
            }
        );

        // Underline color
        assert_eq!(decoded.spans[3].style.underline, UnderlineStyle::Single);
        assert_eq!(
            decoded.spans[3].style.underline_color,
            Some(Color::Named(NamedColor::Yellow))
        );
    }

    #[test]
    fn test_serialize_wide_chars() {
        // Test UTF-8 with CJK and emoji
        let line = StyledLine::plain("Hello \u{4E2D}\u{6587} World \u{1F600}");
        let data = serialize_styled_line(&line);
        let decoded = deserialize_styled_line(&data).unwrap();

        assert_eq!(decoded.spans.len(), 1);
        assert_eq!(decoded.spans[0].text, "Hello \u{4E2D}\u{6587} World \u{1F600}");
    }

    #[test]
    fn test_serialize_all_underline_styles() {
        for style in [
            UnderlineStyle::None,
            UnderlineStyle::Single,
            UnderlineStyle::Double,
            UnderlineStyle::Curly,
            UnderlineStyle::Dotted,
            UnderlineStyle::Dashed,
        ] {
            let line = StyledLine::new(vec![Span::new(
                "test",
                Style {
                    underline: style,
                    ..Style::default()
                },
            )]);
            let data = serialize_styled_line(&line);
            let decoded = deserialize_styled_line(&data).unwrap();
            assert_eq!(decoded.spans[0].style.underline, style);
        }
    }

    #[test]
    fn test_serialize_all_named_colors() {
        let colors = [
            NamedColor::Black,
            NamedColor::Red,
            NamedColor::Green,
            NamedColor::Yellow,
            NamedColor::Blue,
            NamedColor::Magenta,
            NamedColor::Cyan,
            NamedColor::White,
            NamedColor::BrightBlack,
            NamedColor::BrightRed,
            NamedColor::BrightGreen,
            NamedColor::BrightYellow,
            NamedColor::BrightBlue,
            NamedColor::BrightMagenta,
            NamedColor::BrightCyan,
            NamedColor::BrightWhite,
        ];

        for color in colors {
            let line = StyledLine::new(vec![Span::new(
                "test",
                Style {
                    fg: Color::Named(color),
                    ..Style::default()
                },
            )]);
            let data = serialize_styled_line(&line);
            let decoded = deserialize_styled_line(&data).unwrap();
            assert_eq!(decoded.spans[0].style.fg, Color::Named(color));
        }
    }

    // ==================== ColdScrollback Tests ====================

    #[test]
    fn test_cold_scrollback_append_and_get() {
        let mut cold = ColdScrollback::new().unwrap();

        let line1 = StyledLine::plain("Line 1");
        let line2 = StyledLine::plain("Line 2");
        let line3 = StyledLine::plain("Line 3");

        cold.append(&line1).unwrap();
        cold.append(&line2).unwrap();
        cold.append(&line3).unwrap();

        assert_eq!(cold.line_count(), 3);

        let read1 = cold.get(0).unwrap();
        let read2 = cold.get(1).unwrap();
        let read3 = cold.get(2).unwrap();

        assert_eq!(read1.spans[0].text, "Line 1");
        assert_eq!(read2.spans[0].text, "Line 2");
        assert_eq!(read3.spans[0].text, "Line 3");
    }

    #[test]
    fn test_cold_scrollback_many_lines() {
        let mut cold = ColdScrollback::new().unwrap();

        // Append 1000 lines
        for i in 0..1000 {
            let line = StyledLine::plain(format!("Line {:04}", i));
            cold.append(&line).unwrap();
        }

        assert_eq!(cold.line_count(), 1000);

        // Verify random access
        let line_0 = cold.get(0).unwrap();
        let line_500 = cold.get(500).unwrap();
        let line_999 = cold.get(999).unwrap();

        assert_eq!(line_0.spans[0].text, "Line 0000");
        assert_eq!(line_500.spans[0].text, "Line 0500");
        assert_eq!(line_999.spans[0].text, "Line 0999");
    }

    #[test]
    fn test_cold_scrollback_out_of_bounds() {
        let mut cold = ColdScrollback::new().unwrap();
        cold.append(&StyledLine::plain("test")).unwrap();

        assert!(cold.get(1).is_err());
        assert!(cold.get(100).is_err());
    }

    #[test]
    fn test_cold_scrollback_get_range() {
        let mut cold = ColdScrollback::new().unwrap();

        for i in 0..100 {
            cold.append(&StyledLine::plain(format!("Line {}", i))).unwrap();
        }

        let range = cold.get_range(10, 5).unwrap();
        assert_eq!(range.len(), 5);
        assert_eq!(range[0].spans[0].text, "Line 10");
        assert_eq!(range[4].spans[0].text, "Line 14");

        // Range past end of lines
        let range2 = cold.get_range(95, 10).unwrap();
        assert_eq!(range2.len(), 5); // Only 5 lines available
    }

    // ==================== PageCache Tests ====================

    #[test]
    fn test_page_cache_hit() {
        let mut cold = ColdScrollback::new().unwrap();
        for i in 0..100 {
            cold.append(&StyledLine::plain(format!("Line {}", i))).unwrap();
        }

        let mut cache = PageCache::new(1024 * 1024, 64);

        // First access loads the page
        let line1 = cache.get(10, &mut cold).unwrap();
        assert_eq!(line1.spans[0].text, "Line 10");

        // Second access should hit cache (same page)
        let line2 = cache.get(15, &mut cold).unwrap();
        assert_eq!(line2.spans[0].text, "Line 15");

        // Should have one page cached
        assert_eq!(cache.pages.len(), 1);
    }

    #[test]
    fn test_page_cache_miss() {
        let mut cold = ColdScrollback::new().unwrap();
        for i in 0..200 {
            cold.append(&StyledLine::plain(format!("Line {}", i))).unwrap();
        }

        let mut cache = PageCache::new(1024 * 1024, 64);

        // Access line in first page
        let _ = cache.get(10, &mut cold).unwrap();

        // Access line in second page (triggers new page load)
        let line = cache.get(100, &mut cold).unwrap();
        assert_eq!(line.spans[0].text, "Line 100");

        // Should have two pages cached
        assert_eq!(cache.pages.len(), 2);
    }

    #[test]
    fn test_page_cache_eviction() {
        let mut cold = ColdScrollback::new().unwrap();
        for i in 0..1000 {
            cold.append(&StyledLine::plain(format!("Line {:04}", i))).unwrap();
        }

        // Very small cache to force eviction
        let mut cache = PageCache::new(500, 16);

        // Load multiple pages to trigger eviction
        let _ = cache.get(0, &mut cold).unwrap();
        let _ = cache.get(100, &mut cold).unwrap();
        let _ = cache.get(200, &mut cold).unwrap();

        // Cache should have evicted some pages
        assert!(cache.pages.len() <= 3);
    }

    #[test]
    fn test_page_cache_invalidate() {
        let mut cold = ColdScrollback::new().unwrap();
        for i in 0..100 {
            cold.append(&StyledLine::plain(format!("Line {}", i))).unwrap();
        }

        let mut cache = PageCache::new(1024 * 1024, 64);

        let _ = cache.get(10, &mut cold).unwrap();
        assert!(!cache.pages.is_empty());

        cache.invalidate();
        assert!(cache.pages.is_empty());
        assert_eq!(cache.size_bytes(), 0);
    }

    // ==================== Size Reduction Tests ====================

    #[test]
    fn test_size_reduction_plain_text() {
        // A typical 120-char line in raw cell format would be:
        // 120 cells * 24 bytes/cell = 2880 bytes
        // Our format should be much smaller

        let line = StyledLine::plain("x".repeat(120));
        let data = serialize_styled_line(&line);

        // Expected: 4 (len) + 2 (spans) + 2 (text_len) + 120 (text) + 2 (flags) = 130 bytes
        // Cell grid would be ~2880 bytes
        // That's a 22x reduction
        assert!(data.len() < 300, "Serialized size {} should be < 300", data.len());
        assert!(data.len() * 10 < 2880, "Should achieve at least 10x reduction");
    }
}
