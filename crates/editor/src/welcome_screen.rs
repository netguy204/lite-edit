// Chunk: docs/chunks/welcome_screen - Vim-style welcome/intro screen on empty tabs
//!
//! Welcome screen rendering for empty tabs.
//!
//! This module displays a Vim-style welcome screen when a file tab contains
//! an empty buffer (e.g., on initial launch or after Cmd+T creates a new tab).
//! The welcome screen shows:
//!
//! - A feather ASCII art logo (lite-edit branding)
//! - The editor name and tagline
//! - A categorized hotkey reference table
//!
//! The content is centered both horizontally and vertically within the buffer
//! viewport area. The welcome screen disappears automatically when:
//! - The user types any character (buffer becomes non-empty)
//! - A file is opened into the tab
//!
//! ## Design
//!
//! The welcome screen is purely a function of buffer state - no additional
//! state machine is needed. Empty buffer + file tab → show welcome.
//! Non-empty buffer → normal render.
//!
//! ## Colors
//!
//! Uses Catppuccin Mocha accent colors for visual appeal:
//! - Logo: Gradient using lavender, mauve, and blue
//! - Editor name: Bright white
//! - Key combos: Blue accent
//! - Descriptions: Dimmed text (Subtext1)

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::glyph_buffer::{GlyphLayout, GlyphVertex, QuadRange};
use crate::shader::VERTEX_SIZE;

// =============================================================================
// ASCII Art Logo
// =============================================================================

/// ASCII art feather logo for lite-edit.
///
/// This feather design is ~15 lines tall and ~30 chars wide, suitable for
/// typical terminal font sizes. Each line is paired with a color index for
/// gradient rendering.
const FEATHER_LOGO: &[(&str, usize)] = &[
    ("           .", 0),       // lavender
    ("          /|", 0),
    ("         / |", 0),
    ("        /  |", 1),       // mauve
    ("       /   |", 1),
    ("      /    |", 1),
    ("     /  .--'", 2),       // blue
    ("    / .'", 2),
    ("   /.'", 2),
    ("  /|", 2),
    (" / |", 1),
    ("/  |", 1),
    ("|  |", 0),
    ("|  '.", 0),
    (" \\   `'--..__", 0),
    ("  `'--..__", 0),
];

/// Editor name displayed below the logo
const EDITOR_NAME: &str = "lite-edit";

/// Tagline displayed below the editor name
const TAGLINE: &str = "A lightweight, terminal-native code editor";

// =============================================================================
// Hotkey Categories
// =============================================================================

/// Hotkey definitions organized by category.
/// Each category is (category_name, &[(key_combo, description)]).
const HOTKEYS: &[(&str, &[(&str, &str)])] = &[
    ("File", &[
        ("Cmd+S", "Save file"),
        ("Cmd+P", "Open file picker"),
        ("Cmd+N", "New workspace"),
        ("Cmd+T", "New tab"),
    ]),
    ("Navigation", &[
        ("Cmd+]", "Next workspace"),
        ("Cmd+[", "Previous workspace"),
        ("Cmd+Shift+]", "Next tab"),
        ("Cmd+Shift+[", "Previous tab"),
        ("Cmd+1-9", "Switch workspace"),
    ]),
    ("Editing", &[
        ("Cmd+F", "Find in file"),
        ("Cmd+W", "Close tab"),
        ("Cmd+Shift+W", "Close workspace"),
    ]),
    ("Terminal", &[
        ("Cmd+Shift+T", "New terminal tab"),
    ]),
    ("Application", &[
        ("Cmd+Q", "Quit"),
    ]),
];

// =============================================================================
// Colors (Catppuccin Mocha palette)
// =============================================================================

/// Lavender: #b4befe → [0.706, 0.745, 0.996, 1.0]
pub const COLOR_LAVENDER: [f32; 4] = [0.706, 0.745, 0.996, 1.0];

/// Mauve: #cba6f7 → [0.796, 0.651, 0.969, 1.0]
pub const COLOR_MAUVE: [f32; 4] = [0.796, 0.651, 0.969, 1.0];

/// Blue: #89b4fa → [0.537, 0.706, 0.980, 1.0]
pub const COLOR_BLUE: [f32; 4] = [0.537, 0.706, 0.980, 1.0];

