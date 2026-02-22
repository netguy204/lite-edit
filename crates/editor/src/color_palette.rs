// Chunk: docs/chunks/renderer_styled_content - Color palette for resolving Style colors to RGBA
//!
//! Color palette for resolving terminal-grade colors to RGBA.
//!
//! This module provides the `ColorPalette` struct which resolves the `Color` enum
//! from buffer_view to concrete RGBA values for rendering. It supports:
//!
//! - Default foreground/background colors (Catppuccin Mocha theme)
//! - Named ANSI colors (16 colors)
//! - Indexed colors (256-color xterm palette)
//! - True color RGB
//!
//! The palette also handles style transformations like inverse video and dim.

use lite_edit_buffer::{Color, NamedColor, Style};

// =============================================================================
// Catppuccin Mocha Theme Colors
// =============================================================================

/// Default foreground color: #cdd6f4 (Catppuccin Mocha "text")
const DEFAULT_FG: [f32; 4] = [
    0.804, // 0xcd / 255
    0.839, // 0xd6 / 255
    0.957, // 0xf4 / 255
    1.0,
];

/// Default background color: #1e1e2e (Catppuccin Mocha "base")
const DEFAULT_BG: [f32; 4] = [
    0.118, // 0x1e / 255
    0.118, // 0x1e / 255
    0.180, // 0x2e / 255
    1.0,
];

/// Catppuccin Mocha 16-color ANSI palette.
/// These are the standard terminal colors themed for Catppuccin Mocha.
const ANSI_COLORS: [[f32; 4]; 16] = [
    // Normal colors (0-7)
    [0.282, 0.290, 0.392, 1.0], // 0: Black (Surface1: #45475a)
    [0.953, 0.545, 0.659, 1.0], // 1: Red (#f38ba8)
    [0.651, 0.890, 0.631, 1.0], // 2: Green (#a6e3a1)
    [0.976, 0.886, 0.686, 1.0], // 3: Yellow (#f9e2af)
    [0.537, 0.706, 0.980, 1.0], // 4: Blue (#89b4fa)
    [0.796, 0.651, 0.969, 1.0], // 5: Magenta (#cba6f7)
    [0.576, 0.910, 0.765, 1.0], // 6: Cyan (#94e2d5)
    [0.729, 0.753, 0.847, 1.0], // 7: White (Subtext1: #bac2de)
    // Bright colors (8-15)
    [0.384, 0.396, 0.510, 1.0], // 8: Bright Black (Surface2: #585b70)
    [0.953, 0.545, 0.659, 1.0], // 9: Bright Red (#f38ba8)
    [0.651, 0.890, 0.631, 1.0], // 10: Bright Green (#a6e3a1)
    [0.976, 0.886, 0.686, 1.0], // 11: Bright Yellow (#f9e2af)
    [0.537, 0.706, 0.980, 1.0], // 12: Bright Blue (#89b4fa)
    [0.796, 0.651, 0.969, 1.0], // 13: Bright Magenta (#cba6f7)
    [0.576, 0.910, 0.765, 1.0], // 14: Bright Cyan (#94e2d5)
    [0.804, 0.839, 0.957, 1.0], // 15: Bright White (Text: #cdd6f4)
];

// =============================================================================
// ColorPalette
// =============================================================================

/// Palette for resolving terminal colors to RGBA values.
///
/// This struct holds the theme colors and provides methods to resolve
/// the `Color` enum to concrete RGBA values.
#[derive(Debug, Clone)]
pub struct ColorPalette {
    /// Default foreground color
    pub default_fg: [f32; 4],
    /// Default background color
    pub default_bg: [f32; 4],
    /// The 16 ANSI colors
    pub ansi_colors: [[f32; 4]; 16],
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

impl ColorPalette {
    /// Creates a new ColorPalette with Catppuccin Mocha theme colors.
    pub fn catppuccin_mocha() -> Self {
        Self {
            default_fg: DEFAULT_FG,
            default_bg: DEFAULT_BG,
            ansi_colors: ANSI_COLORS,
        }
    }

    /// Resolves a `Color` to an RGBA value.
    ///
    /// # Arguments
    /// * `color` - The color to resolve
    /// * `is_foreground` - Whether this is for foreground (true) or background (false)
    ///
    /// # Returns
    /// The resolved RGBA color as `[f32; 4]`
    pub fn resolve_color(&self, color: Color, is_foreground: bool) -> [f32; 4] {
        match color {
            Color::Default => {
                if is_foreground {
                    self.default_fg
                } else {
                    self.default_bg
                }
            }
            Color::Named(named) => self.resolve_named_color(named),
            Color::Indexed(index) => self.resolve_indexed_color(index),
            Color::Rgb { r, g, b } => [
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                1.0,
            ],
        }
    }

