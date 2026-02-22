// Chunk: docs/chunks/buffer_view_trait - BufferView trait and supporting types
//!
//! Buffer view abstraction for rendering.
//!
//! This module defines the `BufferView` trait that unifies file editing buffers
//! and terminal emulator buffers behind a common rendering interface. This is
//! the keystone abstraction enabling the hierarchical terminal tabs architecture.
//!
//! # Overview
//!
//! The main trait is [`BufferView`], which provides:
//! - Line count and styled line access for rendering
//! - Dirty line tracking for efficient redraws
//! - Cursor information for display
//! - Editability flag to distinguish text buffers from terminal buffers
//!
//! The trait is object-safe, enabling `Box<dyn BufferView>` and `&dyn BufferView`
//! usage for polymorphic buffer handling.
//!
//! # Styling
//!
//! Text styling is terminal-grade from day one:
//! - [`Color`]: Named (16 ANSI), indexed (256), and RGB
//! - [`Style`]: Full terminal attributes (fg/bg, bold, italic, underline variants, etc.)
//! - [`Span`]: A run of text with uniform styling
//! - [`StyledLine`]: A sequence of spans comprising a single line

use crate::types::{DirtyLines, Position};

// =============================================================================
// Color Types
// =============================================================================

/// The 16 standard ANSI colors.
///
/// These correspond to colors 0-15 in the 256-color palette, but are
/// represented separately for clarity and to allow theme-based overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

/// Terminal color representation.
///
/// Supports all common terminal color modes:
/// - Default (let the terminal/theme decide)
/// - Named ANSI colors (16 colors)
/// - Indexed (256-color palette)
/// - True color RGB (24-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Default foreground/background (terminal decides).
    #[default]
    Default,
    /// Named ANSI colors (0-15).
    Named(NamedColor),
    /// 256-color palette index.
    Indexed(u8),
    /// 24-bit RGB color.
    Rgb { r: u8, g: u8, b: u8 },
}

// =============================================================================
// Underline Types
// =============================================================================

/// Underline rendering style.
///
/// Terminals support multiple underline styles beyond the simple single underline.
/// Modern terminal emulators (kitty, iTerm2, WezTerm) support all variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnderlineStyle {
    /// No underline.
    #[default]
    None,
    /// Single straight underline.
    Single,
    /// Double straight underline.
    Double,
    /// Curly/wavy underline (often used for spelling errors).
    Curly,
    /// Dotted underline.
    Dotted,
    /// Dashed underline.
    Dashed,
}

// =============================================================================
// Style
// =============================================================================

/// Terminal-grade text styling attributes.
///
/// This struct captures all text attributes supported by modern terminal
/// emulators. The design is intentionally complete from day one to avoid
/// breaking changes when adding terminal emulator support.
///
/// # Default
///
/// The default style is unstyled text: default colors, no attributes.
///
/// # Example
///
/// ```
/// use lite_edit_buffer::{Style, Color, NamedColor, UnderlineStyle};
///
/// // Create a bold red style
/// let style = Style {
///     fg: Color::Named(NamedColor::Red),
///     bold: true,
///     ..Style::default()
/// };
///
/// // Create an error style with curly underline
/// let error_style = Style {
///     underline: UnderlineStyle::Curly,
///     underline_color: Some(Color::Named(NamedColor::Red)),
///     ..Style::default()
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Style {
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Bold weight.
    pub bold: bool,
    /// Italic slant.
    pub italic: bool,
    /// Dim/faint intensity.
    pub dim: bool,
    /// Underline style.
    pub underline: UnderlineStyle,
    /// Underline color (None = use fg color).
    pub underline_color: Option<Color>,
    /// Strikethrough line.
    pub strikethrough: bool,
    /// Inverse video (swap fg/bg at render time).
    pub inverse: bool,
    /// Hidden text (don't render glyphs).
    pub hidden: bool,
}

// =============================================================================
// Span and StyledLine
// =============================================================================

/// A contiguous run of text with uniform styling.
///
/// Spans are the atoms of styled text. A line is composed of zero or more
/// spans, each with its own style.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The text content of this span.
    pub text: String,
    /// The style applied to this text.
    pub style: Style,
}

impl Span {
    /// Creates a new span with the given text and style.
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    /// Creates an unstyled span (default style).
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }
}

