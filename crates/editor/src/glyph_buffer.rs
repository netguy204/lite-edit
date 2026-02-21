// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/text_selection_rendering - Selection highlight rendering
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
//! (visible in the viewport) and include cursor and selection rendering support.
//!
//! ## Quad Categories
//!
//! The buffer emits three types of quads in a specific order:
//! 1. **Selection quads** - Semi-transparent background highlights for selected text
//! 2. **Glyph quads** - The actual text characters
//! 3. **Cursor quad** - The block cursor at the current position
//!
//! Each category has its own index range tracked separately, allowing the renderer
//! to draw each with different colors via separate draw calls.

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

    // Chunk: docs/chunks/viewport_fractional_scroll - Position with Y offset for smooth scrolling
    /// Calculates the screen position for a character at (row, col) with Y offset
    ///
    /// Returns (x, y) where the Y coordinate is shifted by `-y_offset` pixels.
    /// This is used for smooth scrolling where content is shifted up by the
    /// fractional scroll amount.
    pub fn position_for_with_offset(&self, row: usize, col: usize, y_offset: f32) -> (f32, f32) {
        let x = col as f32 * self.glyph_width;
        let y = row as f32 * self.line_height - y_offset;
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
        self.quad_vertices_with_offset(row, col, glyph, 0.0)
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Quad vertices with Y offset for smooth scrolling
    /// Generates the four vertices for a glyph quad at (row, col) with Y offset
    ///
    /// The Y coordinate is shifted by `-y_offset` pixels for smooth scrolling.
    /// When scrolled to a fractional position, the fractional remainder is passed
    /// as `y_offset` to shift all content up, causing the top line to be partially
    /// clipped and producing smooth sub-pixel scroll animation.
    pub fn quad_vertices_with_offset(
        &self,
        row: usize,
        col: usize,
        glyph: &GlyphInfo,
        y_offset: f32,
    ) -> [GlyphVertex; 4] {
        let (x, y) = self.position_for_with_offset(row, col, y_offset);

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

/// Index range for a category of quads (start index, count)
#[derive(Debug, Clone, Copy, Default)]
pub struct QuadRange {
    /// Starting index in the index buffer
    pub start: usize,
    /// Number of indices in this range
    pub count: usize,
}

impl QuadRange {
    /// Creates a new QuadRange
    pub fn new(start: usize, count: usize) -> Self {
        Self { start, count }
    }

    /// Returns true if this range is empty (no quads)
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

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
    /// Index range for selection highlight quads
    selection_range: QuadRange,
    /// Index range for glyph (text character) quads
    glyph_range: QuadRange,
    /// Index range for cursor quad
    cursor_range: QuadRange,
}

impl GlyphBuffer {
    /// Creates a new empty glyph buffer
    pub fn new(metrics: &FontMetrics) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout: GlyphLayout::from_metrics(metrics),
            selection_range: QuadRange::default(),
            glyph_range: QuadRange::default(),
            cursor_range: QuadRange::default(),
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

    /// Returns the index range for selection highlight quads
    pub fn selection_range(&self) -> QuadRange {
        self.selection_range
    }

    /// Returns the index range for glyph (text character) quads
    pub fn glyph_range(&self) -> QuadRange {
        self.glyph_range
    }

    /// Returns the index range for the cursor quad
    pub fn cursor_range(&self) -> QuadRange {
        self.cursor_range
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
        self.update_from_buffer_with_cursor(device, atlas, buffer, viewport, true, 0.0);
    }

    /// Updates the buffers with content from a TextBuffer, including cursor and selection rendering
    ///
    /// Emits quads in this order:
    /// 1. Selection highlight quads (drawn first, behind text)
    /// 2. Glyph quads (text characters)
    /// 3. Cursor quad (drawn last, on top)
    ///
    /// Each category's index range is tracked separately to allow the renderer
    /// to draw each with different colors.
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings
    /// * `buffer` - The text buffer to render from
    /// * `viewport` - The viewport defining which lines are visible
    /// * `cursor_visible` - Whether to render the cursor (for future blink support)
    /// * `y_offset` - Vertical offset in pixels for smooth scrolling. When scrolled to a
    ///   fractional position (e.g., 2.5 lines), pass the fractional remainder (e.g., 0.5 * line_height)
    ///   to shift all content up, causing the top line to be partially clipped.
    // Chunk: docs/chunks/viewport_fractional_scroll - Y offset parameter for smooth scrolling
    pub fn update_from_buffer_with_cursor(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        buffer: &TextBuffer,
        viewport: &Viewport,
        cursor_visible: bool,
        y_offset: f32,
    ) {
        let visible_range = viewport.visible_range(buffer.line_count());

        // Estimate character count for buffer sizing
        // Add extra for selection quads (one per visible line in selection)
        // and 1 for cursor quad
        let mut estimated_chars: usize = 0;
        for line in visible_range.clone() {
            estimated_chars += buffer.line_content(line).chars().count();
        }
        let selection_lines = visible_range.len(); // Max selection quads
        let cursor_quads = if cursor_visible { 1 } else { 0 };
        let total_estimated = estimated_chars + selection_lines + cursor_quads;

        // Reset quad ranges
        self.selection_range = QuadRange::default();
        self.glyph_range = QuadRange::default();
        self.cursor_range = QuadRange::default();

        if estimated_chars == 0 && cursor_quads == 0 && buffer.selection_range().is_none() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Allocate vertex and index data
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(total_estimated * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(total_estimated * 6);
        let mut vertex_offset: u32 = 0;

        // ==================== Phase 1: Selection Quads ====================
        let selection_start_index = indices.len();

        if let Some((sel_start, sel_end)) = buffer.selection_range() {
            let solid_glyph = atlas.solid_glyph();

            // For each visible line that intersects the selection
            for buffer_line in visible_range.clone() {
                // Check if this line intersects the selection
                if buffer_line < sel_start.line || buffer_line > sel_end.line {
                    continue;
                }

                let screen_row = buffer_line - viewport.first_visible_line();
                let line_len = buffer.line_len(buffer_line);

                // Calculate selection columns for this line
                let start_col = if buffer_line == sel_start.line {
                    sel_start.col
                } else {
                    0
                };
                let end_col = if buffer_line == sel_end.line {
                    sel_end.col
                } else {
                    // Include space for newline character visualization
                    line_len + 1
                };

                // Skip if no columns are selected on this line
                if start_col >= end_col {
                    continue;
                }

                // Emit a single selection quad covering the selected range
                let quad = self.create_selection_quad_with_offset(screen_row, start_col, end_col, solid_glyph, y_offset);
                vertices.extend_from_slice(&quad);

                // Generate indices for the selection quad
                indices.push(vertex_offset);
                indices.push(vertex_offset + 1);
                indices.push(vertex_offset + 2);
                indices.push(vertex_offset);
                indices.push(vertex_offset + 2);
                indices.push(vertex_offset + 3);

                vertex_offset += 4;
            }
        }

        let selection_index_count = indices.len() - selection_start_index;
        self.selection_range = QuadRange::new(selection_start_index, selection_index_count);

        // ==================== Phase 2: Glyph Quads ====================
        let glyph_start_index = indices.len();

        for buffer_line in visible_range.clone() {
            let screen_row = buffer_line - viewport.first_visible_line();
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

                // Generate the quad vertices with y_offset for smooth scrolling
                let quad = self.layout.quad_vertices_with_offset(screen_row, col, glyph, y_offset);
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

        let glyph_index_count = indices.len() - glyph_start_index;
        self.glyph_range = QuadRange::new(glyph_start_index, glyph_index_count);

        // ==================== Phase 3: Cursor Quad ====================
        let cursor_start_index = indices.len();

        if cursor_visible {
            let cursor_pos = buffer.cursor_position();
            if let Some(screen_line) = viewport.buffer_line_to_screen_line(cursor_pos.line) {
                // Render cursor as a block cursor using the solid (fully opaque)
                // atlas region so the fragment shader produces a visible quad.
                let solid_glyph = atlas.solid_glyph();
                let cursor_quad = self.create_cursor_quad_with_offset(
                    screen_line,
                    cursor_pos.col,
                    solid_glyph,
                    y_offset,
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

        let cursor_index_count = indices.len() - cursor_start_index;
        self.cursor_range = QuadRange::new(cursor_start_index, cursor_index_count);

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

    /// Creates a selection highlight quad covering columns [start_col, end_col) on the given row
    ///
    /// The quad uses the solid glyph from the atlas so the fragment shader produces
    /// a fully opaque result (the selection color alpha provides transparency).
    fn create_selection_quad(
        &self,
        screen_row: usize,
        start_col: usize,
        end_col: usize,
        solid_glyph: &GlyphInfo,
    ) -> [GlyphVertex; 4] {
        self.create_selection_quad_with_offset(screen_row, start_col, end_col, solid_glyph, 0.0)
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Selection quad with Y offset for smooth scrolling
    /// Creates a selection highlight quad with Y offset for smooth scrolling
    fn create_selection_quad_with_offset(
        &self,
        screen_row: usize,
        start_col: usize,
        end_col: usize,
        solid_glyph: &GlyphInfo,
        y_offset: f32,
    ) -> [GlyphVertex; 4] {
        let (start_x, y) = self.layout.position_for_with_offset(screen_row, start_col, y_offset);
        let (end_x, _) = self.layout.position_for_with_offset(screen_row, end_col, y_offset);

        // Selection height matches the line height
        let selection_height = self.layout.line_height;

        // Use the solid glyph's UVs (guaranteed to be opaque white)
        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(start_x, y, u0, v0),                       // top-left
            GlyphVertex::new(end_x, y, u1, v0),                         // top-right
            GlyphVertex::new(end_x, y + selection_height, u1, v1),      // bottom-right
            GlyphVertex::new(start_x, y + selection_height, u0, v1),    // bottom-left
        ]
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
        self.create_cursor_quad_with_offset(screen_row, col, reference_glyph, 0.0)
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Cursor quad with Y offset for smooth scrolling
    /// Creates a cursor quad at the specified screen position with Y offset
    fn create_cursor_quad_with_offset(
        &self,
        screen_row: usize,
        col: usize,
        reference_glyph: &GlyphInfo,
        y_offset: f32,
    ) -> [GlyphVertex; 4] {
        let (x, y) = self.layout.position_for_with_offset(screen_row, col, y_offset);

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

    // ==================== Selection Quad Tests ====================

    fn test_solid_glyph() -> GlyphInfo {
        GlyphInfo {
            uv_min: (0.5, 0.5),
            uv_max: (0.6, 0.6),
            width: 10.0,
            height: 18.0,
            bearing_x: 0.0,
            bearing_y: 0.0,
        }
    }

    #[test]
    fn test_selection_quad_single_char() {
        let glyph_buffer = GlyphBuffer::new(&test_metrics());
        let solid = test_solid_glyph();

        // Selection covering one character at row 0, col 0
        let quad = glyph_buffer.create_selection_quad(0, 0, 1, &solid);

        // Selection should span from x=0 to x=8 (one glyph width)
        assert_eq!(quad[0].position, [0.0, 0.0]);    // top-left
        assert_eq!(quad[1].position, [8.0, 0.0]);    // top-right (1 * 8)
        assert_eq!(quad[2].position, [8.0, 16.0]);   // bottom-right
        assert_eq!(quad[3].position, [0.0, 16.0]);   // bottom-left
    }

    #[test]
    fn test_selection_quad_multiple_chars() {
        let glyph_buffer = GlyphBuffer::new(&test_metrics());
        let solid = test_solid_glyph();

        // Selection covering cols 2-5 (3 characters) on row 1
        let quad = glyph_buffer.create_selection_quad(1, 2, 5, &solid);

        // x: col 2 = 16, col 5 = 40
        // y: row 1 = 16
        assert_eq!(quad[0].position, [16.0, 16.0]);  // top-left
        assert_eq!(quad[1].position, [40.0, 16.0]);  // top-right
        assert_eq!(quad[2].position, [40.0, 32.0]);  // bottom-right (y + line_height)
        assert_eq!(quad[3].position, [16.0, 32.0]);  // bottom-left
    }

    #[test]
    fn test_selection_quad_uses_solid_glyph_uvs() {
        let glyph_buffer = GlyphBuffer::new(&test_metrics());
        let solid = test_solid_glyph();

        let quad = glyph_buffer.create_selection_quad(0, 0, 3, &solid);

        // UVs should be from the solid glyph
        assert_eq!(quad[0].uv, [0.5, 0.5]);  // top-left
        assert_eq!(quad[1].uv, [0.6, 0.5]);  // top-right
        assert_eq!(quad[2].uv, [0.6, 0.6]);  // bottom-right
        assert_eq!(quad[3].uv, [0.5, 0.6]);  // bottom-left
    }

    #[test]
    fn test_selection_quad_height_matches_line_height() {
        let glyph_buffer = GlyphBuffer::new(&test_metrics());
        let solid = test_solid_glyph();

        let quad = glyph_buffer.create_selection_quad(0, 0, 1, &solid);

        // Height should be line_height (16.0), not glyph height
        let height = quad[2].position[1] - quad[0].position[1];
        assert_eq!(height, 16.0);
    }

    #[test]
    fn test_quad_range_default() {
        let range = QuadRange::default();
        assert_eq!(range.start, 0);
        assert_eq!(range.count, 0);
        assert!(range.is_empty());
    }

    #[test]
    fn test_quad_range_non_empty() {
        let range = QuadRange::new(10, 24);
        assert_eq!(range.start, 10);
        assert_eq!(range.count, 24);
        assert!(!range.is_empty());
    }
}
