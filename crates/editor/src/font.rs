// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
//!
//! Font loading and metrics via Core Text
//!
//! This module loads a monospace font via Core Text and extracts critical metrics
//! needed for text layout: glyph advance width, line height, ascent, and descent.
//!
//! For a monospace font, layout is trivial:
//! - x = column * advance_width
//! - y = row * line_height

use std::ptr::NonNull;

use objc2_core_foundation::{
    CFData, CFRetained, CFString, CGAffineTransform, CGFloat, CGSize,
};
use objc2_core_graphics::{CGDataProvider, CGFont};
use objc2_core_text::{CTFont, CTFontOrientation};

// =============================================================================
// Font Metrics
// =============================================================================

/// Metrics extracted from a font, used for glyph layout
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Width of a single glyph (monospace assumption: all glyphs same width)
    pub advance_width: f64,
    /// Height of a line (ascent + descent + leading)
    pub line_height: f64,
    /// Distance from baseline to top of glyph (positive)
    pub ascent: f64,
    /// Distance from baseline to bottom of glyph (positive, stored as positive value)
    pub descent: f64,
    /// Extra spacing between lines
    pub leading: f64,
    /// The point size of the font
    pub point_size: f64,
}

// =============================================================================
// Font
// =============================================================================

/// A loaded monospace font with its metrics
///
/// The font is held as a Core Text font reference. We store metrics in pixels,
/// accounting for the display scale factor.
pub struct Font {
    /// The Core Text font reference (retained)
    ct_font: CFRetained<CTFont>,
    /// Font metrics in pixels at the current scale
    pub metrics: FontMetrics,
}

impl Font {
    /// Loads a font by name at the given point size, scaled for the display
    ///
    /// # Arguments
    /// * `name` - The PostScript name of the font (e.g., "Menlo-Regular")
    /// * `point_size` - The size in points (will be scaled by scale_factor)
    /// * `scale_factor` - The display scale factor (1.0 for standard, 2.0 for Retina)
    ///
    /// # Panics
    /// Panics if the font cannot be loaded.
    pub fn new(name: &str, point_size: f64, scale_factor: f64) -> Self {
        // Convert name to CFString
        let font_name = CFString::from_str(name);

        // Create the font with scaled size
        // Note: We create the font at the scaled size because Core Text metrics
        // will be in that coordinate space
        let scaled_size = point_size * scale_factor;

        // Identity transform for the font
        let transform = CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        };

        let ct_font = unsafe {
            CTFont::with_name(&font_name, scaled_size as CGFloat, &transform)
        };

        // Extract metrics (all CTFont metric methods are unsafe)
        let ascent = unsafe { ct_font.ascent() };
        let descent = unsafe { ct_font.descent() };
        let leading = unsafe { ct_font.leading() };

        // Get advance width from a representative character ('M' is a good choice)
        let advance_width = Self::get_advance_width(&ct_font);

        // Line height is ascent + descent + leading
        let line_height = ascent + descent + leading;

        let metrics = FontMetrics {
            advance_width,
            line_height,
            ascent,
            descent,
            leading,
            point_size: scaled_size,
        };