/// Text (bright white): #cdd6f4 → [0.804, 0.839, 0.957, 1.0]
pub const COLOR_TEXT: [f32; 4] = [0.804, 0.839, 0.957, 1.0];

/// Subtext1 (dimmed): #bac2de → [0.729, 0.761, 0.871, 1.0]
pub const COLOR_SUBTEXT: [f32; 4] = [0.729, 0.761, 0.871, 1.0];

/// Overlay0 (category headers): #6c7086 → [0.424, 0.439, 0.525, 1.0]
pub const COLOR_OVERLAY: [f32; 4] = [0.424, 0.439, 0.525, 1.0];

/// Logo gradient colors indexed by FEATHER_LOGO's color index
const LOGO_GRADIENT: &[[f32; 4]] = &[
    COLOR_LAVENDER,
    COLOR_MAUVE,
    COLOR_BLUE,
];

// =============================================================================
// Layout Constants
// =============================================================================

/// Vertical spacing between logo and editor name (in lines)
const LOGO_NAME_GAP: usize = 2;

/// Vertical spacing between editor name and tagline (in lines)
const NAME_TAGLINE_GAP: usize = 1;

/// Vertical spacing between tagline and hotkey table (in lines)
const TAGLINE_HOTKEYS_GAP: usize = 3;

/// Vertical spacing between hotkey categories (in lines)
const CATEGORY_GAP: usize = 1;

/// Horizontal padding for hotkey table (in characters)
const HOTKEY_PADDING: usize = 2;

/// Width of key combo column (in characters)
const KEY_COLUMN_WIDTH: usize = 16;

// =============================================================================
// WelcomeScreenGeometry
// =============================================================================

/// Computed geometry for the welcome screen content.
///
/// All values are in screen coordinates (pixels).
#[derive(Debug, Clone, Copy)]
pub struct WelcomeScreenGeometry {
    /// X offset to center content horizontally
    pub content_x: f32,
    /// Y offset to center content vertically
    pub content_y: f32,
    /// Width of a single character
    pub glyph_width: f32,
    /// Height of a line
    pub line_height: f32,
    /// Total content width in characters
    pub content_width_chars: usize,
    /// Total content height in lines
    pub content_height_lines: usize,
}

/// Calculates the geometry for the welcome screen.
///
/// Centers the content both horizontally and vertically within the viewport.
///
/// # Arguments
/// * `viewport_width` - Available viewport width in pixels
/// * `viewport_height` - Available viewport height in pixels
/// * `glyph_width` - Width of a single character in pixels
/// * `line_height` - Height of a line in pixels
pub fn calculate_welcome_geometry(
    viewport_width: f32,
    viewport_height: f32,
    glyph_width: f32,
    line_height: f32,
) -> WelcomeScreenGeometry {
    // Calculate content dimensions
    let (content_width_chars, content_height_lines) = calculate_content_dimensions();

    // Calculate pixel dimensions
    let content_width_px = content_width_chars as f32 * glyph_width;
    let content_height_px = content_height_lines as f32 * line_height;

    // Center horizontally and vertically
    let content_x = ((viewport_width - content_width_px) / 2.0).max(0.0);
    let content_y = ((viewport_height - content_height_px) / 2.0).max(0.0);

    WelcomeScreenGeometry {
        content_x,
        content_y,
        glyph_width,
        line_height,
        content_width_chars,
        content_height_lines,
    }
}

/// Calculates the total content dimensions (width in chars, height in lines).
fn calculate_content_dimensions() -> (usize, usize) {
    // Logo width and height
    let logo_width = FEATHER_LOGO.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    let logo_height = FEATHER_LOGO.len();

    // Editor name and tagline
    let name_width = EDITOR_NAME.len();
    let tagline_width = TAGLINE.len();

    // Hotkey table dimensions
    let hotkey_width = calculate_hotkey_table_width();
    let hotkey_height = calculate_hotkey_table_height();

    // Total width is the max of all sections
    let total_width = logo_width
        .max(name_width)
        .max(tagline_width)
        .max(hotkey_width);

    // Total height includes all sections and gaps
    let total_height = logo_height
        + LOGO_NAME_GAP
        + 1 // editor name
        + NAME_TAGLINE_GAP
        + 1 // tagline
        + TAGLINE_HOTKEYS_GAP
        + hotkey_height;

    (total_width, total_height)
}

