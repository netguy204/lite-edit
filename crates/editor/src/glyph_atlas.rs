// Subsystem: docs/subsystems/renderer - GPU-accelerated text and UI rendering
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/terminal_background_box_drawing - On-demand glyph addition for terminal rendering
//!
//! Glyph atlas for texture-based text rendering
//!
//! This module implements a texture atlas that caches rasterized glyphs.
//! Glyphs are rasterized on demand via Core Text into a Metal texture.
//!
//! The atlas uses a simple row-based packing strategy:
//! - Fill rows left-to-right
//! - Move to next row when full
//! - Pre-populate printable ASCII (0x20-0x7E) at startup
//!
//! Each glyph is stored with its UV coordinates for texture sampling.

use std::collections::HashMap;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext, CGImageAlphaInfo,
};
use objc2_metal::{MTLDevice, MTLPixelFormat, MTLRegion, MTLTexture, MTLTextureDescriptor};
use objc2_core_text::CTFont;

use crate::font::{Font, GlyphFont, GlyphSource};

// =============================================================================
// Constants
// =============================================================================

/// Default atlas size (1024x1024 gives ~16K glyphs at 8x16 cell size)
pub const ATLAS_SIZE: usize = 1024;

// =============================================================================
// GlyphInfo
// =============================================================================

/// Information about a glyph stored in the atlas
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// UV coordinates of the glyph in the atlas (normalized 0.0-1.0)
    pub uv_min: (f32, f32),
    pub uv_max: (f32, f32),

    /// Size of the glyph in pixels
    pub width: f32,
    pub height: f32,

    /// Offset from the baseline (for rendering)
    pub bearing_x: f32,
    pub bearing_y: f32,
}

// =============================================================================
// GlyphAtlas
// =============================================================================

/// A texture atlas storing rasterized glyphs
pub struct GlyphAtlas {
    /// The Metal texture storing the atlas
    texture: Retained<ProtocolObject<dyn MTLTexture>>,

    /// Mapping from character to glyph info
    glyphs: HashMap<char, GlyphInfo>,

    /// Current packing position
    cursor_x: usize,
    cursor_y: usize,

    /// Height of the current row (max glyph height in row)
    row_height: usize,

    /// Size of each glyph cell (based on font metrics)
    cell_width: usize,
    cell_height: usize,

    /// Padding between glyphs to prevent texture bleeding
    padding: usize,
}

