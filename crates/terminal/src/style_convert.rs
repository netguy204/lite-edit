// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
//! Cell to Style conversion.
//!
//! This module converts alacritty_terminal's cell types to our Style/Span/StyledLine types.

use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color as VteColor, NamedColor as VteNamedColor};
use alacritty_terminal::term::cell::Flags;

use lite_edit_buffer::{Color, NamedColor, Span, Style, StyledLine, UnderlineStyle};

/// Convert an alacritty vte Color to our Color type.
pub fn convert_color(color: VteColor) -> Color {
    match color {
        VteColor::Named(named) => Color::Named(convert_named_color(named)),
        VteColor::Indexed(idx) => Color::Indexed(idx),
        VteColor::Spec(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}

/// Convert an alacritty vte NamedColor to our NamedColor type.
fn convert_named_color(named: VteNamedColor) -> NamedColor {
    match named {
        VteNamedColor::Black => NamedColor::Black,
        VteNamedColor::Red => NamedColor::Red,
        VteNamedColor::Green => NamedColor::Green,
        VteNamedColor::Yellow => NamedColor::Yellow,
        VteNamedColor::Blue => NamedColor::Blue,
        VteNamedColor::Magenta => NamedColor::Magenta,
        VteNamedColor::Cyan => NamedColor::Cyan,
        VteNamedColor::White => NamedColor::White,
        VteNamedColor::BrightBlack => NamedColor::BrightBlack,
        VteNamedColor::BrightRed => NamedColor::BrightRed,
        VteNamedColor::BrightGreen => NamedColor::BrightGreen,
        VteNamedColor::BrightYellow => NamedColor::BrightYellow,
        VteNamedColor::BrightBlue => NamedColor::BrightBlue,
        VteNamedColor::BrightMagenta => NamedColor::BrightMagenta,
        VteNamedColor::BrightCyan => NamedColor::BrightCyan,
        VteNamedColor::BrightWhite => NamedColor::BrightWhite,
        // Default colors - map to Default
        VteNamedColor::Foreground => NamedColor::White,
        VteNamedColor::Background => NamedColor::Black,
        VteNamedColor::Cursor => NamedColor::White,
        VteNamedColor::DimBlack => NamedColor::Black,
        VteNamedColor::DimRed => NamedColor::Red,
        VteNamedColor::DimGreen => NamedColor::Green,
        VteNamedColor::DimYellow => NamedColor::Yellow,
        VteNamedColor::DimBlue => NamedColor::Blue,
        VteNamedColor::DimMagenta => NamedColor::Magenta,
        VteNamedColor::DimCyan => NamedColor::Cyan,
        VteNamedColor::DimWhite => NamedColor::White,
        VteNamedColor::BrightForeground => NamedColor::BrightWhite,
        VteNamedColor::DimForeground => NamedColor::White,
    }
}

/// Convert alacritty cell flags to underline style.
fn flags_to_underline_style(flags: Flags) -> UnderlineStyle {
    if flags.contains(Flags::DOUBLE_UNDERLINE) {
        UnderlineStyle::Double
    } else if flags.contains(Flags::UNDERCURL) {
        UnderlineStyle::Curly
    } else if flags.contains(Flags::DOTTED_UNDERLINE) {
        UnderlineStyle::Dotted
    } else if flags.contains(Flags::DASHED_UNDERLINE) {
        UnderlineStyle::Dashed
    } else if flags.contains(Flags::UNDERLINE) {
        UnderlineStyle::Single
    } else {
        UnderlineStyle::None
    }
}

/// Convert a cell to our Style type.
// Chunk: docs/chunks/terminal_styling_fidelity - Terminal cell to Style conversion with proper DIM flag detection
pub fn cell_to_style(cell: &Cell) -> Style {
    let flags = cell.flags;

    // Handle foreground color, considering if it should be the default
    let fg = if cell.fg == VteColor::Named(VteNamedColor::Foreground) {
        Color::Default
    } else {
        convert_color(cell.fg)
    };

    // Handle background color, considering if it should be the default
    let bg = if cell.bg == VteColor::Named(VteNamedColor::Background) {
        Color::Default
    } else {
        convert_color(cell.bg)
    };

    // Get underline color if set
    let underline_color = cell.underline_color().map(convert_color);

    Style {
        fg,
        bg,
        bold: flags.contains(Flags::BOLD),
        italic: flags.contains(Flags::ITALIC),
        dim: flags.contains(Flags::DIM),
        underline: flags_to_underline_style(flags),
        underline_color,
        strikethrough: flags.contains(Flags::STRIKEOUT),
        inverse: flags.contains(Flags::INVERSE),
        hidden: flags.contains(Flags::HIDDEN),
    }
}

/// Convert a row of cells to a StyledLine.
///
/// This function iterates through cells, coalescing adjacent cells with identical
/// styles into spans. It handles wide characters (WIDE_CHAR flag) and skips
/// the spacer cells that follow wide characters (WIDE_CHAR_SPACER flag).
// Chunk: docs/chunks/terminal_styling_fidelity - Row-to-StyledLine conversion preserving per-span styles
pub fn row_to_styled_line<'a, I>(cells: I, num_cols: usize) -> StyledLine
where
    I: IntoIterator<Item = &'a Cell>,
{
    let mut spans: Vec<Span> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;
    let mut col = 0;

    for cell in cells {
        if col >= num_cols {
            break;
        }

        let flags = cell.flags;

        // Skip spacer cells that follow wide characters
        if flags.contains(Flags::WIDE_CHAR_SPACER) {
            col += 1;
            continue;
        }

        let style = cell_to_style(cell);
        let ch = cell.c;

        // Get the character to render
        let char_str = if ch == ' ' || ch == '\0' {
            // Space or null character
            " ".to_string()
        } else {
            ch.to_string()
        };

        // Check if we can coalesce with the current span
        match &current_style {
            Some(s) if *s == style => {
                // Same style, extend the current span
                current_text.push_str(&char_str);
            }
            _ => {
                // Different style, flush current span if any
                if let Some(s) = current_style.take() {
                    if !current_text.is_empty() {
                        spans.push(Span::new(std::mem::take(&mut current_text), s));
                    }
                }
                // Start a new span
                current_text = char_str;
                current_style = Some(style);
            }
        }

        col += 1;
    }

    // Flush remaining span
    if let Some(s) = current_style {
        if !current_text.is_empty() {
            spans.push(Span::new(current_text, s));
        }
    }

    // If no spans, return an empty line
    if spans.is_empty() {
        StyledLine::empty()
    } else {
        StyledLine::new(spans)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    // ==================== Color Conversion Tests ====================

    #[test]
    fn test_color_named_conversion() {
        // Test all 16 ANSI colors
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Black)),
            Color::Named(NamedColor::Black)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Red)),
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Green)),
            Color::Named(NamedColor::Green)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Yellow)),
            Color::Named(NamedColor::Yellow)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Blue)),
            Color::Named(NamedColor::Blue)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Magenta)),
            Color::Named(NamedColor::Magenta)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::Cyan)),
            Color::Named(NamedColor::Cyan)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::White)),
            Color::Named(NamedColor::White)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightBlack)),
            Color::Named(NamedColor::BrightBlack)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightRed)),
            Color::Named(NamedColor::BrightRed)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightGreen)),
            Color::Named(NamedColor::BrightGreen)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightYellow)),
            Color::Named(NamedColor::BrightYellow)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightBlue)),
            Color::Named(NamedColor::BrightBlue)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightMagenta)),
            Color::Named(NamedColor::BrightMagenta)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightCyan)),
            Color::Named(NamedColor::BrightCyan)
        );
        assert_eq!(
            convert_color(VteColor::Named(VteNamedColor::BrightWhite)),
            Color::Named(NamedColor::BrightWhite)
        );
    }

    #[test]
    fn test_color_indexed_conversion() {
        assert_eq!(convert_color(VteColor::Indexed(0)), Color::Indexed(0));
        assert_eq!(convert_color(VteColor::Indexed(127)), Color::Indexed(127));
        assert_eq!(convert_color(VteColor::Indexed(255)), Color::Indexed(255));
    }

    #[test]
    fn test_color_rgb_conversion() {
        let rgb = Rgb { r: 100, g: 150, b: 200 };
        assert_eq!(
            convert_color(VteColor::Spec(rgb)),
            Color::Rgb { r: 100, g: 150, b: 200 }
        );

        // Test edge cases
        let black = Rgb { r: 0, g: 0, b: 0 };
        assert_eq!(
            convert_color(VteColor::Spec(black)),
            Color::Rgb { r: 0, g: 0, b: 0 }
        );

        let white = Rgb { r: 255, g: 255, b: 255 };
        assert_eq!(
            convert_color(VteColor::Spec(white)),
            Color::Rgb { r: 255, g: 255, b: 255 }
        );
    }

    // ==================== Flags to Style Tests ====================

    #[test]
    fn test_flags_to_underline_style() {
        assert_eq!(flags_to_underline_style(Flags::empty()), UnderlineStyle::None);
        assert_eq!(flags_to_underline_style(Flags::UNDERLINE), UnderlineStyle::Single);
        assert_eq!(flags_to_underline_style(Flags::DOUBLE_UNDERLINE), UnderlineStyle::Double);
        assert_eq!(flags_to_underline_style(Flags::UNDERCURL), UnderlineStyle::Curly);
        assert_eq!(flags_to_underline_style(Flags::DOTTED_UNDERLINE), UnderlineStyle::Dotted);
        assert_eq!(flags_to_underline_style(Flags::DASHED_UNDERLINE), UnderlineStyle::Dashed);
    }

    // ==================== Row to StyledLine Tests ====================

    #[test]
    fn test_empty_row() {
        let cells: Vec<Cell> = Vec::new();
        let line = row_to_styled_line(cells.iter(), 80);
        assert!(line.is_empty());
    }

    #[test]
    fn test_simple_text_row() {
        // Create a simple row of cells with 'H', 'e', 'l', 'l', 'o'
        let text = "Hello";
        let cells: Vec<Cell> = text
            .chars()
            .map(|c| {
                let mut cell = Cell::default();
                cell.c = c;
                cell
            })
            .collect();

        let line = row_to_styled_line(cells.iter(), 80);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].text, "Hello");
    }

    // ==================== Step 1: Terminal Color Resolution Tests ====================
    // Chunk: docs/chunks/terminal_styling_fidelity - Tests for style_convert.rs color handling

    #[test]
    fn test_cell_to_style_named_foreground_colors() {
        // Test that cell_to_style correctly captures all 16 ANSI named colors
        let test_cases = [
            (VteNamedColor::Black, NamedColor::Black),
            (VteNamedColor::Red, NamedColor::Red),
            (VteNamedColor::Green, NamedColor::Green),
            (VteNamedColor::Yellow, NamedColor::Yellow),
            (VteNamedColor::Blue, NamedColor::Blue),
            (VteNamedColor::Magenta, NamedColor::Magenta),
            (VteNamedColor::Cyan, NamedColor::Cyan),
            (VteNamedColor::White, NamedColor::White),
            (VteNamedColor::BrightBlack, NamedColor::BrightBlack),
            (VteNamedColor::BrightRed, NamedColor::BrightRed),
            (VteNamedColor::BrightGreen, NamedColor::BrightGreen),
            (VteNamedColor::BrightYellow, NamedColor::BrightYellow),
            (VteNamedColor::BrightBlue, NamedColor::BrightBlue),
            (VteNamedColor::BrightMagenta, NamedColor::BrightMagenta),
            (VteNamedColor::BrightCyan, NamedColor::BrightCyan),
            (VteNamedColor::BrightWhite, NamedColor::BrightWhite),
        ];

        for (vte_color, expected_named) in test_cases {
            let mut cell = Cell::default();
            cell.fg = VteColor::Named(vte_color);
            let style = cell_to_style(&cell);
            assert_eq!(style.fg, Color::Named(expected_named), "Failed for {:?}", vte_color);
        }
    }

    #[test]
    fn test_cell_to_style_indexed_colors() {
        // Test 256-color indexed colors
        let test_indices = [0u8, 15, 16, 127, 231, 232, 255];

        for idx in test_indices {
            let mut cell = Cell::default();
            cell.fg = VteColor::Indexed(idx);
            let style = cell_to_style(&cell);
            assert_eq!(style.fg, Color::Indexed(idx), "Failed for index {}", idx);
        }
    }

    #[test]
    fn test_cell_to_style_rgb_truecolor() {
        // Test RGB truecolor
        let test_cases = [
            (0, 0, 0),       // Black
            (255, 255, 255), // White
            (255, 0, 0),     // Red
            (0, 255, 0),     // Green
            (0, 0, 255),     // Blue
            (128, 64, 192),  // Arbitrary
        ];

        for (r, g, b) in test_cases {
            let mut cell = Cell::default();
            cell.fg = VteColor::Spec(Rgb { r, g, b });
            let style = cell_to_style(&cell);
            assert_eq!(style.fg, Color::Rgb { r, g, b }, "Failed for RGB({}, {}, {})", r, g, b);
        }
    }

    #[test]
    fn test_cell_to_style_background_colors() {
        // Test that background colors are also correctly captured
        let mut cell = Cell::default();
        cell.bg = VteColor::Named(VteNamedColor::Blue);
        let style = cell_to_style(&cell);
        assert_eq!(style.bg, Color::Named(NamedColor::Blue));

        let mut cell = Cell::default();
        cell.bg = VteColor::Indexed(200);
        let style = cell_to_style(&cell);
        assert_eq!(style.bg, Color::Indexed(200));

        let mut cell = Cell::default();
        cell.bg = VteColor::Spec(Rgb { r: 50, g: 100, b: 150 });
        let style = cell_to_style(&cell);
        assert_eq!(style.bg, Color::Rgb { r: 50, g: 100, b: 150 });
    }

    #[test]
    fn test_cell_to_style_default_colors() {
        // Default foreground/background colors should map to Color::Default
        let mut cell = Cell::default();
        cell.fg = VteColor::Named(VteNamedColor::Foreground);
        cell.bg = VteColor::Named(VteNamedColor::Background);
        let style = cell_to_style(&cell);
        assert_eq!(style.fg, Color::Default);
        assert_eq!(style.bg, Color::Default);
    }

    #[test]
    fn test_cell_to_style_bold_attribute() {
        let mut cell = Cell::default();
        cell.flags = Flags::BOLD;
        let style = cell_to_style(&cell);
        assert!(style.bold);
        assert!(!style.italic);
        assert!(!style.dim);
    }

    #[test]
    fn test_cell_to_style_italic_attribute() {
        let mut cell = Cell::default();
        cell.flags = Flags::ITALIC;
        let style = cell_to_style(&cell);
        assert!(style.italic);
        assert!(!style.bold);
    }

    #[test]
    fn test_cell_to_style_dim_attribute() {
        // Dim should be set when DIM flag is set
        let mut cell = Cell::default();
        cell.flags = Flags::DIM;
        let style = cell_to_style(&cell);
        assert!(style.dim);
        assert!(!style.bold);

        // Dim and bold can coexist
        let mut cell = Cell::default();
        cell.flags = Flags::DIM | Flags::BOLD;
        let style = cell_to_style(&cell);
        assert!(style.bold);
        assert!(style.dim);
    }

    #[test]
    fn test_cell_to_style_inverse_attribute() {
        let mut cell = Cell::default();
        cell.flags = Flags::INVERSE;
        let style = cell_to_style(&cell);
        assert!(style.inverse);
    }

    #[test]
    fn test_cell_to_style_underline_attribute() {
        let mut cell = Cell::default();
        cell.flags = Flags::UNDERLINE;
        let style = cell_to_style(&cell);
        assert_eq!(style.underline, UnderlineStyle::Single);
    }

    #[test]
    fn test_cell_to_style_strikethrough_attribute() {
        let mut cell = Cell::default();
        cell.flags = Flags::STRIKEOUT;
        let style = cell_to_style(&cell);
        assert!(style.strikethrough);
    }

    #[test]
    fn test_cell_to_style_hidden_attribute() {
        let mut cell = Cell::default();
        cell.flags = Flags::HIDDEN;
        let style = cell_to_style(&cell);
        assert!(style.hidden);
    }

    #[test]
    fn test_cell_to_style_combined_attributes() {
        // Test multiple attributes combined
        let mut cell = Cell::default();
        cell.fg = VteColor::Named(VteNamedColor::Red);
        cell.bg = VteColor::Named(VteNamedColor::Blue);
        cell.flags = Flags::BOLD | Flags::ITALIC | Flags::UNDERLINE;
        let style = cell_to_style(&cell);

        assert_eq!(style.fg, Color::Named(NamedColor::Red));
        assert_eq!(style.bg, Color::Named(NamedColor::Blue));
        assert!(style.bold);
        assert!(style.italic);
        assert_eq!(style.underline, UnderlineStyle::Single);
        assert!(!style.dim);
        assert!(!style.strikethrough);
    }

    #[test]
    fn test_row_to_styled_line_preserves_styles_across_spans() {
        // Create a row with multiple styled segments
        // "Red" in red, "Green" in green
        let mut cells: Vec<Cell> = Vec::new();

        // "Red" in red
        for c in "Red".chars() {
            let mut cell = Cell::default();
            cell.c = c;
            cell.fg = VteColor::Named(VteNamedColor::Red);
            cells.push(cell);
        }

        // "Green" in green
        for c in "Green".chars() {
            let mut cell = Cell::default();
            cell.c = c;
            cell.fg = VteColor::Named(VteNamedColor::Green);
            cells.push(cell);
        }

        let line = row_to_styled_line(cells.iter(), 80);

        // Should have two spans with different styles
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].text, "Red");
        assert_eq!(line.spans[0].style.fg, Color::Named(NamedColor::Red));
        assert_eq!(line.spans[1].text, "Green");
        assert_eq!(line.spans[1].style.fg, Color::Named(NamedColor::Green));
    }

    #[test]
    fn test_row_to_styled_line_coalesces_same_style() {
        // Create a row where all cells have the same style
        let mut cells: Vec<Cell> = Vec::new();
        for c in "Hello World".chars() {
            let mut cell = Cell::default();
            cell.c = c;
            cell.fg = VteColor::Named(VteNamedColor::Cyan);
            cells.push(cell);
        }

        let line = row_to_styled_line(cells.iter(), 80);

        // Should have a single span
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].text, "Hello World");
        assert_eq!(line.spans[0].style.fg, Color::Named(NamedColor::Cyan));
    }

    #[test]
    fn test_row_to_styled_line_multiple_style_changes() {
        // Create: "A" red, "B" green, "C" blue, "D" red again
        let colors = [
            ('A', VteNamedColor::Red),
            ('B', VteNamedColor::Green),
            ('C', VteNamedColor::Blue),
            ('D', VteNamedColor::Red),
        ];

        let cells: Vec<Cell> = colors
            .iter()
            .map(|(c, color)| {
                let mut cell = Cell::default();
                cell.c = *c;
                cell.fg = VteColor::Named(*color);
                cell
            })
            .collect();

        let line = row_to_styled_line(cells.iter(), 80);

        // Should have 4 spans (each character is different from the previous)
        assert_eq!(line.spans.len(), 4);
        assert_eq!(line.spans[0].text, "A");
        assert_eq!(line.spans[0].style.fg, Color::Named(NamedColor::Red));
        assert_eq!(line.spans[1].text, "B");
        assert_eq!(line.spans[1].style.fg, Color::Named(NamedColor::Green));
        assert_eq!(line.spans[2].text, "C");
        assert_eq!(line.spans[2].style.fg, Color::Named(NamedColor::Blue));
        assert_eq!(line.spans[3].text, "D");
        assert_eq!(line.spans[3].style.fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn test_row_to_styled_line_preserves_attributes_in_spans() {
        // Create: "Bold" bold, "Italic" italic, "Normal" default
        let mut cells: Vec<Cell> = Vec::new();

        for c in "Bold".chars() {
            let mut cell = Cell::default();
            cell.c = c;
            cell.flags = Flags::BOLD;
            cells.push(cell);
        }

        for c in "Italic".chars() {
            let mut cell = Cell::default();
            cell.c = c;
            cell.flags = Flags::ITALIC;
            cells.push(cell);
        }

        for c in "Normal".chars() {
            let mut cell = Cell::default();
            cell.c = c;
            cells.push(cell);
        }

        let line = row_to_styled_line(cells.iter(), 80);

        assert_eq!(line.spans.len(), 3);
        assert!(line.spans[0].style.bold);
        assert!(!line.spans[0].style.italic);
        assert!(!line.spans[1].style.bold);
        assert!(line.spans[1].style.italic);
        assert!(!line.spans[2].style.bold);
        assert!(!line.spans[2].style.italic);
    }
}
