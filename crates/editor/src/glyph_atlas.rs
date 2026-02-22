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

use crate::font::Font;

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
                // No glyph for this character; use space as fallback
                if c != ' ' {
                    return self.add_glyph(font, ' ');
                }
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

    /// Rasterizes a single glyph into an R8 bitmap
    fn rasterize_glyph(&self, font: &Font, glyph_id: u16, width: usize, height: usize) -> Vec<u8> {
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

        // Position for drawing: baseline is at y = descent (from bottom)
        // Core Graphics has origin at bottom-left
        let position = CGPoint {
            x: 1.0, // Small padding
            y: font.metrics.descent,
        };

        // Draw the glyph
        unsafe {
            font.ct_font().draw_glyphs(
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

    /// Ensures a glyph is in the atlas, adding it if necessary
    pub fn ensure_glyph(&mut self, font: &Font, c: char) -> Option<&GlyphInfo> {
        if !self.glyphs.contains_key(&c) {
            if !self.add_glyph(font, c) {
                // Failed to add, try to return space glyph
                return self.glyphs.get(&' ');
            }
        }
        self.glyphs.get(&c)
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

    #[test]
    fn test_non_bmp_character_falls_back_to_space() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Emoji (outside BMP, > U+FFFF) should fall back to space glyph
        let emoji = '😀'; // U+1F600
        let glyph = atlas.ensure_glyph(&font, emoji);

        // Should return Some (the space glyph fallback)
        assert!(
            glyph.is_some(),
            "Non-BMP char should fall back to space glyph"
        );

        // The glyph returned should be the space glyph (same UV coordinates)
        let space_glyph = atlas.get_glyph(' ').unwrap();
        let emoji_glyph = glyph.unwrap();
        assert_eq!(
            space_glyph.uv_min, emoji_glyph.uv_min,
            "Non-BMP char should get space glyph UV coords"
        );
    }

    #[test]
    fn test_ensure_glyph_is_idempotent() {
        let device = get_test_device();
        let font = Font::new("Menlo-Regular", 14.0, 1.0);
        let mut atlas = GlyphAtlas::new(&device, &font);

        // Call ensure_glyph multiple times for the same character
        let glyph1 = atlas.ensure_glyph(&font, '─');
        let glyph2 = atlas.ensure_glyph(&font, '─');

        // Should return the same glyph info (same UV coordinates)
        assert!(glyph1.is_some() && glyph2.is_some());
        let g1 = glyph1.unwrap();
        let g2 = glyph2.unwrap();
        assert_eq!(g1.uv_min, g2.uv_min, "ensure_glyph should be idempotent");
        assert_eq!(g1.uv_max, g2.uv_max, "ensure_glyph should be idempotent");
    }
}
