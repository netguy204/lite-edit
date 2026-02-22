// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/text_selection_rendering - Selection highlight rendering
// Chunk: docs/chunks/line_wrap_rendering - Soft line wrapping support
// Chunk: docs/chunks/workspace_model - Content area x offset for left rail
// Chunk: docs/chunks/terminal_background_box_drawing - On-demand glyph addition for terminal rendering
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
//! With soft line wrapping enabled, long lines are split across multiple screen rows.
//! The `WrapLayout` struct handles the coordinate mapping between buffer columns and
//! screen positions.
//!
//! ## Quad Categories
//!
//! The buffer emits quads in a specific order:
//! 1. **Selection quads** - Semi-transparent background highlights for selected text
//! 2. **Border quads** - Left-edge indicators for continuation rows (wrapped lines)
//! 3. **Glyph quads** - The actual text characters
//! 4. **Cursor quad** - The block cursor at the current position
//!
//! Each category has its own index range tracked separately, allowing the renderer
//! to draw each with different colors via separate draw calls.

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::color_palette::ColorPalette;
use crate::font::{Font, FontMetrics};
use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::shader::VERTEX_SIZE;
use crate::viewport::Viewport;
use crate::wrap_layout::WrapLayout;
// Chunk: docs/chunks/buffer_view_trait - Use BufferView trait instead of TextBuffer
// Chunk: docs/chunks/renderer_styled_content - Use Style types for per-span colors
use lite_edit_buffer::{BufferView, CursorShape, UnderlineStyle};

// =============================================================================
// Vertex Data
// =============================================================================

/// A single vertex in a glyph quad
// Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GlyphVertex {
    /// Position in screen coordinates (pixels)
    pub position: [f32; 2],
    /// Texture UV coordinates (normalized 0-1)
    pub uv: [f32; 2],
    /// Per-vertex RGBA color
    pub color: [f32; 4],
}

impl GlyphVertex {
    pub fn new(x: f32, y: f32, u: f32, v: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            uv: [u, v],
            color,
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

    // Chunk: docs/chunks/workspace_model - Position with X and Y offset for left rail
    /// Calculates the screen position for a character at (row, col) with both X and Y offset
    ///
    /// Returns (x, y) where:
    /// - X coordinate is shifted by `x_offset` pixels (for left rail offset)
    /// - Y coordinate is shifted by `-y_offset` pixels (for smooth scrolling)
    pub fn position_for_with_xy_offset(&self, row: usize, col: usize, x_offset: f32, y_offset: f32) -> (f32, f32) {
        let x = col as f32 * self.glyph_width + x_offset;
        let y = row as f32 * self.line_height - y_offset;
        (x, y)
    }

    /// Generates the four vertices for a glyph quad at (row, col)
    ///
    /// The quad covers the glyph cell with the given UV coordinates.
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    pub fn quad_vertices(
        &self,
        row: usize,
        col: usize,
        glyph: &GlyphInfo,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        self.quad_vertices_with_offset(row, col, glyph, 0.0, color)
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Quad vertices with Y offset for smooth scrolling
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
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
        color: [f32; 4],
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
            GlyphVertex::new(x, y, u0, v0, color),         // top-left
            GlyphVertex::new(x + w, y, u1, v0, color),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1, color),     // bottom-left
        ]
    }

    // Chunk: docs/chunks/workspace_model - Quad vertices with X and Y offset for left rail
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    /// Generates the four vertices for a glyph quad at (row, col) with X and Y offset
    ///
    /// The X coordinate is shifted by `x_offset` pixels (e.g., for left rail offset).
    /// The Y coordinate is shifted by `-y_offset` pixels for smooth scrolling.
    pub fn quad_vertices_with_xy_offset(
        &self,
        row: usize,
        col: usize,
        glyph: &GlyphInfo,
        x_offset: f32,
        y_offset: f32,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        let (x, y) = self.position_for_with_xy_offset(row, col, x_offset, y_offset);

        // Quad dimensions match the glyph cell size
        let w = glyph.width;
        let h = glyph.height;

        // UV coordinates from the glyph info
        let (u0, v0) = glyph.uv_min;
        let (u1, v1) = glyph.uv_max;

        // Four corners: top-left, top-right, bottom-right, bottom-left
        [
            GlyphVertex::new(x, y, u0, v0, color),         // top-left
            GlyphVertex::new(x + w, y, u1, v0, color),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1, color),     // bottom-left
        ]
    }
}

// =============================================================================
// Glyph Buffer
// =============================================================================

// Chunk: docs/chunks/text_selection_rendering - Index range tracking for selection/glyph/cursor quad categories
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
// Chunk: docs/chunks/renderer_styled_content - Extended with background and underline ranges
pub struct GlyphBuffer {
    /// The vertex buffer containing glyph quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Number of indices to draw
    index_count: usize,
    /// Layout calculator
    layout: GlyphLayout,
    /// Color palette for resolving Style colors to RGBA
    palette: ColorPalette,
    /// Index range for background (per-span bg color) quads
    background_range: QuadRange,
    /// Index range for selection highlight quads
    selection_range: QuadRange,
    /// Index range for continuation row border quads
    border_range: QuadRange,
    /// Index range for glyph (text character) quads
    glyph_range: QuadRange,
    /// Index range for underline quads
    underline_range: QuadRange,
    /// Index range for cursor quad
    cursor_range: QuadRange,
    // Chunk: docs/chunks/workspace_model - Content area x offset for left rail
    /// Horizontal offset for content area (e.g., for left rail)
    x_offset: f32,
    // Chunk: docs/chunks/content_tab_bar - Content area y offset for tab bar
    /// Vertical offset for content area (e.g., for tab bar)
    y_offset: f32,
}