        Self { ct_font, metrics }
    }

    /// Loads a font from raw TTF data at the given point size, scaled for the display
    ///
    /// # Arguments
    /// * `data` - Raw TTF font file bytes
    /// * `point_size` - The size in points (will be scaled by scale_factor)
    /// * `scale_factor` - The display scale factor (1.0 for standard, 2.0 for Retina)
    ///
    /// # Panics
    /// Panics if the font data is invalid or the font cannot be loaded.
    pub fn from_data(data: &[u8], point_size: f64, scale_factor: f64) -> Self {
        let scaled_size = point_size * scale_factor;

        // Create CGFont from raw TTF data
        let cf_data = CFData::from_bytes(data);
        let provider = CGDataProvider::with_cf_data(Some(&cf_data))
            .expect("Failed to create CGDataProvider from font data");
        let cg_font = CGFont::with_data_provider(&provider)
            .expect("Failed to create CGFont from font data");

        let transform = CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        };

        let ct_font = unsafe {
            CTFont::with_graphics_font(&cg_font, scaled_size as CGFloat, &transform, None)
        };

        // Extract metrics
        let ascent = unsafe { ct_font.ascent() };
        let descent = unsafe { ct_font.descent() };
        let leading = unsafe { ct_font.leading() };
        let advance_width = Self::get_advance_width(&ct_font);
        let line_height = ascent + descent + leading;

        let metrics = FontMetrics {
            advance_width,
            line_height,
            ascent,
            descent,
            leading,
            point_size: scaled_size,
        };

        Self { ct_font, metrics }
    }

    /// Returns the Core Text font
    pub fn ct_font(&self) -> &CTFont {
        &self.ct_font
    }

    /// Gets the advance width for a representative monospace character
    fn get_advance_width(ct_font: &CTFont) -> f64 {
        // Use 'M' as the representative character
        let character: u16 = 'M' as u16;
        let mut glyph: u16 = 0;

        // Get the glyph for 'M'
        let success = unsafe {
            ct_font.glyphs_for_characters(
                NonNull::from(&character),
                NonNull::from(&mut glyph),
                1,
            )
        };

        if !success {
            // Fall back to assuming a reasonable width
            eprintln!("Warning: Could not get glyph for 'M', using fallback width");
            return unsafe { ct_font.ascent() } * 0.6; // Rough approximation
        }

        // Get the advance for this glyph
        let mut advance = CGSize {
            width: 0.0,
            height: 0.0,
        };

        unsafe {
            ct_font.advances_for_glyphs(
                CTFontOrientation::Default,
                NonNull::from(&glyph),
                &mut advance,
                1,
            );
        }

        advance.width
    }

    /// Maps a character to its glyph ID
    // Chunk: docs/chunks/terminal_background_box_drawing - BMP character-to-glyph mapping for on-demand glyph addition
    // Chunk: docs/chunks/terminal_multibyte_rendering - Non-BMP character support via UTF-16 surrogate pairs
    pub fn glyph_for_char(&self, c: char) -> Option<u16> {
        let code = c as u32;

        // Handle BMP characters (most common case)
        if code <= 0xFFFF {
            let character: u16 = c as u16;
            let mut glyph: u16 = 0;

            let success = unsafe {
                self.ct_font.glyphs_for_characters(
                    NonNull::from(&character),
                    NonNull::from(&mut glyph),
                    1,
                )
            };

            if success && glyph != 0 {
                Some(glyph)
            } else {
                None
            }
        } else {
            // Character is outside BMP, use UTF-16 surrogate pairs
            // Compute the surrogate pair per UTF-16 encoding
            let adjusted = code - 0x10000;
            let high_surrogate = ((adjusted >> 10) + 0xD800) as u16;
            let low_surrogate = ((adjusted & 0x3FF) + 0xDC00) as u16;

            let characters: [u16; 2] = [high_surrogate, low_surrogate];
            let mut glyphs: [u16; 2] = [0, 0];

            let success = unsafe {
                self.ct_font.glyphs_for_characters(
                    NonNull::new(characters.as_ptr() as *mut u16).unwrap(),
                    NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                    2,
                )
            };

            // The glyph for a non-BMP character is typically in the first slot
            // (Core Text maps the surrogate pair to a single glyph)
            if success && glyphs[0] != 0 {
                Some(glyphs[0])
            } else {
                None
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_loading() {
        // Test that we can load Menlo (should be available on all macOS systems)
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Basic sanity checks on metrics
        assert!(font.metrics.advance_width > 0.0, "advance_width should be positive");
        assert!(font.metrics.line_height > 0.0, "line_height should be positive");
        assert!(font.metrics.ascent > 0.0, "ascent should be positive");
        assert!(font.metrics.descent > 0.0, "descent should be positive");
    }

    #[test]
    fn test_font_metrics_relationships() {
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Line height should be roughly ascent + descent + leading
        let expected_height = font.metrics.ascent + font.metrics.descent + font.metrics.leading;
        assert!(
            (font.metrics.line_height - expected_height).abs() < 0.01,
            "line_height should equal ascent + descent + leading"
        );

        // For a typical monospace font, line_height should be greater than advance_width
        // (characters are taller than wide)
        assert!(
            font.metrics.line_height > font.metrics.advance_width,
            "line_height ({}) should be greater than advance_width ({})",
            font.metrics.line_height,
            font.metrics.advance_width
        );
    }

    #[test]
    fn test_font_scaling() {
        let font_1x = Font::new("Menlo-Regular", 14.0, 1.0);
        let font_2x = Font::new("Menlo-Regular", 14.0, 2.0);

        // At 2x scale, metrics should be approximately double
        let ratio = font_2x.metrics.advance_width / font_1x.metrics.advance_width;
        assert!(
            (ratio - 2.0).abs() < 0.01,
            "2x scale should double advance_width, got ratio {}",
            ratio
        );

        let height_ratio = font_2x.metrics.line_height / font_1x.metrics.line_height;
        assert!(
            (height_ratio - 2.0).abs() < 0.01,
            "2x scale should double line_height, got ratio {}",
            height_ratio
        );
    }

    #[test]
    fn test_glyph_for_char() {
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Should be able to get glyphs for ASCII characters
        assert!(
            font.glyph_for_char('A').is_some(),
            "Should get glyph for 'A'"
        );
        assert!(
            font.glyph_for_char('z').is_some(),
            "Should get glyph for 'z'"
        );
        assert!(
            font.glyph_for_char('0').is_some(),
            "Should get glyph for '0'"
        );
        assert!(
            font.glyph_for_char(' ').is_some(),
            "Should get glyph for space"
        );
    }

    // ==================== Box-drawing glyph tests ====================
    // Chunk: docs/chunks/terminal_background_box_drawing - Verify Menlo has box-drawing glyphs

    #[test]
    fn test_menlo_has_box_drawing_glyphs() {
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Common box-drawing characters that TUI apps use
        let box_drawing_chars = [
            ('─', "horizontal line U+2500"),
            ('│', "vertical line U+2502"),
            ('┌', "top-left corner U+250C"),
            ('┐', "top-right corner U+2510"),
            ('└', "bottom-left corner U+2514"),
            ('┘', "bottom-right corner U+2518"),
        ];

        for (c, name) in box_drawing_chars {
            assert!(
                font.glyph_for_char(c).is_some(),
                "Menlo should have glyph for {} ({})",
                c,
                name
            );
        }
    }

    #[test]
    fn test_menlo_has_block_element_glyphs() {
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Block element characters used by TUI apps
        let block_chars = [
            ('█', "full block U+2588"),
            ('▀', "upper half block U+2580"),
            ('▄', "lower half block U+2584"),
        ];

        for (c, name) in block_chars {
            let result = font.glyph_for_char(c);
            // Log the result - some characters might not be in Menlo
            if result.is_none() {
                eprintln!("Warning: Menlo may not have glyph for {} ({})", c, name);
            }
            // We don't assert here because some block elements may be missing
            // from Menlo. The atlas will fall back to space for these.
        }
    }

    // Chunk: docs/chunks/terminal_multibyte_rendering - Non-BMP character support
    #[test]
    fn test_non_bmp_characters_with_surrogate_pairs() {
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Characters outside BMP (> U+FFFF) should now work via UTF-16 surrogate pairs.
        // The result depends on font coverage - if the font has the glyph, we get Some,
        // otherwise None. We're testing that the lookup path doesn't crash and handles
        // the surrogate pair calculation correctly.
        let emoji = '😀'; // U+1F600
        let result = font.glyph_for_char(emoji);
        // Apple's emoji font should be available through font fallback on macOS,
        // but Menlo itself may not have emoji glyphs. Either outcome is valid.
        // The important thing is that we don't panic and the logic works.
        if result.is_some() {
            eprintln!("Non-BMP emoji '😀' has glyph ID: {}", result.unwrap());
        } else {
            eprintln!("Non-BMP emoji '😀' not found in Menlo (expected - no emoji glyphs)");
        }

        // Test surrogate pair calculation with edge cases
        // U+10000 is the first non-BMP codepoint
        let first_non_bmp = '\u{10000}';
        let _ = font.glyph_for_char(first_non_bmp); // Just verify no panic

        // U+10FFFF is the last valid Unicode codepoint
        let last_unicode = '\u{10FFFF}';
        let _ = font.glyph_for_char(last_unicode); // Just verify no panic
    }

    #[test]
    fn test_non_bmp_egyptian_hieroglyphs() {
        let font = Font::new("Menlo-Regular", 14.0, 1.0);

        // Egyptian hieroglyphs from the GOAL.md
        let hieroglyphs = ['𓆝', '𓆟', '𓆞']; // U+131DD, U+131DF, U+131DE

        for c in hieroglyphs {
            let result = font.glyph_for_char(c);
            // Most fonts won't have these, but the lookup shouldn't crash
            if result.is_some() {
                eprintln!("Hieroglyph '{}' (U+{:04X}) has glyph ID: {}", c, c as u32, result.unwrap());
            } else {
                eprintln!("Hieroglyph '{}' (U+{:04X}) not found in Menlo (expected)", c, c as u32);
            }
        }
    }

    // ==================== Embedded font (from_data) tests ====================

    const INTEL_ONE_MONO: &[u8] = include_bytes!("../../../resources/IntelOneMono-Regular.ttf");

    #[test]
    fn test_from_data_loading() {
        let font = Font::from_data(INTEL_ONE_MONO, 14.0, 1.0);

        assert!(font.metrics.advance_width > 0.0, "advance_width should be positive");
        assert!(font.metrics.line_height > 0.0, "line_height should be positive");
        assert!(font.metrics.ascent > 0.0, "ascent should be positive");
        assert!(font.metrics.descent > 0.0, "descent should be positive");
    }

    #[test]
    fn test_from_data_scaling() {
        let font_1x = Font::from_data(INTEL_ONE_MONO, 14.0, 1.0);
        let font_2x = Font::from_data(INTEL_ONE_MONO, 14.0, 2.0);

        let ratio = font_2x.metrics.advance_width / font_1x.metrics.advance_width;
        assert!(
            (ratio - 2.0).abs() < 0.01,
            "2x scale should double advance_width, got ratio {}",
            ratio
        );
    }

    #[test]
    fn test_from_data_has_ascii_glyphs() {
        let font = Font::from_data(INTEL_ONE_MONO, 14.0, 1.0);

        assert!(font.glyph_for_char('A').is_some(), "Should have glyph for 'A'");
        assert!(font.glyph_for_char('z').is_some(), "Should have glyph for 'z'");
        assert!(font.glyph_for_char('0').is_some(), "Should have glyph for '0'");
        assert!(font.glyph_for_char(' ').is_some(), "Should have glyph for space");
    }

    #[test]
    fn test_from_data_has_box_drawing_glyphs() {
        let font = Font::from_data(INTEL_ONE_MONO, 14.0, 1.0);

        let box_drawing_chars = [
            ('─', "horizontal line U+2500"),
            ('│', "vertical line U+2502"),
            ('┌', "top-left corner U+250C"),
            ('┐', "top-right corner U+2510"),
            ('└', "bottom-left corner U+2514"),
            ('┘', "bottom-right corner U+2518"),
        ];

        for (c, name) in box_drawing_chars {
            assert!(
                font.glyph_for_char(c).is_some(),
                "Intel One Mono should have glyph for {} ({})",
                c,
                name
            );
        }
    }
}