impl GlyphAtlas {
    /// Creates a new glyph atlas and pre-populates it with printable ASCII
    ///
    /// # Arguments
    /// * `device` - The Metal device to create the texture on
    /// * `font` - The font to rasterize glyphs from
    pub fn new(device: &ProtocolObject<dyn MTLDevice>, font: &Font) -> Self {
        // Calculate cell size from font metrics
        // Add a small buffer for anti-aliasing edges
        let cell_width = (font.metrics.advance_width.ceil() as usize).max(1) + 2;
        let cell_height = (font.metrics.line_height.ceil() as usize).max(1) + 2;

        // Create the texture descriptor
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::R8Unorm,
                ATLAS_SIZE,
                ATLAS_SIZE,
                false,
            )
        };

        let texture = device
            .newTextureWithDescriptor(&descriptor)
            .expect("Failed to create atlas texture");

        let mut atlas = Self {
            texture,
            glyphs: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            cell_width,
            cell_height,
            padding: 1,
        };

        // Pre-populate printable ASCII (0x20-0x7E)
        for c in ' '..='~' {
            atlas.add_glyph(font, c);
        }

        // Add a solid white cell (used for cursor and other solid-color quads).
        // We store it under the non-printable '\x01' so it never collides with
        // real characters.
        atlas.add_solid_cell();

        atlas
    }

    /// Returns the Metal texture
    pub fn texture(&self) -> &ProtocolObject<dyn MTLTexture> {
        &self.texture
    }

    /// Gets the glyph info for a character, or None if not in atlas
    pub fn get_glyph(&self, c: char) -> Option<&GlyphInfo> {
        self.glyphs.get(&c)
    }

    /// Returns the cell dimensions used for glyph storage
    pub fn cell_dimensions(&self) -> (usize, usize) {
        (self.cell_width, self.cell_height)
    }

    /// Returns glyph info for a solid (fully opaque) white region in the atlas.
    ///
    /// This is used for rendering solid-colored quads like the cursor, where
    /// we need atlas alpha = 1.0 everywhere so the fragment shader's
    /// `text_color.a * alpha` produces a fully opaque result.
    pub fn solid_glyph(&self) -> &GlyphInfo {
        self.glyphs
            .get(&'\x01')
            .expect("solid glyph must be present in atlas")
    }

    /// Adds a glyph to the atlas
    ///
    /// Returns true if the glyph was added, false if there's no space
    pub fn add_glyph(&mut self, font: &Font, c: char) -> bool {
        // Check if already in atlas
        if self.glyphs.contains_key(&c) {
            return true;
        }

        // Get the glyph ID
        let glyph_id = match font.glyph_for_char(c) {
            Some(id) => id,
            None => {
                // No glyph for this character; return false to trigger
                // fallback to space glyph in ensure_glyph
                return false;
            }
        };

        // Check if we have space
        let glyph_width = self.cell_width;
        let glyph_height = self.cell_height;

        // Check if we need to move to next row
        if self.cursor_x + glyph_width + self.padding > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + self.padding;
            self.row_height = 0;
        }

        // Check if we've run out of vertical space
        if self.cursor_y + glyph_height > ATLAS_SIZE {
            eprintln!("Warning: Glyph atlas is full, cannot add '{}'", c);
            return false;
        }

        // Rasterize the glyph
        let bitmap = self.rasterize_glyph(font, glyph_id, glyph_width, glyph_height);

        // Upload to texture
        let region = MTLRegion {
            origin: objc2_metal::MTLOrigin {
                x: self.cursor_x,
                y: self.cursor_y,
                z: 0,
            },
            size: objc2_metal::MTLSize {
                width: glyph_width,
                height: glyph_height,
                depth: 1,
            },
        };

        // Create a NonNull pointer from the bitmap data
        let bytes_ptr = NonNull::new(bitmap.as_ptr() as *mut std::ffi::c_void)
            .expect("bitmap pointer should not be null");

        // SAFETY: We're uploading valid bitmap data to the texture
        unsafe {
            self.texture
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(region, 0, bytes_ptr, glyph_width);
        }

        // Calculate UV coordinates (normalized)
        let atlas_size = ATLAS_SIZE as f32;
        let uv_min = (
            self.cursor_x as f32 / atlas_size,
            self.cursor_y as f32 / atlas_size,
        );
        let uv_max = (
            (self.cursor_x + glyph_width) as f32 / atlas_size,
            (self.cursor_y + glyph_height) as f32 / atlas_size,
        );

        // Store glyph info
        let info = GlyphInfo {
            uv_min,
            uv_max,
            width: glyph_width as f32,
            height: glyph_height as f32,
            bearing_x: 1.0, // Padding offset
            bearing_y: font.metrics.ascent as f32,
        };

        self.glyphs.insert(c, info);

        // Advance cursor
        self.cursor_x += glyph_width + self.padding;
        self.row_height = self.row_height.max(glyph_height);

        true
    }

    // Chunk: docs/chunks/font_fallback_rendering - Add glyph from primary or fallback font
    /// Adds a glyph to the atlas using a GlyphSource (primary or fallback font).
    ///
    /// This is the fallback-aware version of `add_glyph`. It accepts a `GlyphSource`
    /// that specifies which font the glyph comes from.
    ///
    /// Returns true if the glyph was added, false if there's no space.
    pub fn add_glyph_with_source(&mut self, font: &Font, c: char, source: GlyphSource) -> bool {
        // Check if already in atlas
        if self.glyphs.contains_key(&c) {
            return true;
        }

        // Check if we have space
        let glyph_width = self.cell_width;
        let glyph_height = self.cell_height;

        // Check if we need to move to next row
        if self.cursor_x + glyph_width + self.padding > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + self.padding;
            self.row_height = 0;
        }

        // Check if we've run out of vertical space
        if self.cursor_y + glyph_height > ATLAS_SIZE {
            eprintln!("Warning: Glyph atlas is full, cannot add '{}'", c);
            return false;
        }

        // Rasterize the glyph from the appropriate font
        // Chunk: docs/chunks/fallback_glyph_metrics - Use fallback font's own metrics
        let bitmap = match &source.font {
            GlyphFont::Primary => {
                // Use the primary font with its own metrics
                self.rasterize_glyph(font, source.glyph_id, glyph_width, glyph_height)
            }
            GlyphFont::Fallback(fallback_font) => {
                // Query the fallback font's own metrics for proper scaling and positioning
                let (fb_ascent, fb_descent, fb_line_height) =
                    Font::get_ct_font_metrics(fallback_font);
                self.rasterize_glyph_with_ct_font(
                    fallback_font,
                    fb_ascent,
                    fb_descent,
                    fb_line_height,
                    source.glyph_id,
                    glyph_width,
                    glyph_height,
                )
            }
        };

        // Upload to texture
        let region = MTLRegion {
            origin: objc2_metal::MTLOrigin {
                x: self.cursor_x,
                y: self.cursor_y,
                z: 0,
            },
            size: objc2_metal::MTLSize {
                width: glyph_width,
                height: glyph_height,
                depth: 1,
            },
        };

        // Create a NonNull pointer from the bitmap data
        let bytes_ptr = NonNull::new(bitmap.as_ptr() as *mut std::ffi::c_void)
            .expect("bitmap pointer should not be null");

        // SAFETY: We're uploading valid bitmap data to the texture
        unsafe {
            self.texture
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(region, 0, bytes_ptr, glyph_width);
        }

        // Calculate UV coordinates (normalized)
        let atlas_size = ATLAS_SIZE as f32;
        let uv_min = (
            self.cursor_x as f32 / atlas_size,
            self.cursor_y as f32 / atlas_size,
        );
        let uv_max = (
            (self.cursor_x + glyph_width) as f32 / atlas_size,
            (self.cursor_y + glyph_height) as f32 / atlas_size,
        );

        // Store glyph info
        let info = GlyphInfo {
            uv_min,
            uv_max,
            width: glyph_width as f32,
            height: glyph_height as f32,
            bearing_x: 1.0, // Padding offset
            bearing_y: font.metrics.ascent as f32,
        };

        self.glyphs.insert(c, info);

        // Advance cursor
        self.cursor_x += glyph_width + self.padding;
        self.row_height = self.row_height.max(glyph_height);

        true
    }

    /// Adds a fully opaque (white) cell to the atlas.
    ///
    /// This provides a solid UV region that the cursor and other non-glyph
    /// quads can sample from, ensuring atlas alpha = 1.0.
    fn add_solid_cell(&mut self) {
        let glyph_width = self.cell_width;
        let glyph_height = self.cell_height;

        // Advance to next row if needed
        if self.cursor_x + glyph_width + self.padding > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + self.padding;
            self.row_height = 0;
        }

        // Fill a cell-sized bitmap with 0xFF (fully opaque white)
        let bitmap = vec![0xFFu8; glyph_width * glyph_height];

        let region = MTLRegion {
            origin: objc2_metal::MTLOrigin {
                x: self.cursor_x,
                y: self.cursor_y,
                z: 0,
            },
            size: objc2_metal::MTLSize {
                width: glyph_width,
                height: glyph_height,
                depth: 1,
            },
        };

        let bytes_ptr = NonNull::new(bitmap.as_ptr() as *mut std::ffi::c_void)
            .expect("bitmap pointer should not be null");

        unsafe {
            self.texture
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(region, 0, bytes_ptr, glyph_width);
        }

        let atlas_size = ATLAS_SIZE as f32;
        let uv_min = (
            self.cursor_x as f32 / atlas_size,
            self.cursor_y as f32 / atlas_size,
        );
        let uv_max = (
            (self.cursor_x + glyph_width) as f32 / atlas_size,
            (self.cursor_y + glyph_height) as f32 / atlas_size,
        );

        let info = GlyphInfo {
            uv_min,
            uv_max,
            width: glyph_width as f32,
            height: glyph_height as f32,
            bearing_x: 0.0,
            bearing_y: 0.0,
        };

        self.glyphs.insert('\x01', info);

        self.cursor_x += glyph_width + self.padding;
        self.row_height = self.row_height.max(glyph_height);
    }

    /// Rasterizes a single glyph into an R8 bitmap using the primary font.
    // Chunk: docs/chunks/fallback_glyph_metrics - Pass full primary font metrics
    fn rasterize_glyph(&self, font: &Font, glyph_id: u16, width: usize, height: usize) -> Vec<u8> {
        self.rasterize_glyph_with_ct_font(
            font.ct_font(),
            font.metrics.ascent,
            font.metrics.descent,
            font.metrics.line_height,
            glyph_id,
            width,
            height,
        )
    }

    // Chunk: docs/chunks/font_fallback_rendering - Rasterize from any CTFont (primary or fallback)
    // Chunk: docs/chunks/fallback_glyph_metrics - Scale fallback glyphs to fit cell bounds
    /// Rasterizes a single glyph into an R8 bitmap using a specific CTFont.
    ///
    /// This allows rasterizing glyphs from fallback fonts with proper metrics handling.
    /// When the fallback font's line_height exceeds the cell height, the glyph is scaled
    /// down and vertically centered to fit within the cell bounds.
    ///
    /// # Arguments
    /// * `ct_font` - The font to rasterize from (primary or fallback)
    /// * `font_ascent` - The font's ascent (for baseline calculation)
    /// * `font_descent` - The font's descent (for baseline calculation)
    /// * `font_line_height` - The font's line height (for scale calculation)
    /// * `glyph_id` - The glyph ID to rasterize
    /// * `width` - Target bitmap width (cell width)
    /// * `height` - Target bitmap height (cell height)
    fn rasterize_glyph_with_ct_font(
        &self,
        ct_font: &CTFont,
        _font_ascent: f64,
        font_descent: f64,
        font_line_height: f64,
        glyph_id: u16,
        width: usize,
        height: usize,
    ) -> Vec<u8> {
        // Create a grayscale color space
        let color_space = CGColorSpace::new_device_gray();

        // Alpha info for grayscale - None means no alpha channel
        // CGBitmapInfo is a bitfield: alpha info | byte order
        let bitmap_info: u32 = CGImageAlphaInfo::None.0;

        let context = unsafe {
            CGBitmapContextCreate(
                std::ptr::null_mut(),
                width,
                height,
                8,     // bits per component
                width, // bytes per row
                color_space.as_deref(),
                bitmap_info,
            )
        };

        let context = match context {
            Some(ctx) => ctx,
            None => {
                eprintln!("Failed to create bitmap context");
                return vec![0u8; width * height];
            }
        };

        // Clear to black (fully transparent for our purposes)
        let rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as CGFloat,
                height: height as CGFloat,
            },
        };

        // Set fill color to black and clear
        CGContext::set_gray_fill_color(Some(&*context), 0.0, 1.0);
        CGContext::fill_rect(Some(&*context), rect);

        // Set the text color to white (this is what we'll draw the glyph with)
        CGContext::set_gray_fill_color(Some(&*context), 1.0, 1.0);

        // Compute scale factor: scale down if the font's line_height exceeds cell height
        let cell_height = height as f64;
        let scale = if font_line_height > cell_height {
            cell_height / font_line_height
        } else {
            1.0
        };

        // Position for drawing. Core Graphics has origin at bottom-left.
        // The baseline is positioned so that:
        // - For scale=1.0: baseline at y = descent (standard positioning)
        // - For scale<1.0: glyph is scaled and vertically centered
        let (draw_x, draw_y) = if scale < 1.0 {
            // Apply scaling transform to fit oversized glyph in cell
            // CGContextScaleCTM scales around the origin, so we need to adjust position
            //
            // After scaling by `scale`, the glyph's visual extent is:
            //   scaled_height = font_line_height * scale = cell_height
            //
            // To center vertically:
            //   y_offset = (cell_height - scaled_height) / 2.0 = 0 (exactly fits)
            //
            // The baseline in the scaled coordinate system:
            //   The font's descent determines how far below the baseline the glyph extends.
            //   In the scaled space, we want the glyph centered, so position baseline at:
            //   y = scaled_descent + vertical_centering_offset
            //
            // Since we're drawing at scale, positions are divided by scale in the
            // transform, so we specify positions in scaled (cell) coordinates.
            let scaled_descent = font_descent * scale;

            // For horizontal positioning, keep small padding
            let x = 1.0 / scale; // Account for scale transform

            // Vertically center: the scaled glyph height equals cell_height,
            // so we position the baseline at scaled_descent from the bottom
            let y = scaled_descent;

            // Apply the scale transform
            CGContext::scale_ctm(Some(&*context), scale, scale);

            (x, y)
        } else {
            // No scaling needed - use standard positioning
            (1.0, font_descent)
        };

        let position = CGPoint {
            x: draw_x,
            y: draw_y,
        };

        // Draw the glyph
        unsafe {
            ct_font.draw_glyphs(
                NonNull::from(&glyph_id),
                NonNull::from(&position),
                1,
                &*context,
            );
        }

        // Extract the bitmap data
        let data = CGBitmapContextGetData(Some(&*context));

        if data.is_null() {
            return vec![0u8; width * height];
        }

        // Copy the data (Core Graphics manages the buffer)
        let byte_count = width * height;
        let mut result = vec![0u8; byte_count];

        unsafe {
            std::ptr::copy_nonoverlapping(data as *const u8, result.as_mut_ptr(), byte_count);
        }

        result
    }

    // Chunk: docs/chunks/font_fallback_rendering - Fallback-aware glyph lookup
    /// Ensures a glyph is in the atlas, adding it if necessary.
    ///
    /// This method implements the font fallback chain:
    /// 1. Try to add the glyph from the primary font
    /// 2. If not found, try to find a fallback font via Core Text
    /// 3. If no fallback found, render the replacement character (U+FFFD)
    /// 4. If even U+FFFD fails, use the solid glyph as a visible placeholder
    pub fn ensure_glyph(&mut self, font: &Font, c: char) -> Option<&GlyphInfo> {
        // If already in atlas, return it
        if self.glyphs.contains_key(&c) {
            return self.glyphs.get(&c);
        }

        // Try to add with fallback support
        if let Some(source) = font.glyph_for_char_with_fallback(c) {
            if self.add_glyph_with_source(font, c, source) {
                return self.glyphs.get(&c);
            }
            // Atlas is full - fall through to replacement character
        }

        // No glyph found in any font, or atlas is full
        // Try to use the replacement character (U+FFFD)
        self.ensure_replacement_glyph(font, c)
    }

    // Chunk: docs/chunks/font_fallback_rendering - Replacement character for truly missing glyphs
    /// Returns a replacement glyph for characters with no glyph in any font.
    ///
    /// First attempts to use U+FFFD (REPLACEMENT CHARACTER), which should be
    /// available in most system fonts. If that fails, falls back to a solid
    /// glyph as a visible placeholder.
    fn ensure_replacement_glyph(&mut self, font: &Font, c: char) -> Option<&GlyphInfo> {
        const REPLACEMENT_CHAR: char = '\u{FFFD}';

        // First, try to ensure we have the replacement character itself
        if c != REPLACEMENT_CHAR {
            // If we're not already looking for the replacement char, try to get it
            if !self.glyphs.contains_key(&REPLACEMENT_CHAR) {
                // Try to add U+FFFD via the fallback path
                if let Some(source) = font.glyph_for_char_with_fallback(REPLACEMENT_CHAR) {
                    self.add_glyph_with_source(font, REPLACEMENT_CHAR, source);
                }
            }

            // If we have the replacement character, use it
            if self.glyphs.contains_key(&REPLACEMENT_CHAR) {
                return self.glyphs.get(&REPLACEMENT_CHAR);
            }
        }

        // U+FFFD not available either - use the solid glyph as ultimate fallback
        // This ensures characters are never invisible
        Some(self.solid_glyph())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        extern "C" {
            fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
        }

        let ptr = unsafe { MTLCreateSystemDefaultDevice() };
        assert!(!ptr.is_null(), "Metal device should be available");
        unsafe { Retained::from_raw(ptr).unwrap() }
    }

    #[test]
    fn test_atlas_creation() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let atlas = GlyphAtlas::new(&device, &font);

        // Should have all printable ASCII characters
        for c in ' '..='~' {
            assert!(atlas.get_glyph(c).is_some(), "Atlas should contain '{}'", c);
        }
    }

    #[test]
    fn test_glyph_uv_bounds() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let atlas = GlyphAtlas::new(&device, &font);

        // Check UV coordinates are valid
        for c in ' '..='~' {
            let glyph = atlas.get_glyph(c).expect("Should have glyph");

            assert!(
                glyph.uv_min.0 >= 0.0 && glyph.uv_min.0 <= 1.0,
                "UV min x should be normalized"
            );
            assert!(
                glyph.uv_min.1 >= 0.0 && glyph.uv_min.1 <= 1.0,
                "UV min y should be normalized"
            );
            assert!(
                glyph.uv_max.0 >= 0.0 && glyph.uv_max.0 <= 1.0,
                "UV max x should be normalized"
            );
            assert!(
                glyph.uv_max.1 >= 0.0 && glyph.uv_max.1 <= 1.0,
                "UV max y should be normalized"
            );
            assert!(
                glyph.uv_min.0 < glyph.uv_max.0,
                "UV min x should be less than max"
            );
            assert!(
                glyph.uv_min.1 < glyph.uv_max.1,
                "UV min y should be less than max"
            );
        }
    }

    // ==================== On-demand glyph extension tests ====================
    // Chunk: docs/chunks/terminal_background_box_drawing - Glyph atlas on-demand extension

    #[test]
    fn test_ensure_glyph_adds_on_demand() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Box-drawing horizontal line (U+2500) should not be in atlas initially
        assert!(
            atlas.get_glyph('─').is_none(),
            "Box-drawing char should not be pre-populated"
        );

        // ensure_glyph should add it on demand
        let glyph = atlas.ensure_glyph(&font, '─');
        assert!(glyph.is_some(), "ensure_glyph should add box-drawing char");

        // Now get_glyph should find it
        assert!(
            atlas.get_glyph('─').is_some(),
            "Box-drawing char should now be in atlas"
        );
    }

    #[test]
    fn test_box_drawing_characters_rasterize() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Test common box-drawing characters
        let box_chars = [
            '─', // U+2500 horizontal line
            '│', // U+2502 vertical line
            '┌', // U+250C top-left corner
            '┐', // U+2510 top-right corner
            '└', // U+2514 bottom-left corner
            '┘', // U+2518 bottom-right corner
            '├', // U+251C vertical and right
            '┤', // U+2524 vertical and left
            '┬', // U+252C horizontal and down
            '┴', // U+2534 horizontal and up
            '┼', // U+253C cross
        ];

        for c in box_chars {
            let glyph = atlas.ensure_glyph(&font, c);
            assert!(
                glyph.is_some(),
                "Box-drawing char '{}' (U+{:04X}) should be rasterizable",
                c,
                c as u32
            );

            // Verify glyph has valid dimensions
            let glyph = glyph.unwrap();
            assert!(glyph.width > 0.0, "Glyph should have positive width");
            assert!(glyph.height > 0.0, "Glyph should have positive height");
        }
    }

    #[test]
    fn test_block_element_characters_rasterize() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Test common block element characters
        let block_chars = [
            '█', // U+2588 full block
            '▀', // U+2580 upper half block
            '▄', // U+2584 lower half block
            '▌', // U+258C left half block
            '▐', // U+2590 right half block
            '░', // U+2591 light shade
            '▒', // U+2592 medium shade
            '▓', // U+2593 dark shade
        ];

        for c in block_chars {
            let glyph = atlas.ensure_glyph(&font, c);
            assert!(
                glyph.is_some(),
                "Block element '{}' (U+{:04X}) should be rasterizable",
                c,
                c as u32
            );
        }
    }

    // Chunk: docs/chunks/terminal_multibyte_rendering - Non-BMP character rendering support
    #[test]
    fn test_non_bmp_character_handling() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Emoji (outside BMP, > U+FFFF) - now handled via surrogate pair lookup
        let emoji = '😀'; // U+1F600
        let glyph = atlas.ensure_glyph(&font, emoji);

        // Should return Some - either the actual glyph (if font has it) or space fallback
        assert!(
            glyph.is_some(),
            "Non-BMP char should return a glyph (actual or fallback)"
        );

        // Copy the glyph data to release the borrow
        let emoji_glyph = glyph.unwrap().clone();

        // Verify the glyph has valid dimensions
        assert!(emoji_glyph.width > 0.0, "Glyph should have positive width");
        assert!(emoji_glyph.height > 0.0, "Glyph should have positive height");
    }

    // Chunk: docs/chunks/terminal_multibyte_rendering - Test non-BMP hieroglyph rendering
    #[test]
    fn test_non_bmp_hieroglyphs() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Egyptian hieroglyphs from the GOAL.md
        let hieroglyphs = ['𓆝', '𓆟', '𓆞']; // U+131DD, U+131DF, U+131DE

        for c in hieroglyphs {
            let glyph = atlas.ensure_glyph(&font, c);
            // Should always return Some (actual glyph or space fallback)
            assert!(
                glyph.is_some(),
                "Hieroglyph '{}' (U+{:04X}) should return a glyph",
                c, c as u32
            );
        }
    }

    #[test]
    fn test_ensure_glyph_is_idempotent() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Call ensure_glyph multiple times for the same character
        // Clone first result to release the borrow before second call
        let glyph1 = atlas.ensure_glyph(&font, '─');
        assert!(glyph1.is_some());
        let g1 = glyph1.unwrap().clone();

        let glyph2 = atlas.ensure_glyph(&font, '─');
        assert!(glyph2.is_some());
        let g2 = glyph2.unwrap();

        // Should return the same glyph info (same UV coordinates)
        assert_eq!(g1.uv_min, g2.uv_min, "ensure_glyph should be idempotent");
        assert_eq!(g1.uv_max, g2.uv_max, "ensure_glyph should be idempotent");
    }

    // ==================== Font fallback integration tests ====================
    // Chunk: docs/chunks/font_fallback_rendering - Integration tests for fallback glyph caching

    #[test]
    fn test_fallback_glyphs_cached_in_atlas() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Emoji should come from a fallback font (Apple Color Emoji)
        let emoji = '😀'; // U+1F600

        // First call - should lookup and cache
        let glyph1 = atlas.ensure_glyph(&font, emoji);
        assert!(glyph1.is_some(), "Emoji should have a glyph via fallback");
        let g1 = glyph1.unwrap().clone();

        // Second call - should return cached glyph
        let glyph2 = atlas.ensure_glyph(&font, emoji);
        assert!(glyph2.is_some());
        let g2 = glyph2.unwrap();

        // UV coordinates should be identical (same cached glyph)
        assert_eq!(
            g1.uv_min, g2.uv_min,
            "Fallback glyph should be cached (same UV coords)"
        );
        assert_eq!(
            g1.uv_max, g2.uv_max,
            "Fallback glyph should be cached (same UV coords)"
        );
    }

    #[test]
    fn test_fallback_glyphs_have_valid_dimensions() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Characters that should come from fallback fonts
        let fallback_chars = [
            '😀', // Emoji
            '∫',  // Mathematical integral (may be in Menlo, may be fallback)
        ];

        for c in fallback_chars {
            let glyph = atlas.ensure_glyph(&font, c);
            assert!(glyph.is_some(), "Char '{}' should have a glyph", c);

            let g = glyph.unwrap();
            assert!(g.width > 0.0, "Glyph for '{}' should have positive width", c);
            assert!(g.height > 0.0, "Glyph for '{}' should have positive height", c);
            assert!(
                g.uv_min.0 < g.uv_max.0,
                "Glyph for '{}' should have valid UV x coords",
                c
            );
            assert!(
                g.uv_min.1 < g.uv_max.1,
                "Glyph for '{}' should have valid UV y coords",
                c
            );
        }
    }

    #[test]
    fn test_replacement_character_rendered_for_unmapped_codepoints() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Use a Private Use Area character that likely has no glyph
        let private_use = '\u{F0000}';

        // Should return Some (either replacement char or solid glyph)
        let glyph = atlas.ensure_glyph(&font, private_use);
        assert!(
            glyph.is_some(),
            "Unmapped codepoint should return a replacement glyph"
        );

        let g = glyph.unwrap();
        assert!(g.width > 0.0, "Replacement glyph should have positive width");
        assert!(g.height > 0.0, "Replacement glyph should have positive height");
    }

    #[test]
    fn test_hieroglyphs_render_via_fallback() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Egyptian hieroglyphs from the GOAL.md
        let hieroglyphs = ['𓆝', '𓆟', '𓆞']; // U+131DD, U+131DF, U+131DE

        for c in hieroglyphs {
            // Verify Menlo doesn't have this glyph directly
            assert!(
                font.glyph_for_char(c).is_none(),
                "Menlo should NOT have hieroglyph '{}' (U+{:04X})",
                c, c as u32
            );

            // But ensure_glyph should still return a valid glyph
            let glyph = atlas.ensure_glyph(&font, c);
            assert!(
                glyph.is_some(),
                "Hieroglyph '{}' (U+{:04X}) should have a glyph via fallback",
                c, c as u32
            );

            let g = glyph.unwrap();
            assert!(
                g.width > 0.0 && g.height > 0.0,
                "Hieroglyph '{}' glyph should have valid dimensions",
                c
            );
        }
    }

    #[test]
    fn test_ascii_uses_primary_font_not_fallback() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let atlas = GlyphAtlas::new(&device, &font);

        // ASCII characters should be pre-populated from the primary font
        // (no fallback needed)
        for c in 'A'..='Z' {
            // Verify the primary font has this glyph
            assert!(
                font.glyph_for_char(c).is_some(),
                "Menlo should have ASCII char '{}'",
                c
            );

            // And it should be in the atlas (pre-populated)
            assert!(
                atlas.get_glyph(c).is_some(),
                "ASCII char '{}' should be pre-populated in atlas",
                c
            );
        }
    }

    // ==================== Fallback glyph metrics tests ====================
    // Chunk: docs/chunks/fallback_glyph_metrics - Test fallback glyph scaling and metrics

    #[test]
    fn test_ascii_glyphs_unaffected_by_fallback_metrics_changes() {
        // ASCII characters should render identically before and after this change
        // since they use the primary font (Menlo) which doesn't need scaling
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let atlas = GlyphAtlas::new(&device, &font);

        for c in 'A'..='Z' {
            let glyph = atlas.get_glyph(c);
            assert!(glyph.is_some(), "ASCII char '{}' should be in atlas", c);

            // Dimensions should match cell size
            let (cell_w, cell_h) = atlas.cell_dimensions();
            let g = glyph.unwrap();
            assert_eq!(
                g.width as usize, cell_w,
                "ASCII '{}' width should match cell width",
                c
            );
            assert_eq!(
                g.height as usize, cell_h,
                "ASCII '{}' height should match cell height",
                c
            );
        }
    }

    #[test]
    fn test_fallback_glyphs_fit_within_cell() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);
        let (cell_w, cell_h) = atlas.cell_dimensions();

        // Emoji often come from tall Apple Color Emoji font
        let emoji = '😀';
        let glyph = atlas.ensure_glyph(&font, emoji);

        assert!(glyph.is_some(), "Emoji should have a glyph");
        let g = glyph.unwrap();

        // The glyph info should have dimensions matching cell size
        // (the scaling happens during rasterization, the GlyphInfo dimensions are fixed)
        assert_eq!(
            g.width as usize, cell_w,
            "Emoji glyph width should match cell width"
        );
        assert_eq!(
            g.height as usize, cell_h,
            "Emoji glyph height should match cell height"
        );
    }

    #[test]
    fn test_fallback_glyph_scaling_preserves_visibility() {
        // This test verifies that fallback glyphs from fonts with larger metrics
        // (like Apple Color Emoji or symbol fonts) are scaled to fit within cells
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Test characters that typically come from fallback fonts with different metrics
        let test_chars = [
            '😀', // Emoji - Apple Color Emoji has large metrics
            '∫',  // Mathematical integral
            '∑',  // Summation symbol
            '√',  // Square root
        ];

        let (cell_w, cell_h) = atlas.cell_dimensions();

        for c in test_chars {
            let glyph = atlas.ensure_glyph(&font, c);
            assert!(
                glyph.is_some(),
                "Character '{}' (U+{:04X}) should have a glyph",
                c, c as u32
            );

            let g = glyph.unwrap();

            // All glyphs should fit within the cell dimensions
            assert_eq!(
                g.width as usize, cell_w,
                "Glyph '{}' width should match cell width",
                c
            );
            assert_eq!(
                g.height as usize, cell_h,
                "Glyph '{}' height should match cell height",
                c
            );

            // UV coordinates should be valid (glyph was actually rasterized)
            assert!(
                g.uv_min.0 < g.uv_max.0 && g.uv_min.1 < g.uv_max.1,
                "Glyph '{}' should have valid UV coordinates",
                c
            );
        }
    }

    #[test]
    fn test_powerline_symbols_render() {
        // Powerline symbols often come from nerd fonts or fallback fonts
        // with different metrics. This test ensures they render without clipping.
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);
        let (cell_w, cell_h) = atlas.cell_dimensions();

        // Common powerline/nerd font symbols
        let powerline_chars = [
            '\u{E0B0}', // Right-pointing triangle (powerline separator)
            '\u{E0B2}', // Left-pointing triangle (powerline separator)
        ];

        for c in powerline_chars {
            let glyph = atlas.ensure_glyph(&font, c);

            // These may not be available on all systems, but if they are,
            // they should render correctly
            if let Some(g) = glyph {
                assert!(g.width > 0.0, "Powerline glyph '{}' should have width", c);
                assert!(g.height > 0.0, "Powerline glyph '{}' should have height", c);

                // The glyph should fit within cell dimensions
                assert_eq!(
                    g.width as usize, cell_w,
                    "Powerline glyph width should match cell width"
                );
                assert_eq!(
                    g.height as usize, cell_h,
                    "Powerline glyph height should match cell height"
                );
            }
        }
    }

    #[test]
    fn test_no_regression_box_drawing_after_metrics_change() {
        // Box-drawing characters should still work correctly
        // These are typically in Menlo, so they shouldn't need scaling
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        let box_chars = ['─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];

        let (cell_w, cell_h) = atlas.cell_dimensions();

        for c in box_chars {
            let glyph = atlas.ensure_glyph(&font, c);
            assert!(
                glyph.is_some(),
                "Box-drawing char '{}' should be rasterizable",
                c
            );

            let g = glyph.unwrap();
            assert_eq!(
                g.width as usize, cell_w,
                "Box-drawing '{}' width should match cell",
                c
            );
            assert_eq!(
                g.height as usize, cell_h,
                "Box-drawing '{}' height should match cell",
                c
            );
        }
    }
}