impl GlyphBuffer {
    /// Creates a new empty glyph buffer
    // Chunk: docs/chunks/renderer_styled_content - ColorPalette for styled text
    pub fn new(metrics: &FontMetrics) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout: GlyphLayout::from_metrics(metrics),
            palette: ColorPalette::default(),
            background_range: QuadRange::default(),
            selection_range: QuadRange::default(),
            border_range: QuadRange::default(),
            glyph_range: QuadRange::default(),
            underline_range: QuadRange::default(),
            cursor_range: QuadRange::default(),
            x_offset: 0.0,
            y_offset: 0.0,
        }
    }

    // Chunk: docs/chunks/workspace_model - Content area x offset for left rail
    /// Sets the horizontal offset for content area rendering.
    ///
    /// When set to a positive value (e.g., RAIL_WIDTH), all glyphs are shifted
    /// right by that amount to make room for the left rail.
    pub fn set_x_offset(&mut self, offset: f32) {
        self.x_offset = offset;
    }

    /// Returns the current horizontal offset
    pub fn x_offset(&self) -> f32 {
        self.x_offset
    }

    // Chunk: docs/chunks/content_tab_bar - Content area y offset for tab bar
    /// Sets the vertical offset for content area rendering.
    ///
    /// When set to a positive value (e.g., TAB_BAR_HEIGHT), all glyphs are shifted
    /// down by that amount to make room for the tab bar at the top.
    pub fn set_y_offset(&mut self, offset: f32) {
        self.y_offset = offset;
    }

    /// Returns the current vertical offset
    pub fn y_offset(&self) -> f32 {
        self.y_offset
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

    /// Returns the index range for background (per-span bg color) quads
    // Chunk: docs/chunks/renderer_styled_content - Background quads for styled text
    pub fn background_range(&self) -> QuadRange {
        self.background_range
    }

    /// Returns the index range for selection highlight quads
    pub fn selection_range(&self) -> QuadRange {
        self.selection_range
    }

    // Chunk: docs/chunks/line_wrap_rendering - Continuation row border quad range
    /// Returns the index range for continuation row border quads
    pub fn border_range(&self) -> QuadRange {
        self.border_range
    }

    /// Returns the index range for glyph (text character) quads
    pub fn glyph_range(&self) -> QuadRange {
        self.glyph_range
    }

    /// Returns the index range for underline quads
    // Chunk: docs/chunks/renderer_styled_content - Underline rendering for styled text
    pub fn underline_range(&self) -> QuadRange {
        self.underline_range
    }

    /// Returns the index range for the cursor quad
    pub fn cursor_range(&self) -> QuadRange {
        self.cursor_range
    }

    /// Updates the buffers with new text content
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings (mutable for on-demand glyph addition)
    /// * `font` - The font for on-demand glyph rasterization
    /// * `lines` - The text lines to render
    // Chunk: docs/chunks/renderer_styled_content - Uses default text color
    // Chunk: docs/chunks/terminal_background_box_drawing - Mutable atlas for on-demand glyph addition
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &mut GlyphAtlas,
        font: &Font,
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

        // Use default text color for this simple API
        let text_color = self.palette.default_foreground();

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

                // Get the glyph info from the atlas (adding on-demand if needed)
                // Chunk: docs/chunks/terminal_background_box_drawing - On-demand glyph addition
                let glyph = match atlas.ensure_glyph(font, c) {
                    Some(g) => g,
                    None => {
                        // Character not in atlas and can't be added, skip it
                        continue;
                    }
                };

                // Generate the quad vertices
                let quad = self.layout.quad_vertices(row, col, glyph, text_color);
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

    /// Updates the buffers with content from a BufferView, rendering only visible lines
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings (mutable for on-demand glyph addition)
    /// * `font` - The font for on-demand glyph rasterization
    /// * `view` - The buffer view to render from
    /// * `viewport` - The viewport defining which lines are visible
    // Chunk: docs/chunks/buffer_view_trait - Accept BufferView trait instead of TextBuffer
    // Chunk: docs/chunks/terminal_background_box_drawing - Mutable atlas for on-demand glyph addition
    pub fn update_from_buffer(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &mut GlyphAtlas,
        font: &Font,
        view: &dyn BufferView,
        viewport: &Viewport,
    ) {
        self.update_from_buffer_with_cursor(device, atlas, font, view, viewport, true, 0.0);
    }

    /// Updates the buffers with content from a BufferView, including cursor and selection rendering
    ///
    /// Emits quads in this order:
    /// 1. Background quads (per-span bg colors)
    /// 2. Selection highlight quads
    /// 3. Glyph quads (text characters with per-span fg colors)
    /// 4. Underline quads (for underlined spans)
    /// 5. Cursor quad (drawn last, on top)
    ///
    /// Each category's index range is tracked separately. With per-vertex colors,
    /// all quads are drawn in a single pass with no uniform changes.
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas containing character UV mappings (mutable for on-demand glyph addition)
    /// * `font` - The font for on-demand glyph rasterization
    /// * `view` - The buffer view to render from
    /// * `viewport` - The viewport defining which lines are visible
    /// * `cursor_visible` - Whether to render the cursor (for future blink support)
    /// * `y_offset` - Vertical offset in pixels for smooth scrolling. When scrolled to a
    ///   fractional position (e.g., 2.5 lines), pass the fractional remainder (e.g., 0.5 * line_height)
    ///   to shift all content up, causing the top line to be partially clipped.
    // Chunk: docs/chunks/viewport_fractional_scroll - Y offset parameter for smooth scrolling
    // Chunk: docs/chunks/buffer_view_trait - Accept BufferView trait instead of TextBuffer
    // Chunk: docs/chunks/text_selection_rendering - Three-phase quad emission: selection -> glyphs -> cursor with per-category index ranges
    // Chunk: docs/chunks/renderer_styled_content - Per-span colors, backgrounds, underlines, cursor shapes
    // Chunk: docs/chunks/terminal_background_box_drawing - Mutable atlas for on-demand glyph addition
    pub fn update_from_buffer_with_cursor(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &mut GlyphAtlas,
        font: &Font,
        view: &dyn BufferView,
        viewport: &Viewport,
        cursor_visible: bool,
        y_offset: f32,
    ) {
        let visible_range = viewport.visible_range(view.line_count());

        // Estimate character count for buffer sizing
        // Add extra for background quads, selection quads, underline quads, and cursor
        let mut estimated_chars: usize = 0;
        let mut estimated_spans: usize = 0;
        for line in visible_range.clone() {
            if let Some(styled_line) = view.styled_line(line) {
                estimated_chars += styled_line.char_count();
                estimated_spans += styled_line.spans.len();
            }
        }
        let selection_lines = visible_range.len();
        let cursor_quads = if cursor_visible { 1 } else { 0 };
        // Background quads: one per span with non-default bg
        // Underline quads: one per span with underline
        // Plus glyphs, selection, cursor
        let total_estimated = estimated_chars + estimated_spans * 2 + selection_lines + cursor_quads;

        // Reset quad ranges
        self.background_range = QuadRange::default();
        self.selection_range = QuadRange::default();
        self.glyph_range = QuadRange::default();
        self.underline_range = QuadRange::default();
        self.cursor_range = QuadRange::default();

        if estimated_chars == 0 && cursor_quads == 0 && view.selection_range().is_none() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Allocate vertex and index data
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(total_estimated * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(total_estimated * 6);
        let mut vertex_offset: u32 = 0;

        // Copy the solid glyph info to avoid borrowing atlas during later mutable operations
        // Chunk: docs/chunks/terminal_background_box_drawing - Copy solid glyph to avoid borrow conflict
        let solid_glyph = *atlas.solid_glyph();

        // Selection color (Catppuccin Mocha surface2 at 40% alpha)
        let selection_color: [f32; 4] = [0.345, 0.357, 0.439, 0.4];
        // Cursor color (same as default text color)
        let cursor_color = self.palette.default_foreground();

        // ==================== Phase 1: Background Quads ====================
        // Chunk: docs/chunks/renderer_styled_content - Background quads for per-span bg colors
        let background_start_index = indices.len();

        for buffer_line in visible_range.clone() {
            let screen_row = buffer_line - viewport.first_visible_line();

            if let Some(styled_line) = view.styled_line(buffer_line) {
                let mut col: usize = 0;
                for span in &styled_line.spans {
                    let span_len = span.text.chars().count();
                    let end_col = col + span_len;

                    // Only emit background quad if bg is not default
                    if !self.palette.is_default_background(span.style.bg) {
                        let (_, bg) = self.palette.resolve_style_colors(&span.style);
                        let quad = self.create_selection_quad_with_offset(
                            screen_row, col, end_col, &solid_glyph, y_offset, bg
                        );
                        vertices.extend_from_slice(&quad);

                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 1);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset + 3);

                        vertex_offset += 4;
                    }

                    col = end_col;
                }
            }
        }

        let background_index_count = indices.len() - background_start_index;
        self.background_range = QuadRange::new(background_start_index, background_index_count);

        // ==================== Phase 2: Selection Quads ====================
        let selection_start_index = indices.len();

        if let Some((sel_start, sel_end)) = view.selection_range() {
            for buffer_line in visible_range.clone() {
                if buffer_line < sel_start.line || buffer_line > sel_end.line {
                    continue;
                }

                let screen_row = buffer_line - viewport.first_visible_line();
                let line_len = view.line_len(buffer_line);

                let start_col = if buffer_line == sel_start.line {
                    sel_start.col
                } else {
                    0
                };
                let end_col = if buffer_line == sel_end.line {
                    sel_end.col
                } else {
                    line_len + 1
                };

                if start_col >= end_col {
                    continue;
                }

                let quad = self.create_selection_quad_with_offset(
                    screen_row, start_col, end_col, &solid_glyph, y_offset, selection_color
                );
                vertices.extend_from_slice(&quad);

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

        // ==================== Phase 3: Glyph Quads ====================
        // Chunk: docs/chunks/renderer_styled_content - Per-span foreground colors
        let glyph_start_index = indices.len();

        for buffer_line in visible_range.clone() {
            let screen_row = buffer_line - viewport.first_visible_line();

            if let Some(styled_line) = view.styled_line(buffer_line) {
                let mut col: usize = 0;
                for span in &styled_line.spans {
                    // Skip hidden text
                    if span.style.hidden {
                        col += span.text.chars().count();
                        continue;
                    }

                    // Resolve foreground color for this span
                    let (fg, _) = self.palette.resolve_style_colors(&span.style);

                    for c in span.text.chars() {
                        // Skip spaces (they don't need quads)
                        if c == ' ' {
                            col += 1;
                            continue;
                        }

                        // Get the glyph info from the atlas (adding on-demand if needed)
                        // Chunk: docs/chunks/terminal_background_box_drawing - On-demand glyph addition
                        let glyph = match atlas.ensure_glyph(font, c) {
                            Some(g) => g,
                            None => {
                                col += 1;
                                continue;
                            }
                        };

                        // Generate the quad vertices with per-span fg color
                        // Chunk: docs/chunks/tab_bar_layout_fixes - Apply both x_offset and y_offset
                        // Previously used quad_vertices_with_offset which only applied y_offset,
                        // causing glyphs to render without the left rail offset (x_offset).
                        let effective_y_offset = y_offset - self.y_offset;
                        let quad = self.layout.quad_vertices_with_xy_offset(screen_row, col, glyph, self.x_offset, effective_y_offset, fg);
                        vertices.extend_from_slice(&quad);

                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 1);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset + 3);

                        vertex_offset += 4;
                        col += 1;
                    }
                }
            }
        }

        let glyph_index_count = indices.len() - glyph_start_index;
        self.glyph_range = QuadRange::new(glyph_start_index, glyph_index_count);

        // ==================== Phase 4: Underline Quads ====================
        // Chunk: docs/chunks/renderer_styled_content - Underline rendering
        let underline_start_index = indices.len();

        for buffer_line in visible_range.clone() {
            let screen_row = buffer_line - viewport.first_visible_line();

            if let Some(styled_line) = view.styled_line(buffer_line) {
                let mut col: usize = 0;
                for span in &styled_line.spans {
                    let span_len = span.text.chars().count();
                    let end_col = col + span_len;

                    // Emit underline quad if underline style is not None
                    if span.style.underline != UnderlineStyle::None {
                        // Use underline_color if specified, otherwise use fg color
                        let underline_color = if let Some(uc) = span.style.underline_color {
                            self.palette.resolve_color(uc, true)
                        } else {
                            let (fg, _) = self.palette.resolve_style_colors(&span.style);
                            fg
                        };

                        let quad = self.create_underline_quad(
                            screen_row, col, end_col, &solid_glyph, y_offset, underline_color
                        );
                        vertices.extend_from_slice(&quad);

                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 1);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset + 3);

                        vertex_offset += 4;
                    }

                    col = end_col;
                }
            }
        }

        let underline_index_count = indices.len() - underline_start_index;
        self.underline_range = QuadRange::new(underline_start_index, underline_index_count);

        // ==================== Phase 5: Cursor Quad ====================
        // Chunk: docs/chunks/renderer_styled_content - Cursor shape rendering
        let cursor_start_index = indices.len();

        if cursor_visible {
            if let Some(cursor_info) = view.cursor_info() {
                // Skip if cursor is hidden
                if cursor_info.shape != CursorShape::Hidden {
                    let cursor_pos = cursor_info.position;
                    if let Some(screen_line) = viewport.buffer_line_to_screen_line(cursor_pos.line) {
                        let cursor_quad = self.create_cursor_quad_for_shape(
                            screen_line,
                            cursor_pos.col,
                            cursor_info.shape,
                            &solid_glyph,
                            y_offset,
                            cursor_color,
                        );
                        vertices.extend_from_slice(&cursor_quad);

                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 1);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset);
                        indices.push(vertex_offset + 2);
                        indices.push(vertex_offset + 3);
                    }
                }
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

    /// Creates an underline quad at the baseline of the given row
    // Chunk: docs/chunks/renderer_styled_content - Underline rendering
    fn create_underline_quad(
        &self,
        screen_row: usize,
        start_col: usize,
        end_col: usize,
        solid_glyph: &GlyphInfo,
        y_offset: f32,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        // Chunk: docs/chunks/content_tab_bar - Add y_offset for tab bar
        let effective_y_offset = y_offset - self.y_offset;
        let (start_x, y) = self.layout.position_for_with_offset(screen_row, start_col, effective_y_offset);
        let (end_x, _) = self.layout.position_for_with_offset(screen_row, end_col, effective_y_offset);

        // Underline position: at the bottom of the line cell, 2 pixels thick
        let underline_y = y + self.layout.line_height - 2.0;
        let underline_height = 1.0; // Single pixel underline

        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(start_x, underline_y, u0, v0, color),
            GlyphVertex::new(end_x, underline_y, u1, v0, color),
            GlyphVertex::new(end_x, underline_y + underline_height, u1, v1, color),
            GlyphVertex::new(start_x, underline_y + underline_height, u0, v1, color),
        ]
    }

    /// Creates a cursor quad with the appropriate shape
    // Chunk: docs/chunks/renderer_styled_content - Cursor shape rendering
    fn create_cursor_quad_for_shape(
        &self,
        screen_row: usize,
        col: usize,
        shape: CursorShape,
        solid_glyph: &GlyphInfo,
        y_offset: f32,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        // Chunk: docs/chunks/content_tab_bar - Add y_offset for tab bar
        let effective_y_offset = y_offset - self.y_offset;
        let (x, y) = self.layout.position_for_with_offset(screen_row, col, effective_y_offset);
        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        match shape {
            CursorShape::Block => {
                // Full cell block cursor
                let w = self.layout.glyph_width;
                let h = self.layout.line_height;
                [
                    GlyphVertex::new(x, y, u0, v0, color),
                    GlyphVertex::new(x + w, y, u1, v0, color),
                    GlyphVertex::new(x + w, y + h, u1, v1, color),
                    GlyphVertex::new(x, y + h, u0, v1, color),
                ]
            }
            CursorShape::Beam => {
                // Thin vertical bar at left edge of cell
                let w = 2.0; // 2 pixels wide
                let h = self.layout.line_height;
                [
                    GlyphVertex::new(x, y, u0, v0, color),
                    GlyphVertex::new(x + w, y, u1, v0, color),
                    GlyphVertex::new(x + w, y + h, u1, v1, color),
                    GlyphVertex::new(x, y + h, u0, v1, color),
                ]
            }
            CursorShape::Underline => {
                // Thin horizontal bar at bottom of cell
                let w = self.layout.glyph_width;
                let h = 2.0; // 2 pixels tall
                let underline_y = y + self.layout.line_height - h;
                [
                    GlyphVertex::new(x, underline_y, u0, v0, color),
                    GlyphVertex::new(x + w, underline_y, u1, v0, color),
                    GlyphVertex::new(x + w, underline_y + h, u1, v1, color),
                    GlyphVertex::new(x, underline_y + h, u0, v1, color),
                ]
            }
            CursorShape::Hidden => {
                // Return degenerate quad (zero area) - shouldn't be reached
                [
                    GlyphVertex::new(x, y, u0, v0, color),
                    GlyphVertex::new(x, y, u1, v0, color),
                    GlyphVertex::new(x, y, u1, v1, color),
                    GlyphVertex::new(x, y, u0, v1, color),
                ]
            }
        }
    }

    // Chunk: docs/chunks/text_selection_rendering - Selection highlight quad geometry covering selected columns on a row
    /// Creates a selection highlight quad covering columns [start_col, end_col) on the given row
    ///
    /// The quad uses the solid glyph from the atlas so the fragment shader produces
    /// a fully opaque result (the selection color alpha provides transparency).
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    fn create_selection_quad(
        &self,
        screen_row: usize,
        start_col: usize,
        end_col: usize,
        solid_glyph: &GlyphInfo,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        self.create_selection_quad_with_offset(screen_row, start_col, end_col, solid_glyph, 0.0, color)
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Selection quad with Y offset for smooth scrolling
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    // Chunk: docs/chunks/workspace_model - Uses self.x_offset for left rail offset
    /// Creates a selection highlight quad with Y offset for smooth scrolling
    fn create_selection_quad_with_offset(
        &self,
        screen_row: usize,
        start_col: usize,
        end_col: usize,
        solid_glyph: &GlyphInfo,
        y_offset: f32,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        // Chunk: docs/chunks/content_tab_bar - Add y_offset for tab bar
        let effective_y_offset = y_offset - self.y_offset;
        let (start_x, y) = self.layout.position_for_with_xy_offset(screen_row, start_col, self.x_offset, effective_y_offset);
        let (end_x, _) = self.layout.position_for_with_xy_offset(screen_row, end_col, self.x_offset, effective_y_offset);

        // Selection height matches the line height
        let selection_height = self.layout.line_height;

        // Use the solid glyph's UVs (guaranteed to be opaque white)
        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(start_x, y, u0, v0, color),                       // top-left
            GlyphVertex::new(end_x, y, u1, v0, color),                         // top-right
            GlyphVertex::new(end_x, y + selection_height, u1, v1, color),      // bottom-right
            GlyphVertex::new(start_x, y + selection_height, u0, v1, color),    // bottom-left
        ]
    }

    /// Creates a cursor quad at the specified screen position
    ///
    /// The cursor is rendered as a solid block that uses a portion of the atlas
    /// that is guaranteed to be opaque (we use the same technique as text but
    /// the shader will render it with a different color).
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    fn create_cursor_quad(
        &self,
        screen_row: usize,
        col: usize,
        reference_glyph: &GlyphInfo,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        self.create_cursor_quad_with_offset(screen_row, col, reference_glyph, 0.0, color)
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Cursor quad with Y offset for smooth scrolling
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    // Chunk: docs/chunks/workspace_model - Uses self.x_offset for left rail offset
    /// Creates a cursor quad at the specified screen position with Y offset
    fn create_cursor_quad_with_offset(
        &self,
        screen_row: usize,
        col: usize,
        reference_glyph: &GlyphInfo,
        y_offset: f32,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        // Chunk: docs/chunks/content_tab_bar - Add y_offset for tab bar
        let effective_y_offset = y_offset - self.y_offset;
        let (x, y) = self.layout.position_for_with_xy_offset(screen_row, col, self.x_offset, effective_y_offset);

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
            GlyphVertex::new(x, y, u0, v0, color),                             // top-left
            GlyphVertex::new(x + cursor_width, y, u1, v0, color),              // top-right
            GlyphVertex::new(x + cursor_width, y + cursor_height, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + cursor_height, u0, v1, color),             // bottom-left
        ]
    }

    // Chunk: docs/chunks/line_wrap_rendering - Continuation row border indicator
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    // Chunk: docs/chunks/workspace_model - Uses self.x_offset for left rail offset
    /// Creates a left-edge border quad for a continuation row.
    ///
    /// The border is 2 pixels wide and spans the full line height, positioned
    /// at the leftmost edge of the content area (offset by x_offset to account
    /// for the left rail).
    fn create_border_quad(
        &self,
        screen_row: usize,
        solid_glyph: &GlyphInfo,
        y_offset: f32,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        // Chunk: docs/chunks/content_tab_bar - Add y_offset for tab bar
        let y = screen_row as f32 * self.layout.line_height - y_offset + self.y_offset;
        let x = self.x_offset; // Start at the left edge of the content area

        // Border is 2 pixels wide at the left edge
        let border_width = 2.0;
        let border_height = self.layout.line_height;

        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(x, y, u0, v0, color),                             // top-left
            GlyphVertex::new(x + border_width, y, u1, v0, color),              // top-right
            GlyphVertex::new(x + border_width, y + border_height, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + border_height, u0, v1, color),             // bottom-left
        ]
    }

    // Chunk: docs/chunks/line_wrap_rendering - Wrap-aware rendering
    // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Fixed coordinate space alignment
    /// Updates the buffers with content from a BufferView, with soft line wrapping.
    ///
    /// This is the main rendering entry point when line wrapping is enabled.
    /// It iterates buffer lines, wrapping each according to the WrapLayout,
    /// and emits quads for each screen row until the viewport is filled.
    ///
    /// **Coordinate spaces**: When wrapping is enabled, `scroll_offset_px` is in
    /// screen row space (set by `ensure_visible_wrapped`). This method converts
    /// that to the correct buffer line starting point using `buffer_line_for_screen_row`.
    ///
    /// Emits quads in this order:
    /// 1. Selection highlight quads
    /// 2. Border quads (for continuation rows)
    /// 3. Glyph quads (text characters)
    /// 4. Cursor quad
    // Chunk: docs/chunks/buffer_view_trait - Accept BufferView trait instead of TextBuffer
    // Chunk: docs/chunks/terminal_background_box_drawing - Mutable atlas for on-demand glyph addition
    // Chunk: docs/chunks/terminal_styling_fidelity - Per-span foreground colors, background quads, and underline quads in wrapped rendering path
    pub fn update_from_buffer_with_wrap(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &mut GlyphAtlas,
        font: &Font,
        view: &dyn BufferView,
        viewport: &Viewport,
        wrap_layout: &WrapLayout,
        cursor_visible: bool,
        y_offset: f32,
    ) {
        let line_count = view.line_count();
        let max_screen_rows = viewport.visible_lines() + 2; // +2 for partial visibility at top/bottom

        // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Correct coordinate space conversion
        // In wrapped mode, scroll_offset_px is in screen row space.
        // Convert the first visible screen row to the corresponding buffer line.
        let first_visible_screen_row = viewport.first_visible_screen_row();
        let (first_visible_buffer_line, screen_row_offset_in_line, _) =
            Viewport::buffer_line_for_screen_row(
                first_visible_screen_row,
                line_count,
                wrap_layout,
                |line| view.line_len(line),
            );

        // Estimate buffer sizes
        // We don't know exact counts without iterating, but we can estimate
        let mut estimated_quads = 0;
        let mut screen_row = 0;

        // Quick estimate: count chars in visible lines
        for buffer_line in first_visible_buffer_line..line_count {
            if screen_row >= max_screen_rows {
                break;
            }
            let line_len = view.line_len(buffer_line);
            let rows_for_line = wrap_layout.screen_rows_for_line(line_len);
            estimated_quads += line_len + rows_for_line; // chars + potential borders
            screen_row += rows_for_line;
        }
        estimated_quads += 1; // cursor

        // Reset quad ranges
        // Chunk: docs/chunks/terminal_styling_fidelity - Added background and underline ranges
        self.background_range = QuadRange::default();
        self.selection_range = QuadRange::default();
        self.border_range = QuadRange::default();
        self.glyph_range = QuadRange::default();
        self.underline_range = QuadRange::default();
        self.cursor_range = QuadRange::default();

        // Define colors for this rendering pass
        // Selection color (Catppuccin Mocha surface2 at 40% alpha)
        let selection_color: [f32; 4] = [0.345, 0.357, 0.439, 0.4];
        // Cursor color (same as default text color)
        let cursor_color = self.palette.default_foreground();
        // Border color for continuation lines (dimmed foreground)
        let border_color: [f32; 4] = [0.4, 0.4, 0.45, 0.6];

        if estimated_quads == 0 && !cursor_visible {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Allocate vertex and index data
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_quads * 6);
        let mut vertex_offset: u32 = 0;

        // Helper to push quad indices
        let push_quad_indices = |indices: &mut Vec<u32>, vertex_offset: &mut u32| {
            indices.push(*vertex_offset);
            indices.push(*vertex_offset + 1);
            indices.push(*vertex_offset + 2);
            indices.push(*vertex_offset);
            indices.push(*vertex_offset + 2);
            indices.push(*vertex_offset + 3);
            *vertex_offset += 4;
        };

        // ==================== Phase 1: Background Quads ====================
        // Chunk: docs/chunks/terminal_styling_fidelity - Per-span background colors for terminal styling
        let background_start_index = indices.len();

        {
            let solid_glyph = atlas.solid_glyph();
            let cols_per_row = wrap_layout.cols_per_row();
            let mut cumulative_screen_row: usize = 0;
            let mut is_first_buffer_line = true;

            for buffer_line in first_visible_buffer_line..line_count {
                if cumulative_screen_row >= max_screen_rows {
                    break;
                }

                let line_len = view.line_len(buffer_line);
                let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

                // Determine the starting row offset within this buffer line
                let start_row_offset = if is_first_buffer_line {
                    screen_row_offset_in_line
                } else {
                    0
                };
                is_first_buffer_line = false;

                // Iterate spans and emit background quads for non-default backgrounds
                if let Some(styled_line) = view.styled_line(buffer_line) {
                    let mut col: usize = 0;
                    for span in &styled_line.spans {
                        let span_len = span.text.chars().count();
                        let end_col = col + span_len;

                        // Only emit background quad if bg is not default
                        if !self.palette.is_default_background(span.style.bg) {
                            let (_, bg) = self.palette.resolve_style_colors(&span.style);

                            // The span may cross multiple screen rows due to wrapping.
                            // Emit a background quad for each screen row the span covers.
                            let span_start_row_offset = col / cols_per_row;
                            let span_end_row_offset = if end_col == 0 { 0 } else { (end_col - 1) / cols_per_row };

                            for row_offset in span_start_row_offset..=span_end_row_offset {
                                // Skip rows before our starting row
                                if row_offset < start_row_offset {
                                    continue;
                                }

                                let screen_row = cumulative_screen_row + (row_offset - start_row_offset);
                                if screen_row >= max_screen_rows {
                                    break;
                                }

                                // Calculate the portion of the span on this screen row
                                let row_start_col = row_offset * cols_per_row;
                                let row_end_col = (row_offset + 1) * cols_per_row;

                                let span_start_on_row = col.max(row_start_col);
                                let span_end_on_row = end_col.min(row_end_col);

                                if span_start_on_row < span_end_on_row {
                                    // Convert to screen columns (relative to this screen row)
                                    let screen_start_col = span_start_on_row - row_start_col;
                                    let screen_end_col = span_end_on_row - row_start_col;

                                    let quad = self.create_selection_quad_with_offset(
                                        screen_row,
                                        screen_start_col,
                                        screen_end_col,
                                        solid_glyph,
                                        y_offset,
                                        bg,
                                    );
                                    vertices.extend_from_slice(&quad);
                                    push_quad_indices(&mut indices, &mut vertex_offset);
                                }
                            }
                        }

                        col = end_col;
                    }
                }

                cumulative_screen_row += rows_for_line - start_row_offset;
            }
        }

        let background_index_count = indices.len() - background_start_index;
        self.background_range = QuadRange::new(background_start_index, background_index_count);

        // ==================== Phase 2: Selection Quads ====================
        // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Fixed screen row tracking
        let selection_start_index = indices.len();

        if let Some((sel_start, sel_end)) = view.selection_range() {
            let solid_glyph = atlas.solid_glyph();
            let cols_per_row = wrap_layout.cols_per_row();

            // Iterate buffer lines, tracking cumulative screen row.
            // cumulative_screen_row tracks the screen row relative to the viewport's top,
            // starting at 0 for the first visible screen row.
            //
            // For the first buffer line, we skip rows before screen_row_offset_in_line
            // since they're scrolled above the viewport.
            let mut cumulative_screen_row: usize = 0;
            let mut is_first_buffer_line = true;

            for buffer_line in first_visible_buffer_line..line_count {
                if cumulative_screen_row >= max_screen_rows {
                    break;
                }

                let line_len = view.line_len(buffer_line);
                let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

                // Determine the starting row offset within this buffer line
                let start_row_offset = if is_first_buffer_line {
                    screen_row_offset_in_line
                } else {
                    0
                };
                is_first_buffer_line = false;

                // Check if this buffer line intersects the selection
                if buffer_line >= sel_start.line && buffer_line <= sel_end.line {
                    // Calculate selection bounds for this buffer line
                    let line_sel_start = if buffer_line == sel_start.line {
                        sel_start.col
                    } else {
                        0
                    };
                    let line_sel_end = if buffer_line == sel_end.line {
                        sel_end.col
                    } else {
                        // Include newline character visualization
                        line_len + 1
                    };

                    if line_sel_start < line_sel_end {
                        // Emit selection quads for each screen row in this buffer line
                        for row_offset in start_row_offset..rows_for_line {
                            let screen_row = cumulative_screen_row + (row_offset - start_row_offset);
                            if screen_row >= max_screen_rows {
                                break;
                            }

                            // Calculate which buffer columns are on this screen row
                            let row_start_col = row_offset * cols_per_row;
                            let row_end_col = ((row_offset + 1) * cols_per_row).min(line_len + 1);

                            // Calculate intersection with selection
                            let sel_start_on_row = line_sel_start.max(row_start_col);
                            let sel_end_on_row = line_sel_end.min(row_end_col);

                            if sel_start_on_row < sel_end_on_row {
                                // Convert to screen columns (relative to this screen row)
                                let screen_start_col = sel_start_on_row - row_start_col;
                                let screen_end_col = sel_end_on_row - row_start_col;

                                let quad = self.create_selection_quad_with_offset(
                                    screen_row,
                                    screen_start_col,
                                    screen_end_col,
                                    solid_glyph,
                                    y_offset,
                                    selection_color,
                                );
                                vertices.extend_from_slice(&quad);
                                push_quad_indices(&mut indices, &mut vertex_offset);
                            }
                        }
                    }
                }

                // Add only the visible rows from this line to cumulative count
                cumulative_screen_row += rows_for_line - start_row_offset;
            }
        }

        let selection_index_count = indices.len() - selection_start_index;
        self.selection_range = QuadRange::new(selection_start_index, selection_index_count);

        // ==================== Phase 2: Border Quads ====================
        // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Fixed screen row tracking
        let border_start_index = indices.len();

        {
            let solid_glyph = atlas.solid_glyph();
            let mut cumulative_screen_row: usize = 0;
            let mut is_first_buffer_line = true;

            for buffer_line in first_visible_buffer_line..line_count {
                if cumulative_screen_row >= max_screen_rows {
                    break;
                }

                let line_len = view.line_len(buffer_line);
                let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

                // Determine the starting row offset within this buffer line
                let start_row_offset = if is_first_buffer_line {
                    screen_row_offset_in_line
                } else {
                    0
                };
                is_first_buffer_line = false;

                // Emit border quads for continuation rows (row_offset > 0)
                // Start from max(1, start_row_offset) since row 0 within a buffer line
                // is never a continuation row
                let border_start = start_row_offset.max(1);
                for row_offset in border_start..rows_for_line {
                    let screen_row = cumulative_screen_row + (row_offset - start_row_offset);
                    if screen_row >= max_screen_rows {
                        break;
                    }

                    let quad = self.create_border_quad(screen_row, solid_glyph, y_offset, border_color);
                    vertices.extend_from_slice(&quad);
                    push_quad_indices(&mut indices, &mut vertex_offset);
                }

                cumulative_screen_row += rows_for_line - start_row_offset;
            }
        }

        let border_index_count = indices.len() - border_start_index;
        self.border_range = QuadRange::new(border_start_index, border_index_count);

        // ==================== Phase 3: Glyph Quads ====================
        // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Fixed screen row tracking
        // Chunk: docs/chunks/terminal_styling_fidelity - Per-span foreground colors for terminal styling
        let glyph_start_index = indices.len();

        {
            let mut cumulative_screen_row: usize = 0;
            let mut is_first_buffer_line = true;
            let cols_per_row = wrap_layout.cols_per_row();

            for buffer_line in first_visible_buffer_line..line_count {
                if cumulative_screen_row >= max_screen_rows {
                    break;
                }

                // Get styled_line and iterate spans to preserve per-span colors
                let styled_line = if let Some(sl) = view.styled_line(buffer_line) {
                    sl
                } else {
                    is_first_buffer_line = false;
                    continue;
                };

                // Calculate line length from spans
                let line_len: usize = styled_line.spans.iter().map(|s| s.text.chars().count()).sum();
                let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

                // Determine the starting row offset within this buffer line
                let start_row_offset = if is_first_buffer_line {
                    screen_row_offset_in_line
                } else {
                    0
                };
                is_first_buffer_line = false;

                // Calculate which buffer columns to skip (those before start_row_offset)
                let start_col = start_row_offset * cols_per_row;

                // Iterate spans, tracking cumulative column position
                let mut col: usize = 0;
                for span in &styled_line.spans {
                    // Skip hidden text
                    if span.style.hidden {
                        col += span.text.chars().count();
                        continue;
                    }

                    // Resolve foreground color for this span
                    let (fg, _) = self.palette.resolve_style_colors(&span.style);

                    for c in span.text.chars() {
                        // Skip characters on rows before our starting row
                        if col < start_col {
                            col += 1;
                            continue;
                        }

                        // Skip spaces (they don't need quads)
                        if c == ' ' {
                            col += 1;
                            continue;
                        }

                        // Get the glyph info from the atlas (adding on-demand if needed)
                        // Chunk: docs/chunks/terminal_background_box_drawing - On-demand glyph addition
                        let glyph = match atlas.ensure_glyph(font, c) {
                            Some(g) => g,
                            None => {
                                col += 1;
                                continue;
                            }
                        };

                        // Calculate screen position using wrap layout
                        let (row_offset, screen_col) = wrap_layout.buffer_col_to_screen_pos(col);
                        // Adjust row_offset to be relative to viewport top
                        let screen_row = cumulative_screen_row + (row_offset - start_row_offset);

                        if screen_row >= max_screen_rows {
                            // Don't break entirely - there might be more chars on earlier rows
                            col += 1;
                            continue;
                        }

                        // Generate quad at the calculated screen position with per-span fg color
                        // Chunk: docs/chunks/workspace_model - Apply x_offset for left rail
                        // Chunk: docs/chunks/content_tab_bar - Apply y_offset for tab bar
                        let effective_y_offset = y_offset - self.y_offset;
                        let quad = self.layout.quad_vertices_with_xy_offset(
                            screen_row,
                            screen_col,
                            glyph,
                            self.x_offset,
                            effective_y_offset,
                            fg,
                        );
                        vertices.extend_from_slice(&quad);
                        push_quad_indices(&mut indices, &mut vertex_offset);

                        col += 1;
                    }
                }

                cumulative_screen_row += rows_for_line - start_row_offset;
            }
        }

        let glyph_index_count = indices.len() - glyph_start_index;
        self.glyph_range = QuadRange::new(glyph_start_index, glyph_index_count);

        // ==================== Phase 4: Underline Quads ====================
        // Chunk: docs/chunks/terminal_styling_fidelity - Underline rendering for terminal styling
        let underline_start_index = indices.len();

        {
            let solid_glyph = atlas.solid_glyph();
            let cols_per_row = wrap_layout.cols_per_row();
            let mut cumulative_screen_row: usize = 0;
            let mut is_first_buffer_line = true;

            for buffer_line in first_visible_buffer_line..line_count {
                if cumulative_screen_row >= max_screen_rows {
                    break;
                }

                let line_len = view.line_len(buffer_line);
                let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

                // Determine the starting row offset within this buffer line
                let start_row_offset = if is_first_buffer_line {
                    screen_row_offset_in_line
                } else {
                    0
                };
                is_first_buffer_line = false;

                // Iterate spans and emit underline quads for underlined spans
                if let Some(styled_line) = view.styled_line(buffer_line) {
                    let mut col: usize = 0;
                    for span in &styled_line.spans {
                        let span_len = span.text.chars().count();
                        let end_col = col + span_len;

                        // Emit underline quad if underline style is not None
                        if span.style.underline != UnderlineStyle::None {
                            // Use underline_color if specified, otherwise use fg color
                            let underline_color = if let Some(uc) = span.style.underline_color {
                                self.palette.resolve_color(uc, true)
                            } else {
                                let (fg, _) = self.palette.resolve_style_colors(&span.style);
                                fg
                            };

                            // The span may cross multiple screen rows due to wrapping.
                            // Emit an underline quad for each screen row the span covers.
                            let span_start_row_offset = col / cols_per_row;
                            let span_end_row_offset = if end_col == 0 { 0 } else { (end_col - 1) / cols_per_row };

                            for row_offset in span_start_row_offset..=span_end_row_offset {
                                // Skip rows before our starting row
                                if row_offset < start_row_offset {
                                    continue;
                                }

                                let screen_row = cumulative_screen_row + (row_offset - start_row_offset);
                                if screen_row >= max_screen_rows {
                                    break;
                                }

                                // Calculate the portion of the span on this screen row
                                let row_start_col = row_offset * cols_per_row;
                                let row_end_col = (row_offset + 1) * cols_per_row;

                                let span_start_on_row = col.max(row_start_col);
                                let span_end_on_row = end_col.min(row_end_col);

                                if span_start_on_row < span_end_on_row {
                                    // Convert to screen columns (relative to this screen row)
                                    let screen_start_col = span_start_on_row - row_start_col;
                                    let screen_end_col = span_end_on_row - row_start_col;

                                    let quad = self.create_underline_quad(
                                        screen_row,
                                        screen_start_col,
                                        screen_end_col,
                                        solid_glyph,
                                        y_offset,
                                        underline_color,
                                    );
                                    vertices.extend_from_slice(&quad);
                                    push_quad_indices(&mut indices, &mut vertex_offset);
                                }
                            }
                        }

                        col = end_col;
                    }
                }

                cumulative_screen_row += rows_for_line - start_row_offset;
            }
        }

        let underline_index_count = indices.len() - underline_start_index;
        self.underline_range = QuadRange::new(underline_start_index, underline_index_count);

        // ==================== Phase 5: Cursor Quad ====================
        // Chunk: docs/chunks/cursor_wrap_scroll_alignment - Fixed cursor positioning
        let cursor_start_index = indices.len();

        if cursor_visible {
            if let Some(cursor_info) = view.cursor_info() {
                let cursor_pos = cursor_info.position;
                let solid_glyph = atlas.solid_glyph();

                // Check if cursor is above the viewport
                if cursor_pos.line < first_visible_buffer_line {
                    // Cursor is above viewport, don't render
                } else {
                    // Calculate the cumulative screen row for the cursor's buffer line
                    // Start from the first visible buffer line and track screen rows
                    let mut cumulative_screen_row: usize = 0;
                    let mut is_first_buffer_line = true;
                    let mut found_cursor = false;

                    for buffer_line in first_visible_buffer_line..=cursor_pos.line.min(line_count.saturating_sub(1)) {
                        let line_len = view.line_len(buffer_line);
                        let rows_for_line = wrap_layout.screen_rows_for_line(line_len);

                        // Determine the starting row offset within this buffer line
                        let start_row_offset = if is_first_buffer_line {
                            screen_row_offset_in_line
                        } else {
                            0
                        };
                        is_first_buffer_line = false;

                        if buffer_line == cursor_pos.line {
                            // Calculate cursor's screen position within this buffer line
                            let (row_offset, screen_col) = wrap_layout.buffer_col_to_screen_pos(cursor_pos.col);

                            // Check if the cursor's row is scrolled above the viewport
                            if row_offset < start_row_offset {
                                // Cursor's row is above viewport, don't render
                                break;
                            }

                            // Adjust row_offset to be relative to viewport top
                            let screen_row = cumulative_screen_row + (row_offset - start_row_offset);

                            if screen_row < max_screen_rows {
                                let cursor_quad = self.create_cursor_quad_with_offset(
                                    screen_row,
                                    screen_col,
                                    solid_glyph,
                                    y_offset,
                                    cursor_color,
                                );
                                vertices.extend_from_slice(&cursor_quad);
                                push_quad_indices(&mut indices, &mut vertex_offset);
                                found_cursor = true;
                            }
                            break;
                        }

                        cumulative_screen_row += rows_for_line - start_row_offset;
                    }

                    let _ = found_cursor; // Suppress unused warning
                }
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

        let test_color = [1.0, 0.5, 0.0, 1.0];
        let quad = layout.quad_vertices(0, 0, &glyph, test_color);

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

        // Check colors
        assert_eq!(quad[0].color, test_color);
        assert_eq!(quad[1].color, test_color);
        assert_eq!(quad[2].color, test_color);
        assert_eq!(quad[3].color, test_color);
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

    fn test_color() -> [f32; 4] {
        [1.0, 0.0, 0.0, 0.5]
    }

    #[test]
    fn test_selection_quad_single_char() {
        let glyph_buffer = GlyphBuffer::new(&test_metrics());
        let solid = test_solid_glyph();
        let color = test_color();

        // Selection covering one character at row 0, col 0
        let quad = glyph_buffer.create_selection_quad(0, 0, 1, &solid, color);

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
        let color = test_color();

        // Selection covering cols 2-5 (3 characters) on row 1
        let quad = glyph_buffer.create_selection_quad(1, 2, 5, &solid, color);

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
        let color = test_color();

        let quad = glyph_buffer.create_selection_quad(0, 0, 3, &solid, color);

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
        let color = test_color();

        let quad = glyph_buffer.create_selection_quad(0, 0, 1, &solid, color);

        // Height should be line_height (16.0), not glyph height
        let height = quad[2].position[1] - quad[0].position[1];
        assert_eq!(height, 16.0);
    }

    #[test]
    fn test_selection_quad_color() {
        let glyph_buffer = GlyphBuffer::new(&test_metrics());
        let solid = test_solid_glyph();
        let color = test_color();

        let quad = glyph_buffer.create_selection_quad(0, 0, 1, &solid, color);

        // All vertices should have the same color
        assert_eq!(quad[0].color, color);
        assert_eq!(quad[1].color, color);
        assert_eq!(quad[2].color, color);
        assert_eq!(quad[3].color, color);
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

    // ==================== Tab Bar/Rail Offset Tests ====================
    // Chunk: docs/chunks/tab_bar_layout_fixes - Glyph positioning with content offsets

    #[test]
    fn test_position_for_with_xy_offset() {
        // Test that position_for_with_xy_offset correctly applies both offsets
        let layout = GlyphLayout::from_metrics(&test_metrics());

        // With x_offset=56 (RAIL_WIDTH) and y_offset=-32 (negative to shift DOWN for tab bar)
        // Note: y_offset is SUBTRACTED in the calculation, so passing 32 shifts DOWN
        let (x, y) = layout.position_for_with_xy_offset(0, 0, 56.0, -32.0);

        // x should be 0 + 56 = 56
        assert_eq!(x, 56.0);
        // y should be 0 - (-32) = 32 (shifted down by 32 for tab bar)
        assert_eq!(y, 32.0);
    }

    #[test]
    fn test_quad_vertices_with_xy_offset() {
        // Test that glyph quads are correctly positioned with both offsets
        let layout = GlyphLayout::from_metrics(&test_metrics());

        let glyph = GlyphInfo {
            uv_min: (0.0, 0.0),
            uv_max: (0.1, 0.2),
            width: 10.0,
            height: 18.0,
            bearing_x: 0.0,
            bearing_y: 12.0,
        };

        let test_color = [1.0, 0.5, 0.0, 1.0];
        // x_offset=56 (RAIL_WIDTH), y_offset=-32 (to shift DOWN by 32 for tab bar)
        let quad = layout.quad_vertices_with_xy_offset(0, 0, &glyph, 56.0, -32.0, test_color);

        // Glyph at row 0, col 0 with offsets should be at:
        // x = 0 * 8 + 56 = 56
        // y = 0 * 16 - (-32) = 32
        assert_eq!(quad[0].position, [56.0, 32.0]);    // top-left
        assert_eq!(quad[1].position, [66.0, 32.0]);    // top-right (56 + 10)
        assert_eq!(quad[2].position, [66.0, 50.0]);    // bottom-right (32 + 18)
        assert_eq!(quad[3].position, [56.0, 50.0]);    // bottom-left
    }

    #[test]
    fn test_selection_quad_with_offset_applies_x_offset() {
        // Selection quads should also apply x_offset
        let mut glyph_buffer = GlyphBuffer::new(&test_metrics());
        glyph_buffer.set_x_offset(56.0); // RAIL_WIDTH
        glyph_buffer.set_y_offset(32.0); // TAB_BAR_HEIGHT

        let solid = test_solid_glyph();
        let color = test_color();

        // Selection at row 0, cols 0-1, with y_offset=0 (no fractional scroll)
        let quad = glyph_buffer.create_selection_quad_with_offset(0, 0, 1, &solid, 0.0, color);

        // With x_offset=56 and y_offset=32 (shifted down):
        // x = 0 + 56 = 56
        // y = 0 - (0 - 32) = 32 (effective_y_offset = y_offset - self.y_offset = 0 - 32 = -32)
        assert_eq!(quad[0].position, [56.0, 32.0]);    // top-left
        assert_eq!(quad[1].position, [64.0, 32.0]);    // top-right (56 + 8)
        assert_eq!(quad[2].position, [64.0, 48.0]);    // bottom-right (32 + 16)
        assert_eq!(quad[3].position, [56.0, 48.0]);    // bottom-left
    }

    #[test]
    fn test_cursor_quad_with_offset_applies_x_offset() {
        // Cursor quads should also apply x_offset
        let mut glyph_buffer = GlyphBuffer::new(&test_metrics());
        glyph_buffer.set_x_offset(56.0); // RAIL_WIDTH
        glyph_buffer.set_y_offset(32.0); // TAB_BAR_HEIGHT

        let solid = test_solid_glyph();
        let color = test_color();

        // Cursor at row 0, col 0, with y_offset=0 (no fractional scroll)
        let quad = glyph_buffer.create_cursor_quad_with_offset(0, 0, &solid, 0.0, color);

        // With x_offset=56 and y_offset=32:
        // x = 0 + 56 = 56
        // y = 0 - (0 - 32) = 32
        assert_eq!(quad[0].position, [56.0, 32.0]);    // top-left
    }
}
