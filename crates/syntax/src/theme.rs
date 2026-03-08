// Chunk: docs/chunks/syntax_highlighting - Catppuccin Mocha theme for syntax highlighting

//! Syntax theme mapping capture names to styles.
//!
//! This module provides a `SyntaxTheme` that maps tree-sitter capture names
//! (e.g., "keyword", "string", "comment") to visual styles using the
//! Catppuccin Mocha color palette.

use lite_edit_buffer::Style;
use std::collections::HashMap;

/// Catppuccin Mocha color palette constants.
///
/// These are 24-bit RGB colors from the Catppuccin Mocha theme.
mod catppuccin {
    use lite_edit_buffer::Color;

    // Accent colors
    pub const MAUVE: Color = Color::Rgb {
        r: 0xcb,
        g: 0xa6,
        b: 0xf7,
    }; // #cba6f7
    pub const BLUE: Color = Color::Rgb {
        r: 0x89,
        g: 0xb4,
        b: 0xfa,
    }; // #89b4fa
    pub const SAPPHIRE: Color = Color::Rgb {
        r: 0x74,
        g: 0xc7,
        b: 0xec,
    }; // #74c7ec
    pub const GREEN: Color = Color::Rgb {
        r: 0xa6,
        g: 0xe3,
        b: 0xa1,
    }; // #a6e3a1
    pub const PINK: Color = Color::Rgb {
        r: 0xf5,
        g: 0xc2,
        b: 0xe7,
    }; // #f5c2e7
    pub const PEACH: Color = Color::Rgb {
        r: 0xfa,
        g: 0xb3,
        b: 0x87,
    }; // #fab387
    pub const YELLOW: Color = Color::Rgb {
        r: 0xf9,
        g: 0xe2,
        b: 0xaf,
    }; // #f9e2af
    pub const MAROON: Color = Color::Rgb {
        r: 0xeb,
        g: 0xa0,
        b: 0xac,
    }; // #eba0ac
    pub const RED: Color = Color::Rgb {
        r: 0xf3,
        g: 0x8b,
        b: 0xa8,
    }; // #f38ba8
    pub const LAVENDER: Color = Color::Rgb {
        r: 0xb4,
        g: 0xbe,
        b: 0xfe,
    }; // #b4befe
    pub const SKY: Color = Color::Rgb {
        r: 0x89,
        g: 0xdc,
        b: 0xeb,
    }; // #89dceb

    // Surface/text colors
    pub const OVERLAY0: Color = Color::Rgb {
        r: 0x6c,
        g: 0x70,
        b: 0x86,
    }; // #6c7086
    pub const SUBTEXT0: Color = Color::Rgb {
        r: 0xa6,
        g: 0xad,
        b: 0xc8,
    }; // #a6adc8
}

/// A mapping from tree-sitter capture names to visual styles.
///
/// The theme holds a map from capture name prefixes to `Style` values.
/// When looking up a capture like "function.method", it first tries the
/// exact match, then falls back to prefix matches ("function").
pub struct SyntaxTheme {
    /// Map from capture name to style
    styles: HashMap<&'static str, Style>,
    /// Ordered list of capture names (for tree-sitter-highlight)
    capture_names: Vec<&'static str>,
}