/// Calculates the width of the hotkey table in characters.
fn calculate_hotkey_table_width() -> usize {
    let mut max_width = 0;
    for (category, hotkeys) in HOTKEYS {
        // Category header width
        max_width = max_width.max(category.len());
        // Hotkey entry width: padding + key + gap + description + padding
        for (_key, desc) in *hotkeys {
            let entry_width = HOTKEY_PADDING + KEY_COLUMN_WIDTH + desc.len() + HOTKEY_PADDING;
            max_width = max_width.max(entry_width);
        }
    }
    max_width
}

/// Calculates the height of the hotkey table in lines.
fn calculate_hotkey_table_height() -> usize {
    let mut total = 0;
    for (i, (_, hotkeys)) in HOTKEYS.iter().enumerate() {
        // Category header
        total += 1;
        // Hotkey entries
        total += hotkeys.len();
        // Gap between categories (except after last)
        if i < HOTKEYS.len() - 1 {
            total += CATEGORY_GAP;
        }
    }
    total
}

// =============================================================================
// WelcomeScreenGlyphBuffer
// =============================================================================

/// Manages vertex and index buffers for rendering the welcome screen.
///
/// This is similar to `SelectorGlyphBuffer` but specialized for the welcome
/// screen content. It renders the ASCII logo, editor name, tagline, and
/// hotkey reference table.
pub struct WelcomeScreenGlyphBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,
    /// Layout calculator for glyph positioning
    layout: GlyphLayout,

    // Quad ranges for different draw phases
    /// Logo text glyphs (multiple colors)
    logo_range: QuadRange,
    /// Editor name and tagline glyphs
    title_range: QuadRange,
    /// Hotkey table glyphs (keys and descriptions)
    hotkey_range: QuadRange,
}

