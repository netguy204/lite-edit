// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
//!
//! Glyph vertex buffer construction
//!
//! This module takes text content and produces vertex/index buffers for
//! rendering textured glyph quads. Each character becomes a quad with
//! four vertices positioned in screen coordinates.
//!
//! Layout for monospace fonts is trivial:
//! - x = col * glyph_width
//! - y = row * line_height
//!
//! The viewport-aware rendering methods allow rendering a subset of buffer lines
//! (visible in the viewport) and include cursor rendering support.

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::font::FontMetrics;
use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::shader::VERTEX_SIZE;
use crate::viewport::Viewport;
use lite_edit_buffer::TextBuffer;

// =============================================================================
// Vertex Data
// =============================================================================

/// A single vertex in a glyph quad
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GlyphVertex {
    /// Position in screen coordinates (pixels)
    pub position: [f32; 2],
    /// Texture UV coordinates (normalized 0-1)
    pub uv: [f32; 2],
}

impl GlyphVertex {
    pub fn new(x: f32, y: f32, u: f32, v: f32) -> Self {
        Self {
            position: [x, y],
            uv: [u, v],
        }
    }
}

// =============================================================================
// Layout Calculator
// =============================================================================

/// Pure layout calculation for glyph positioning (testable without Metal)
pub struct GlyphLayout {
    /// Width of each glyph cell in pixels
    pub glyph_width: f32,
    /// Height of each line in pixels
    pub line_height: f32,
    /// Distance from top of line to baseline
    pub ascent: f32,
}

impl GlyphLayout {
    /// Creates a new layout calculator from font metrics
    pub fn from_metrics(metrics: &FontMetrics) -> Self {
        Self {
            glyph_width: metrics.advance_width as f32,
            line_height: metrics.line_height as f32,
            ascent: metrics.ascent as f32,
        }
    }

    /// Calculates the screen position for a character at (row, col)
    ///
    /// Returns (x, y) where (0, 0) is the top-left of the text area.
    pub fn position_for(&self, row: usize, col: usize) -> (f32, f32) {
        let x = col as f32 * self.glyph_width;
        let y = row as f32 * self.line_height;
        (x, y)
    }

    /// Generates the four vertices for a glyph quad at (row, col)
    ///
    /// The quad covers the glyph cell with the given UV coordinates.
    pub fn quad_vertices(
        &self,
        row: usize,
        col: usize,
        glyph: &GlyphInfo,
    ) -> [GlyphVertex; 4] {
        let (x, y) = self.position_for(row, col);

        // Quad dimensions match the glyph cell size
        let w = glyph.width;
        let h = glyph.height;

        // UV coordinates from the glyph info
        let (u0, v0) = glyph.uv_min;
        let (u1, v1) = glyph.uv_max;

        // Four corners: top-left, top-right, bottom-right, bottom-left
        [
            GlyphVertex::new(x, y, u0, v0),         // top-left
            GlyphVertex::new(x + w, y, u1, v0),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1),     // bottom-left
        ]
    }
}

// =============================================================================
// Glyph Buffer
// =============================================================================

/// Manages vertex and index buffers for rendering text
pub struct GlyphBuffer {
    /// The vertex buffer containing glyph quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Number of indices to draw
    index_count: usize,
    /// Layout calculator
    layout: GlyphLayout,
}