impl SyntaxTheme {
    /// Creates the Catppuccin Mocha syntax theme.
    ///
    /// This theme maps common tree-sitter capture names to colors from
    /// the Catppuccin Mocha palette.
    pub fn catppuccin_mocha() -> Self {
        let mut styles = HashMap::new();

        // Keywords - Mauve
        styles.insert(
            "keyword",
            Style {
                fg: catppuccin::MAUVE,
                ..Style::default()
            },
        );

        // Functions - Blue
        styles.insert(
            "function",
            Style {
                fg: catppuccin::BLUE,
                ..Style::default()
            },
        );
        styles.insert(
            "function.method",
            Style {
                fg: catppuccin::BLUE,
                ..Style::default()
            },
        );
        styles.insert(
            "function.macro",
            Style {
                fg: catppuccin::MAUVE,
                ..Style::default()
            },
        );

        // Types - Yellow
        styles.insert(
            "type",
            Style {
                fg: catppuccin::YELLOW,
                ..Style::default()
            },
        );
        styles.insert(
            "type.builtin",
            Style {
                fg: catppuccin::YELLOW,
                italic: true,
                ..Style::default()
            },
        );

        // Constructor - Sapphire
        styles.insert(
            "constructor",
            Style {
                fg: catppuccin::SAPPHIRE,
                ..Style::default()
            },
        );

        // Strings - Green
        styles.insert(
            "string",
            Style {
                fg: catppuccin::GREEN,
                ..Style::default()
            },
        );

        // Escape sequences - Pink
        styles.insert(
            "escape",
            Style {
                fg: catppuccin::PINK,
                ..Style::default()
            },
        );

        // Constants - Peach
        styles.insert(
            "constant",
            Style {
                fg: catppuccin::PEACH,
                ..Style::default()
            },
        );
        styles.insert(
            "constant.builtin",
            Style {
                fg: catppuccin::PEACH,
                ..Style::default()
            },
        );
        styles.insert(
            "number",
            Style {
                fg: catppuccin::PEACH,
                ..Style::default()
            },
        );

        // Comments - Overlay0 with italic
        styles.insert(
            "comment",
            Style {
                fg: catppuccin::OVERLAY0,
                italic: true,
                ..Style::default()
            },
        );
        styles.insert(
            "comment.documentation",
            Style {
                fg: catppuccin::OVERLAY0,
                italic: true,
                ..Style::default()
            },
        );

        // Variables
        styles.insert(
            "variable.parameter",
            Style {
                fg: catppuccin::MAROON,
                italic: true,
                ..Style::default()
            },
        );
        styles.insert(
            "variable.builtin",
            Style {
                fg: catppuccin::RED,
                ..Style::default()
            },
        );

        // Properties - Lavender
        styles.insert(
            "property",
            Style {
                fg: catppuccin::LAVENDER,
                ..Style::default()
            },
        );

        // Labels - Sapphire with italic
        styles.insert(
            "label",
            Style {
                fg: catppuccin::SAPPHIRE,
                italic: true,
                ..Style::default()
            },
        );

        // Punctuation - Subtext0
        styles.insert(
            "punctuation.bracket",
            Style {
                fg: catppuccin::SUBTEXT0,
                ..Style::default()
            },
        );
        styles.insert(
            "punctuation.delimiter",
            Style {
                fg: catppuccin::SUBTEXT0,
                ..Style::default()
            },
        );

        // Operators - Sky
        styles.insert(
            "operator",
            Style {
                fg: catppuccin::SKY,
                ..Style::default()
            },
        );

        // Attributes - Yellow
        styles.insert(
            "attribute",
            Style {
                fg: catppuccin::YELLOW,
                ..Style::default()
            },
        );

        // Markdown-specific captures
        // Headings - Mauve (bold)
        styles.insert(
            "text.title",
            Style {
                fg: catppuccin::MAUVE,
                bold: true,
                ..Style::default()
            },
        );
        // Inline code - Green
        styles.insert(
            "text.literal",
            Style {
                fg: catppuccin::GREEN,
                ..Style::default()
            },
        );
        // URIs / links - Blue with underline
        styles.insert(
            "text.uri",
            Style {
                fg: catppuccin::BLUE,
                underline: lite_edit_buffer::UnderlineStyle::Single,
                ..Style::default()
            },
        );
        // Link references - Lavender
        styles.insert(
            "text.reference",
            Style {
                fg: catppuccin::LAVENDER,
                ..Style::default()
            },
        );
        // Markdown punctuation (# for headings, ``` for code fences, etc.) - Subtext0
        styles.insert(
            "punctuation.special",
            Style {
                fg: catppuccin::SUBTEXT0,
                ..Style::default()
            },
        );

        // Build the ordered capture names list
        // This order matters for tree-sitter-highlight - more specific names first
        let capture_names = vec![
            "attribute",
            "comment.documentation",
            "comment",
            "constant.builtin",
            "constant",
            "constructor",
            "escape",
            "function.macro",
            "function.method",
            "function",
            "keyword",
            "label",
            "number",
            "operator",
            "property",
            "punctuation.bracket",
            "punctuation.delimiter",
            "punctuation.special",
            "string",
            "text.literal",
            "text.reference",
            "text.title",
            "text.uri",
            "type.builtin",
            "type",
            "variable.builtin",
            "variable.parameter",
        ];

        Self {
            styles,
            capture_names,
        }
    }