/// A line as the renderer sees it — a sequence of styled spans.
///
/// This is the primary unit of rendering: the renderer iterates over lines,
/// and for each line, iterates over spans to produce glyph quads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyledLine {
    /// The spans comprising this line.
    pub spans: Vec<Span>,
}

impl StyledLine {
    /// Creates a new styled line from spans.
    pub fn new(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    /// Creates a line with a single unstyled span.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span::plain(text)],
        }
    }

    /// Creates an empty line.
    pub fn empty() -> Self {
        Self { spans: vec![] }
    }

    /// Returns true if the line has no spans.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Returns the total character count across all spans.
    pub fn char_count(&self) -> usize {
        self.spans.iter().map(|s| s.text.chars().count()).sum()
    }
}

// =============================================================================
// Cursor Types
// =============================================================================

/// Cursor shape for rendering.
///
/// Different modes and contexts may want different cursor appearances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    /// Solid block cursor (default for most terminals).
    #[default]
    Block,
    /// Vertical line cursor (insert mode).
    Beam,
    /// Horizontal line at bottom of cell.
    Underline,
    /// Cursor is hidden.
    Hidden,
}

/// Information about the cursor for rendering.
///
/// This struct provides everything the renderer needs to draw the cursor:
/// position, visual shape, and blink state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorInfo {
    /// Position in the buffer (line, column).
    pub position: Position,
    /// Visual shape of the cursor.
    pub shape: CursorShape,
    /// Whether the cursor should blink.
    pub blinking: bool,
}

impl CursorInfo {
    /// Creates a new cursor info.
    pub fn new(position: Position, shape: CursorShape, blinking: bool) -> Self {
        Self {
            position,
            shape,
            blinking,
        }
    }

    /// Creates a default block cursor at the given position.
    pub fn block(position: Position) -> Self {
        Self {
            position,
            shape: CursorShape::Block,
            blinking: true,
        }
    }
}

// =============================================================================
// BufferView Trait
// =============================================================================

/// A buffer as seen by the renderer — the view abstraction that unifies
/// file buffers and terminal buffers.
///
/// This trait is object-safe: it can be used as `&dyn BufferView` or
/// `Box<dyn BufferView>`.
///
/// # Implementors
///
/// - `TextBuffer`: File editing buffer (returns single unstyled span per line)
/// - Future: Terminal emulator buffer (returns styled spans from PTY output)
///
/// # Example
///
/// ```ignore
/// fn render_buffer(view: &dyn BufferView) {
///     for line_idx in 0..view.line_count() {
///         if let Some(styled_line) = view.styled_line(line_idx) {
///             for span in styled_line.spans {
///                 render_span(&span);
///             }
///         }
///     }
/// }
/// ```
// Chunk: docs/chunks/buffer_view_trait - BufferView trait definition
pub trait BufferView {
    /// Returns the total number of lines available for display.
    fn line_count(&self) -> usize;

    /// Returns a styled representation of the given line.
    ///
    /// Returns `None` if the line index is out of bounds.
    fn styled_line(&self, line: usize) -> Option<StyledLine>;

    /// Returns the length of the specified line in characters.
    ///
    /// Returns 0 if the line index is out of bounds.
    fn line_len(&self, line: usize) -> usize;

    /// Drains accumulated dirty state since last call.
    ///
    /// Returns which lines need re-rendering. After calling this,
    /// the buffer's dirty state is reset.
    fn take_dirty(&mut self) -> DirtyLines;

    /// Returns whether this buffer accepts direct text input.
    ///
    /// - `true` for file editing buffers (TextBuffer)
    /// - `false` for terminal buffers (input goes to PTY stdin instead)
    fn is_editable(&self) -> bool;

    /// Returns cursor information for display.
    ///
    /// Returns `None` if the buffer has no cursor (e.g., read-only view).
    fn cursor_info(&self) -> Option<CursorInfo>;