impl GlyphBuffer {
    /// Creates a new empty glyph buffer
    pub fn new(metrics: &FontMetrics) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout: GlyphLayout::from_metrics(metrics),
        }
    }

    /// Returns the vertex buffer, if any
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.vertex_buffer.as_deref()
    }

    /// Returns the index buffer, if any
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.index_buffer.as_deref()
    }

    /// Returns the number of indices to draw
    pub fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the layout calculator
    pub fn layout(&self) -> &GlyphLayout {
        &self.layout
    }

    /// Updates the buffers with new text content
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings
    /// * `lines` - The text lines to render
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        lines: &[&str],
    ) {
        // Count total characters to size the buffers
        let char_count: usize = lines.iter().map(|l| l.chars().count()).sum();

        if char_count == 0 {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Allocate vertex and index data
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(char_count * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(char_count * 6);

        let mut vertex_offset: u32 = 0;

        for (row, line) in lines.iter().enumerate() {
            for (col, c) in line.chars().enumerate() {
                // Skip spaces (they don't need quads)
                if c == ' ' {
                    continue;
                }

                // Get the glyph info from the atlas
                let glyph = match atlas.get_glyph(c) {
                    Some(g) => g,
                    None => {
                        // Character not in atlas, skip it
                        continue;
                    }
                };

                // Generate the quad vertices
                let quad = self.layout.quad_vertices(row, col, glyph);
                vertices.extend_from_slice(&quad);

                // Generate indices for two triangles
                // Triangle 1: top-left, top-right, bottom-right
                // Triangle 2: top-left, bottom-right, bottom-left
                indices.push(vertex_offset);
                indices.push(vertex_offset + 1);
                indices.push(vertex_offset + 2);
                indices.push(vertex_offset);
                indices.push(vertex_offset + 2);
                indices.push(vertex_offset + 3);

                vertex_offset += 4;
            }
        }

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

        // SAFETY: We're passing valid vertex data to create a buffer
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

        // SAFETY: We're passing valid index data to create a buffer
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

    /// Updates the buffers with content from a TextBuffer, rendering only visible lines
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings
    /// * `buffer` - The text buffer to render from
    /// * `viewport` - The viewport defining which lines are visible
    pub fn update_from_buffer(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        buffer: &TextBuffer,
        viewport: &Viewport,
    ) {
        self.update_from_buffer_with_cursor(device, atlas, buffer, viewport, true);
    }

    /// Updates the buffers with content from a TextBuffer, including cursor rendering
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings
    /// * `buffer` - The text buffer to render from
    /// * `viewport` - The viewport defining which lines are visible
    /// * `cursor_visible` - Whether to render the cursor (for future blink support)
    pub fn update_from_buffer_with_cursor(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        buffer: &TextBuffer,
        viewport: &Viewport,
        cursor_visible: bool,
    ) {
        let visible_range = viewport.visible_range(buffer.line_count());

        // Estimate character count for buffer sizing
        // Add 1 for cursor quad
        let mut estimated_chars: usize = 0;
        for line in visible_range.clone() {
            estimated_chars += buffer.line_content(line).chars().count();
        }
        let cursor_quads = if cursor_visible { 1 } else { 0 };

        if estimated_chars == 0 && cursor_quads == 0 {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Allocate vertex and index data with capacity for chars + cursor
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity((estimated_chars + cursor_quads) * 4);
        let mut indices: Vec<u32> = Vec::with_capacity((estimated_chars + cursor_quads) * 6);

        let mut vertex_offset: u32 = 0;

        // Render visible lines
        for buffer_line in visible_range.clone() {
            let screen_row = buffer_line - viewport.scroll_offset;
            let line_content = buffer.line_content(buffer_line);

            for (col, c) in line_content.chars().enumerate() {
                // Skip spaces (they don't need quads)
                if c == ' ' {
                    continue;
                }

                // Get the glyph info from the atlas
                let glyph = match atlas.get_glyph(c) {
                    Some(g) => g,
                    None => {
                        // Character not in atlas, skip it
                        continue;
                    }
                };

                // Generate the quad vertices
                let quad = self.layout.quad_vertices(screen_row, col, glyph);
                vertices.extend_from_slice(&quad);

                // Generate indices for two triangles
                indices.push(vertex_offset);
                indices.push(vertex_offset + 1);
                indices.push(vertex_offset + 2);
                indices.push(vertex_offset);
                indices.push(vertex_offset + 2);
                indices.push(vertex_offset + 3);

                vertex_offset += 4;
            }
        }

        // Render cursor if visible and in viewport
        if cursor_visible {
            let cursor_pos = buffer.cursor_position();
            if let Some(screen_line) = viewport.buffer_line_to_screen_line(cursor_pos.line) {
                // Render cursor as a block cursor using the solid (fully opaque)
                // atlas region so the fragment shader produces a visible quad.
                {
                    let solid_glyph = atlas.solid_glyph();
                    let cursor_quad = self.create_cursor_quad(
                        screen_line,
                        cursor_pos.col,
                        solid_glyph,
                    );
                    vertices.extend_from_slice(&cursor_quad);

                    // Generate indices for the cursor quad
                    indices.push(vertex_offset);
                    indices.push(vertex_offset + 1);
                    indices.push(vertex_offset + 2);
                    indices.push(vertex_offset);
                    indices.push(vertex_offset + 2);
                    indices.push(vertex_offset + 3);
                }
            }
        }

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

    /// Creates a cursor quad at the specified screen position
    ///
    /// The cursor is rendered as a solid block that uses a portion of the atlas
    /// that is guaranteed to be opaque (we use the same technique as text but
    /// the shader will render it with a different color).
    fn create_cursor_quad(
        &self,
        screen_row: usize,
        col: usize,
        reference_glyph: &GlyphInfo,
    ) -> [GlyphVertex; 4] {
        let (x, y) = self.layout.position_for(screen_row, col);

        // Cursor width is a thin bar (2 pixels) for line cursor
        // For now we use a block cursor that's the full glyph width
        let cursor_width = self.layout.glyph_width;
        let cursor_height = reference_glyph.height;

        // Use a small portion of the atlas that should be opaque
        // We use a single pixel from the space glyph area - it doesn't matter
        // what's there since the shader will apply a solid color
        let (u0, v0) = reference_glyph.uv_min;
        let (u1, v1) = reference_glyph.uv_max;

        // For a solid cursor, we just need any UV region
        // The fragment shader will handle the color
        [
            GlyphVertex::new(x, y, u0, v0),                             // top-left
            GlyphVertex::new(x + cursor_width, y, u1, v0),              // top-right
            GlyphVertex::new(x + cursor_width, y + cursor_height, u1, v1), // bottom-right
            GlyphVertex::new(x, y + cursor_height, u0, v1),             // bottom-left
        ]
    }

    /// Returns whether the last rendered content includes a cursor
    ///
    /// This is useful for determining if we need special cursor rendering
    /// in the shader pipeline.
    pub fn has_cursor(&self) -> bool {
        // For now, we always include cursor when update_from_buffer_with_cursor is called
        // with cursor_visible = true and the cursor is in the viewport
        // The actual state is embedded in the vertex data
        true
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metrics() -> FontMetrics {
        FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        }
    }

    #[test]
    fn test_layout_position() {
        let layout = GlyphLayout::from_metrics(&test_metrics());

        // Position at (0, 0) should be at origin
        let (x, y) = layout.position_for(0, 0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);

        // Position at (1, 5) should be offset
        let (x, y) = layout.position_for(1, 5);
        assert_eq!(x, 40.0); // 5 * 8
        assert_eq!(y, 16.0); // 1 * 16
    }

    #[test]
    fn test_quad_vertices() {
        let layout = GlyphLayout::from_metrics(&test_metrics());

        let glyph = GlyphInfo {
            uv_min: (0.0, 0.0),
            uv_max: (0.1, 0.2),
            width: 10.0,
            height: 18.0,
            bearing_x: 0.0,
            bearing_y: 12.0,
        };

        let quad = layout.quad_vertices(0, 0, &glyph);

        // Check positions
        assert_eq!(quad[0].position, [0.0, 0.0]);    // top-left
        assert_eq!(quad[1].position, [10.0, 0.0]);   // top-right
        assert_eq!(quad[2].position, [10.0, 18.0]);  // bottom-right
        assert_eq!(quad[3].position, [0.0, 18.0]);   // bottom-left

        // Check UVs
        assert_eq!(quad[0].uv, [0.0, 0.0]);
        assert_eq!(quad[1].uv, [0.1, 0.0]);
        assert_eq!(quad[2].uv, [0.1, 0.2]);
        assert_eq!(quad[3].uv, [0.0, 0.2]);
    }

    #[test]
    fn test_vertex_size() {
        // Verify our vertex struct matches the expected size
        assert_eq!(
            std::mem::size_of::<GlyphVertex>(),
            VERTEX_SIZE,
            "GlyphVertex size should match VERTEX_SIZE"
        );
    }
}