    /// Returns the style for a capture name, if defined.
    ///
    /// First tries an exact match, then tries prefix matching
    /// (e.g., "function.method.call" would match "function.method" then "function").
    pub fn style_for_capture(&self, name: &str) -> Option<&Style> {
        // Try exact match first
        if let Some(style) = self.styles.get(name) {
            return Some(style);
        }

        // Try progressively shorter prefixes
        let mut prefix = name;
        while let Some(dot_pos) = prefix.rfind('.') {
            prefix = &prefix[..dot_pos];
            if let Some(style) = self.styles.get(prefix) {
                return Some(style);
            }
        }

        None
    }

    /// Returns the ordered list of capture names for tree-sitter-highlight.
    ///
    /// This list determines the highlight ID -> capture name mapping.
    pub fn capture_names(&self) -> &[&'static str] {
        &self.capture_names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lite_edit_buffer::Color;

    #[test]
    fn test_all_capture_names_have_styles() {
        let theme = SyntaxTheme::catppuccin_mocha();
        for name in theme.capture_names() {
            assert!(
                theme.style_for_capture(name).is_some(),
                "Capture '{}' should have a style",
                name
            );
        }
    }

    #[test]
    fn test_styles_use_rgb_colors() {
        let theme = SyntaxTheme::catppuccin_mocha();
        for name in theme.capture_names() {
            let style = theme.style_for_capture(name).unwrap();
            assert!(
                matches!(style.fg, Color::Rgb { .. }),
                "Capture '{}' should have RGB fg color",
                name
            );
        }
    }

    #[test]
    fn test_italic_captures() {
        let theme = SyntaxTheme::catppuccin_mocha();

        // These should be italic
        let italic_names = [
            "comment",
            "comment.documentation",
            "type.builtin",
            "variable.parameter",
            "label",
        ];

        for name in italic_names {
            let style = theme.style_for_capture(name).unwrap();
            assert!(
                style.italic,
                "Capture '{}' should be italic",
                name
            );
        }
    }

    #[test]
    fn test_non_italic_captures() {
        let theme = SyntaxTheme::catppuccin_mocha();

        // These should NOT be italic
        let non_italic_names = ["keyword", "function", "string", "number", "operator"];

        for name in non_italic_names {
            let style = theme.style_for_capture(name).unwrap();
            assert!(
                !style.italic,
                "Capture '{}' should not be italic",
                name
            );
        }
    }

    #[test]
    fn test_prefix_matching() {
        let theme = SyntaxTheme::catppuccin_mocha();

        // "function.method.call" should match "function.method"
        let style = theme.style_for_capture("function.method.call");
        assert!(style.is_some());

        // "comment.line" should match "comment"
        let style = theme.style_for_capture("comment.line");
        assert!(style.is_some());
    }

    #[test]
    fn test_unknown_capture() {
        let theme = SyntaxTheme::catppuccin_mocha();
        assert!(theme.style_for_capture("unknown.capture").is_none());
    }

    #[test]
    fn test_keyword_is_mauve() {
        let theme = SyntaxTheme::catppuccin_mocha();
        let style = theme.style_for_capture("keyword").unwrap();
        assert_eq!(
            style.fg,
            Color::Rgb {
                r: 0xcb,
                g: 0xa6,
                b: 0xf7
            }
        );
    }

    #[test]
    fn test_string_is_green() {
        let theme = SyntaxTheme::catppuccin_mocha();
        let style = theme.style_for_capture("string").unwrap();
        assert_eq!(
            style.fg,
            Color::Rgb {
                r: 0xa6,
                g: 0xe3,
                b: 0xa1
            }
        );
    }

    #[test]
    fn test_comment_is_overlay0() {
        let theme = SyntaxTheme::catppuccin_mocha();
        let style = theme.style_for_capture("comment").unwrap();
        assert_eq!(
            style.fg,
            Color::Rgb {
                r: 0x6c,
                g: 0x70,
                b: 0x86
            }
        );
        assert!(style.italic);
    }
}