impl WelcomeScreenGlyphBuffer {
    /// Creates a new empty welcome screen glyph buffer.
    pub fn new(layout: GlyphLayout) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout,
            logo_range: QuadRange::default(),
            title_range: QuadRange::default(),
            hotkey_range: QuadRange::default(),
        }
    }

    /// Returns the vertex buffer, if any.
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.vertex_buffer.as_deref()
    }

    /// Returns the index buffer, if any.
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.index_buffer.as_deref()
    }

    /// Returns the total number of indices.
    pub fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the index range for logo glyphs.
    pub fn logo_range(&self) -> QuadRange {
        self.logo_range
    }

    /// Returns the index range for title glyphs.
    pub fn title_range(&self) -> QuadRange {
        self.title_range
    }

    /// Returns the index range for hotkey glyphs.
    pub fn hotkey_range(&self) -> QuadRange {
        self.hotkey_range
    }

    /// Updates the buffers with welcome screen content.
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas for text rendering
    /// * `geometry` - The computed welcome screen geometry
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        geometry: &WelcomeScreenGeometry,
    ) {
        // Estimate capacity: logo + name + tagline + all hotkeys
        let logo_chars: usize = FEATHER_LOGO.iter().map(|(s, _)| s.len()).sum();
        let name_chars = EDITOR_NAME.len() + TAGLINE.len();
        let hotkey_chars: usize = HOTKEYS
            .iter()
            .flat_map(|(cat, keys)| {
                std::iter::once(cat.len()).chain(keys.iter().map(|(k, d)| k.len() + d.len()))
            })
            .sum();
        let estimated_quads = logo_chars + name_chars + hotkey_chars;

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_quads * 6);
        let mut vertex_offset: u32 = 0;

        // Reset ranges
        self.logo_range = QuadRange::default();
        self.title_range = QuadRange::default();
        self.hotkey_range = QuadRange::default();

        let mut current_line: usize = 0;

        // ==================== Phase 1: Logo ====================
        let logo_start = indices.len();

        // Calculate logo centering offset within content area
        let logo_width = FEATHER_LOGO.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
        let logo_x_offset = (geometry.content_width_chars.saturating_sub(logo_width)) / 2;

        for (line_text, color_idx) in FEATHER_LOGO {
            let color = LOGO_GRADIENT.get(*color_idx).copied().unwrap_or(COLOR_LAVENDER);
            let line_x_offset = logo_x_offset;

            self.emit_line(
                &mut vertices,
                &mut indices,
                &mut vertex_offset,
                atlas,
                geometry,
                line_text,
                current_line,
                line_x_offset,
                color,
            );
            current_line += 1;
        }

        self.logo_range = QuadRange::new(logo_start, indices.len() - logo_start);

        // ==================== Phase 2: Title (Name + Tagline) ====================
        let title_start = indices.len();

        // Gap after logo
        current_line += LOGO_NAME_GAP;

        // Editor name (centered, bright white)
        let name_x_offset = (geometry.content_width_chars.saturating_sub(EDITOR_NAME.len())) / 2;
        self.emit_line(
            &mut vertices,
            &mut indices,
            &mut vertex_offset,
            atlas,
            geometry,
            EDITOR_NAME,
            current_line,
            name_x_offset,
            COLOR_TEXT,
        );
        current_line += 1;

        // Gap after name
        current_line += NAME_TAGLINE_GAP;

        // Tagline (centered, dimmed)
        let tagline_x_offset = (geometry.content_width_chars.saturating_sub(TAGLINE.len())) / 2;
        self.emit_line(
            &mut vertices,
            &mut indices,
            &mut vertex_offset,
            atlas,
            geometry,
            TAGLINE,
            current_line,
            tagline_x_offset,
            COLOR_SUBTEXT,
        );
        current_line += 1;

        self.title_range = QuadRange::new(title_start, indices.len() - title_start);

        // ==================== Phase 3: Hotkey Table ====================
        let hotkey_start = indices.len();

        // Gap after tagline
        current_line += TAGLINE_HOTKEYS_GAP;

        // Calculate hotkey table centering
        let table_width = calculate_hotkey_table_width();
        let table_x_offset = (geometry.content_width_chars.saturating_sub(table_width)) / 2;

        for (i, (category, hotkeys)) in HOTKEYS.iter().enumerate() {
            // Category header (overlay color)
            self.emit_line(
                &mut vertices,
                &mut indices,
                &mut vertex_offset,
                atlas,
                geometry,
                category,
                current_line,
                table_x_offset,
                COLOR_OVERLAY,
            );
            current_line += 1;

            // Hotkey entries
            for (key, desc) in *hotkeys {
                // Key combo (blue)
                self.emit_line(
                    &mut vertices,
                    &mut indices,
                    &mut vertex_offset,
                    atlas,
                    geometry,
                    key,
                    current_line,
                    table_x_offset + HOTKEY_PADDING,
                    COLOR_BLUE,
                );

                // Description (dimmed)
                self.emit_line(
                    &mut vertices,
                    &mut indices,
                    &mut vertex_offset,
                    atlas,
                    geometry,
                    desc,
                    current_line,
                    table_x_offset + HOTKEY_PADDING + KEY_COLUMN_WIDTH,
                    COLOR_SUBTEXT,
                );
                current_line += 1;
            }

            // Gap between categories (except after last)
            if i < HOTKEYS.len() - 1 {
                current_line += CATEGORY_GAP;
            }
        }

        self.hotkey_range = QuadRange::new(hotkey_start, indices.len() - hotkey_start);

        // ==================== Create GPU Buffers ====================
        if vertices.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Create the vertex buffer
        let vertex_data_size = vertices.len() * VERTEX_SIZE;
        let vertex_ptr =
            NonNull::new(vertices.as_ptr() as *mut std::ffi::c_void).expect("vertex ptr not null");

        let vertex_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    vertex_ptr,
                    vertex_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create vertex buffer")
        };

        // Create the index buffer
        let index_data_size = indices.len() * std::mem::size_of::<u32>();
        let index_ptr =
            NonNull::new(indices.as_ptr() as *mut std::ffi::c_void).expect("index ptr not null");

        let index_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    index_ptr,
                    index_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer")
        };

        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.index_count = indices.len();
    }

    /// Emits glyphs for a line of text.
    #[allow(clippy::too_many_arguments)]
    fn emit_line(
        &self,
        vertices: &mut Vec<GlyphVertex>,
        indices: &mut Vec<u32>,
        vertex_offset: &mut u32,
        atlas: &GlyphAtlas,
        geometry: &WelcomeScreenGeometry,
        text: &str,
        line: usize,
        x_offset_chars: usize,
        color: [f32; 4],
    ) {
        for (col, c) in text.chars().enumerate() {
            // Skip spaces (they don't need quads)
            if c == ' ' {
                continue;
            }

            if let Some(glyph) = atlas.get_glyph(c) {
                let x = geometry.content_x + (x_offset_chars + col) as f32 * geometry.glyph_width;
                let y = geometry.content_y + line as f32 * geometry.line_height;

                let quad = self.create_glyph_quad_at(x, y, glyph, color);
                vertices.extend_from_slice(&quad);
                Self::push_quad_indices(indices, *vertex_offset);
                *vertex_offset += 4;
            }
        }
    }

    /// Creates a glyph quad at an absolute position with the specified color.
    fn create_glyph_quad_at(&self, x: f32, y: f32, glyph: &GlyphInfo, color: [f32; 4]) -> [GlyphVertex; 4] {
        let (u0, v0) = glyph.uv_min;
        let (u1, v1) = glyph.uv_max;

        let w = glyph.width;
        let h = glyph.height;

        [
            GlyphVertex::new(x, y, u0, v0, color),         // top-left
            GlyphVertex::new(x + w, y, u1, v0, color),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1, color),     // bottom-left
        ]
    }

    /// Pushes indices for a quad (two triangles).
    fn push_quad_indices(indices: &mut Vec<u32>, vertex_offset: u32) {
        // Triangle 1: top-left, top-right, bottom-right
        indices.push(vertex_offset);
        indices.push(vertex_offset + 1);
        indices.push(vertex_offset + 2);
        // Triangle 2: top-left, bottom-right, bottom-left
        indices.push(vertex_offset);
        indices.push(vertex_offset + 2);
        indices.push(vertex_offset + 3);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logo_has_lines() {
        assert!(!FEATHER_LOGO.is_empty());
    }

    #[test]
    fn test_hotkeys_have_entries() {
        assert!(!HOTKEYS.is_empty());
        for (_, hotkeys) in HOTKEYS {
            assert!(!hotkeys.is_empty());
        }
    }

    #[test]
    fn test_content_dimensions_are_reasonable() {
        let (width, height) = calculate_content_dimensions();
        // Content should be at least logo-sized
        assert!(width >= 10);
        assert!(height >= 10);
        // Content shouldn't be excessively large
        assert!(width < 100);
        assert!(height < 100);
    }

    #[test]
    fn test_geometry_calculation() {
        // Use a large viewport that can fit all content
        let geometry = calculate_welcome_geometry(1200.0, 1000.0, 8.0, 16.0);

        // Content should be centered (positive x offset)
        assert!(geometry.content_x > 0.0, "content_x should be > 0 for large viewport");
        assert!(geometry.content_y > 0.0, "content_y should be > 0 for large viewport");

        // Content dimensions should be positive
        assert!(geometry.content_width_chars > 0);
        assert!(geometry.content_height_lines > 0);

        // Verify centering math
        let content_width_px = geometry.content_width_chars as f32 * geometry.glyph_width;
        let expected_x = (1200.0 - content_width_px) / 2.0;
        assert!((geometry.content_x - expected_x).abs() < 0.001);
    }

    #[test]
    fn test_geometry_small_viewport() {
        // Very small viewport should clamp content_x and content_y to 0
        let geometry = calculate_welcome_geometry(50.0, 50.0, 8.0, 16.0);

        // Should not be negative
        assert!(geometry.content_x >= 0.0);
        assert!(geometry.content_y >= 0.0);
    }

    #[test]
    fn test_hotkey_table_width() {
        let width = calculate_hotkey_table_width();
        // Should be at least the key column width + some description
        assert!(width >= KEY_COLUMN_WIDTH);
    }

    #[test]
    fn test_hotkey_table_height() {
        let height = calculate_hotkey_table_height();
        // Should have at least as many lines as category headers + entries
        let min_height: usize = HOTKEYS.iter().map(|(_, ks)| 1 + ks.len()).sum();
        assert!(height >= min_height);
    }
}