    /// Returns selection range as (start, end) positions in document order.
    ///
    /// Returns `None` if there is no active selection. This is an optional
    /// feature that text editing buffers support. Terminal buffers may
    /// have different selection semantics.
    fn selection_range(&self) -> Option<(Position, Position)> {
        None
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Color Tests ====================

    #[test]
    fn test_color_default() {
        let color = Color::default();
        assert_eq!(color, Color::Default);
    }

    #[test]
    fn test_color_rgb() {
        let color = Color::Rgb {
            r: 255,
            g: 128,
            b: 0,
        };
        assert!(matches!(color, Color::Rgb { r: 255, g: 128, b: 0 }));
    }

    #[test]
    fn test_color_indexed() {
        let color = Color::Indexed(42);
        assert_eq!(color, Color::Indexed(42));
    }

    #[test]
    fn test_color_named() {
        let color = Color::Named(NamedColor::Red);
        assert_eq!(color, Color::Named(NamedColor::Red));
    }

    #[test]
    fn test_color_equality() {
        assert_eq!(Color::Default, Color::Default);
        assert_ne!(Color::Default, Color::Indexed(0));
        assert_ne!(
            Color::Named(NamedColor::Red),
            Color::Named(NamedColor::Blue)
        );
        assert_eq!(
            Color::Rgb { r: 1, g: 2, b: 3 },
            Color::Rgb { r: 1, g: 2, b: 3 }
        );
    }

    // ==================== UnderlineStyle Tests ====================

    #[test]
    fn test_underline_style_default() {
        let style = UnderlineStyle::default();
        assert_eq!(style, UnderlineStyle::None);
    }

    #[test]
    fn test_underline_style_variants() {
        assert_eq!(UnderlineStyle::Single, UnderlineStyle::Single);
        assert_ne!(UnderlineStyle::Single, UnderlineStyle::Double);
        assert_ne!(UnderlineStyle::Curly, UnderlineStyle::Dotted);
    }

    // ==================== Style Tests ====================

    #[test]
    fn test_style_default() {
        let style = Style::default();
        assert_eq!(style.fg, Color::Default);
        assert_eq!(style.bg, Color::Default);
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.dim);
        assert_eq!(style.underline, UnderlineStyle::None);
        assert!(style.underline_color.is_none());
        assert!(!style.strikethrough);
        assert!(!style.inverse);
        assert!(!style.hidden);
    }

    #[test]
    fn test_style_equality() {
        let style1 = Style::default();
        let style2 = Style::default();
        assert_eq!(style1, style2);

        let style3 = Style {
            bold: true,
            ..Style::default()
        };
        assert_ne!(style1, style3);
    }

    #[test]
    fn test_style_with_attributes() {
        let style = Style {
            fg: Color::Named(NamedColor::Red),
            bg: Color::Rgb { r: 0, g: 0, b: 0 },
            bold: true,
            italic: true,
            underline: UnderlineStyle::Curly,
            underline_color: Some(Color::Named(NamedColor::Yellow)),
            ..Style::default()
        };

        assert_eq!(style.fg, Color::Named(NamedColor::Red));
        assert_eq!(style.bg, Color::Rgb { r: 0, g: 0, b: 0 });
        assert!(style.bold);
        assert!(style.italic);
        assert!(!style.dim);
        assert_eq!(style.underline, UnderlineStyle::Curly);
        assert_eq!(
            style.underline_color,
            Some(Color::Named(NamedColor::Yellow))
        );
    }

    // ==================== Span Tests ====================

    #[test]
    fn test_span_plain() {
        let span = Span::plain("hello");
        assert_eq!(span.text, "hello");
        assert_eq!(span.style, Style::default());
    }

    #[test]
    fn test_span_new() {
        let style = Style {
            bold: true,
            ..Style::default()
        };
        let span = Span::new("world", style);
        assert_eq!(span.text, "world");
        assert!(span.style.bold);
    }

    #[test]
    fn test_span_from_string() {
        let span = Span::plain(String::from("owned"));
        assert_eq!(span.text, "owned");
    }

    // ==================== StyledLine Tests ====================