    /// Resolves a named ANSI color to RGBA.
    fn resolve_named_color(&self, named: NamedColor) -> [f32; 4] {
        let index = match named {
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
        };
        self.ansi_colors[index]
    }

    /// Resolves a 256-color palette index to RGBA.
    ///
    /// The 256-color palette is structured as:
    /// - 0-15: Standard ANSI colors (from theme)
    /// - 16-231: 6×6×6 color cube
    /// - 232-255: 24-level grayscale
    fn resolve_indexed_color(&self, index: u8) -> [f32; 4] {
        match index {
            // ANSI colors (0-15)
            0..=15 => self.ansi_colors[index as usize],

            // 6×6×6 color cube (16-231)
            16..=231 => {
                let idx = index - 16;
                let r = idx / 36;
                let g = (idx % 36) / 6;
                let b = idx % 6;

                // Each component maps: 0 -> 0, 1 -> 95, 2 -> 135, 3 -> 175, 4 -> 215, 5 -> 255
                fn cube_component(c: u8) -> f32 {
                    if c == 0 {
                        0.0
                    } else {
                        (55.0 + c as f32 * 40.0) / 255.0
                    }
                }

                [
                    cube_component(r),
                    cube_component(g),
                    cube_component(b),
                    1.0,
                ]
            }

            // Grayscale (232-255)
            232..=255 => {
                let idx = index - 232;
                // Maps 0-23 to gray levels 8, 18, 28, ..., 238
                let gray = (8.0 + idx as f32 * 10.0) / 255.0;
                [gray, gray, gray, 1.0]
            }
        }
    }

    // Chunk: docs/chunks/terminal_styling_fidelity - Style to RGBA resolution including inverse and dim transformations
    /// Resolves foreground and background colors from a style, applying
    /// inverse and dim transformations.
    ///
    /// # Arguments
    /// * `style` - The style containing colors and attributes
    ///
    /// # Returns
    /// A tuple of (foreground RGBA, background RGBA)
    pub fn resolve_style_colors(&self, style: &Style) -> ([f32; 4], [f32; 4]) {
        let mut fg = self.resolve_color(style.fg, true);
        let mut bg = self.resolve_color(style.bg, false);

        // Apply inverse video: swap fg and bg
        if style.inverse {
            std::mem::swap(&mut fg, &mut bg);
        }

        // Apply dim: reduce foreground alpha by 50%
        if style.dim {
            fg[3] *= 0.5;
        }

        (fg, bg)
    }

    /// Returns the default foreground color.
    pub fn default_foreground(&self) -> [f32; 4] {
        self.default_fg
    }

    /// Returns the default background color.
    pub fn default_background(&self) -> [f32; 4] {
        self.default_bg
    }

    /// Checks if a color is the default background.
    ///
    /// This is used to skip emitting background quads for spans with
    /// the default background color.
    pub fn is_default_background(&self, color: Color) -> bool {
        matches!(color, Color::Default)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    fn colors_approx_eq(a: &[f32; 4], b: &[f32; 4]) -> bool {
        approx_eq(a[0], b[0]) && approx_eq(a[1], b[1]) && approx_eq(a[2], b[2]) && approx_eq(a[3], b[3])
    }

    #[test]
    fn test_default_colors() {
        let palette = ColorPalette::default();

        let fg = palette.resolve_color(Color::Default, true);
        let bg = palette.resolve_color(Color::Default, false);

        assert!(colors_approx_eq(&fg, &DEFAULT_FG));
        assert!(colors_approx_eq(&bg, &DEFAULT_BG));
    }

    #[test]
    fn test_named_colors() {
        let palette = ColorPalette::default();

        let red = palette.resolve_color(Color::Named(NamedColor::Red), true);
        let green = palette.resolve_color(Color::Named(NamedColor::Green), true);
        let blue = palette.resolve_color(Color::Named(NamedColor::Blue), true);

        // Red should have high R component
        assert!(red[0] > 0.9);
        // Green should have high G component
        assert!(green[1] > 0.8);
        // Blue should have high B component
        assert!(blue[2] > 0.9);
    }

    #[test]
    fn test_rgb_color() {
        let palette = ColorPalette::default();

        let color = palette.resolve_color(Color::Rgb { r: 255, g: 128, b: 0 }, true);

        assert!(approx_eq(color[0], 1.0));
        assert!(approx_eq(color[1], 128.0 / 255.0));
        assert!(approx_eq(color[2], 0.0));
        assert!(approx_eq(color[3], 1.0));
    }

    #[test]
    fn test_indexed_ansi() {
        let palette = ColorPalette::default();

        // Index 1 should be red (same as Named::Red)
        let indexed = palette.resolve_color(Color::Indexed(1), true);
        let named = palette.resolve_color(Color::Named(NamedColor::Red), true);

        assert!(colors_approx_eq(&indexed, &named));
    }

    #[test]
    fn test_indexed_cube() {
        let palette = ColorPalette::default();

        // Index 16 is the first color cube entry: r=0, g=0, b=0 -> black
        let black = palette.resolve_color(Color::Indexed(16), true);
        assert!(approx_eq(black[0], 0.0));
        assert!(approx_eq(black[1], 0.0));
        assert!(approx_eq(black[2], 0.0));

        // Index 231 is the last color cube entry: r=5, g=5, b=5 -> white
        let white = palette.resolve_color(Color::Indexed(231), true);
        assert!(approx_eq(white[0], 1.0));
        assert!(approx_eq(white[1], 1.0));
        assert!(approx_eq(white[2], 1.0));
    }

    #[test]
    fn test_indexed_grayscale() {
        let palette = ColorPalette::default();

        // Index 232 is the darkest gray
        let dark = palette.resolve_color(Color::Indexed(232), true);
        assert!(dark[0] < 0.1);
        assert!(approx_eq(dark[0], dark[1]));
        assert!(approx_eq(dark[1], dark[2]));

        // Index 255 is the lightest gray
        let light = palette.resolve_color(Color::Indexed(255), true);
        assert!(light[0] > 0.9);
        assert!(approx_eq(light[0], light[1]));
        assert!(approx_eq(light[1], light[2]));
    }

    #[test]
    fn test_style_inverse() {
        let palette = ColorPalette::default();

        let style = Style {
            fg: Color::Named(NamedColor::Red),
            bg: Color::Named(NamedColor::Blue),
            inverse: true,
            ..Style::default()
        };

        let (fg, bg) = palette.resolve_style_colors(&style);

        // After inverse, fg should be blue and bg should be red
        let blue = palette.resolve_color(Color::Named(NamedColor::Blue), true);
        let red = palette.resolve_color(Color::Named(NamedColor::Red), true);

        assert!(colors_approx_eq(&fg, &blue));
        assert!(colors_approx_eq(&bg, &red));
    }

    #[test]
    fn test_style_dim() {
        let palette = ColorPalette::default();

        let style = Style {
            fg: Color::Default,
            dim: true,
            ..Style::default()
        };

        let (fg, _bg) = palette.resolve_style_colors(&style);

        // Alpha should be 0.5 (halved from 1.0)
        assert!(approx_eq(fg[3], 0.5));
    }

    #[test]
    fn test_style_inverse_and_dim() {
        let palette = ColorPalette::default();

        let style = Style {
            fg: Color::Named(NamedColor::Green),
            bg: Color::Named(NamedColor::Black),
            inverse: true,
            dim: true,
            ..Style::default()
        };

        let (fg, bg) = palette.resolve_style_colors(&style);

        // After inverse: fg is Black, bg is Green
        let black = palette.resolve_color(Color::Named(NamedColor::Black), true);
        let green = palette.resolve_color(Color::Named(NamedColor::Green), true);

        // fg color should match black, but with halved alpha
        assert!(approx_eq(fg[0], black[0]));
        assert!(approx_eq(fg[1], black[1]));
        assert!(approx_eq(fg[2], black[2]));
        assert!(approx_eq(fg[3], 0.5)); // dim halves alpha

        // bg should be green
        assert!(colors_approx_eq(&bg, &green));
    }

    #[test]
    fn test_is_default_background() {
        let palette = ColorPalette::default();

        assert!(palette.is_default_background(Color::Default));
        assert!(!palette.is_default_background(Color::Named(NamedColor::Black)));
        assert!(!palette.is_default_background(Color::Indexed(0)));
        assert!(!palette.is_default_background(Color::Rgb { r: 30, g: 30, b: 46 }));
    }
}