    #[test]
    fn test_styled_line_plain() {
        let line = StyledLine::plain("hello");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].text, "hello");
        assert_eq!(line.spans[0].style, Style::default());
    }

    #[test]
    fn test_styled_line_empty() {
        let line = StyledLine::empty();
        assert!(line.spans.is_empty());
        assert!(line.is_empty());
    }

    #[test]
    fn test_styled_line_char_count() {
        let line = StyledLine::new(vec![
            Span::plain("hello"),
            Span::plain(" "),
            Span::plain("world"),
        ]);
        assert_eq!(line.char_count(), 11);
    }

    #[test]
    fn test_styled_line_new() {
        let spans = vec![
            Span::new(
                "error",
                Style {
                    fg: Color::Named(NamedColor::Red),
                    ..Style::default()
                },
            ),
            Span::plain(": "),
            Span::plain("something went wrong"),
        ];
        let line = StyledLine::new(spans);
        assert_eq!(line.spans.len(), 3);
        assert!(!line.is_empty());
    }

    // ==================== CursorShape Tests ====================

    #[test]
    fn test_cursor_shape_default() {
        let shape = CursorShape::default();
        assert_eq!(shape, CursorShape::Block);
    }

    #[test]
    fn test_cursor_shape_variants() {
        assert_eq!(CursorShape::Block, CursorShape::Block);
        assert_ne!(CursorShape::Block, CursorShape::Beam);
        assert_ne!(CursorShape::Underline, CursorShape::Hidden);
    }

    // ==================== CursorInfo Tests ====================

    #[test]
    fn test_cursor_info_block() {
        let pos = Position::new(5, 10);
        let cursor = CursorInfo::block(pos);

        assert_eq!(cursor.position, pos);
        assert_eq!(cursor.shape, CursorShape::Block);
        assert!(cursor.blinking);
    }

    #[test]
    fn test_cursor_info_new() {
        let pos = Position::new(3, 7);
        let cursor = CursorInfo::new(pos, CursorShape::Beam, false);

        assert_eq!(cursor.position, pos);
        assert_eq!(cursor.shape, CursorShape::Beam);
        assert!(!cursor.blinking);
    }

    // ==================== BufferView Object-Safety Tests ====================

    /// Mock implementation to verify trait object-safety.
    struct MockBufferView {
        lines: Vec<String>,
        dirty: DirtyLines,
    }

    impl MockBufferView {
        fn new(lines: Vec<&str>) -> Self {
            Self {
                lines: lines.into_iter().map(String::from).collect(),
                dirty: DirtyLines::None,
            }
        }
    }

    impl BufferView for MockBufferView {
        fn line_count(&self) -> usize {
            self.lines.len()
        }

        fn styled_line(&self, line: usize) -> Option<StyledLine> {
            self.lines.get(line).map(|s| StyledLine::plain(s.clone()))
        }

        fn line_len(&self, line: usize) -> usize {
            self.lines.get(line).map(|s| s.chars().count()).unwrap_or(0)
        }

        fn take_dirty(&mut self) -> DirtyLines {
            std::mem::take(&mut self.dirty)
        }

        fn is_editable(&self) -> bool {
            true
        }

        fn cursor_info(&self) -> Option<CursorInfo> {
            Some(CursorInfo::block(Position::new(0, 0)))
        }
    }

    #[test]
    fn test_buffer_view_object_safe_box() {
        // Verify we can create Box<dyn BufferView>
        let mock = MockBufferView::new(vec!["line one", "line two"]);
        let boxed: Box<dyn BufferView> = Box::new(mock);

        assert_eq!(boxed.line_count(), 2);
        assert_eq!(
            boxed.styled_line(0),
            Some(StyledLine::plain("line one"))
        );
        assert_eq!(
            boxed.styled_line(1),
            Some(StyledLine::plain("line two"))
        );
        assert!(boxed.styled_line(2).is_none());
    }

    #[test]
    fn test_buffer_view_object_safe_ref() {
        // Verify we can use &dyn BufferView
        let mock = MockBufferView::new(vec!["hello"]);
        let view_ref: &dyn BufferView = &mock;

        assert_eq!(view_ref.line_count(), 1);
        assert!(view_ref.is_editable());
        assert!(view_ref.cursor_info().is_some());
    }

    #[test]
    fn test_buffer_view_take_dirty() {
        let mut mock = MockBufferView {
            lines: vec![String::from("test")],
            dirty: DirtyLines::Single(0),
        };

        let dirty = mock.take_dirty();
        assert_eq!(dirty, DirtyLines::Single(0));

        // After take, should be None
        let dirty2 = mock.take_dirty();
        assert_eq!(dirty2, DirtyLines::None);
    }
}
