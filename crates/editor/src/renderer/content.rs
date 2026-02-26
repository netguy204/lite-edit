// Chunk: docs/chunks/renderer_decomposition - Text content rendering extracted from renderer.rs
// Chunk: docs/chunks/glyph_rendering - render_text and set_content for glyph-based text rendering

//! Text content rendering implementation.
//!
//! This module contains the methods for rendering text buffer content:
//! - Glyph buffer updates from buffer views
//! - The render_text method for drawing text quads
//! - Legacy render methods

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
};

use lite_edit_buffer::BufferView;

use crate::metal_view::MetalView;
use crate::wrap_layout::WrapLayout;

use super::constants::{BORDER_COLOR, Uniforms};
use super::Renderer;

impl Renderer {
    // Chunk: docs/chunks/viewport_fractional_scroll - Pass y_offset for smooth scrolling
    // Chunk: docs/chunks/line_wrap_rendering - Use WrapLayout for soft wrapping
    // Chunk: docs/chunks/renderer_polymorphic_buffer - Accept &dyn BufferView for polymorphic rendering
    // Chunk: docs/chunks/wrap_click_offset - Use content_width_px for consistent cols_per_row
    // Chunk: docs/chunks/terminal_background_box_drawing - Pass mutable atlas and font for on-demand glyph addition
    /// Updates the glyph buffer from the given buffer view and viewport
    pub(super) fn update_glyph_buffer(&mut self, view: &dyn BufferView) {
        self.update_glyph_buffer_with_cursor_visible(view, self.cursor_visible);
    }

    // Chunk: docs/chunks/cursor_blink_pane_focus - Pane-aware cursor visibility for multi-pane rendering
    /// Updates the glyph buffer with explicit cursor visibility.
    ///
    /// In multi-pane layouts, only the focused pane should show a blinking cursor.
    /// Unfocused panes pass `cursor_visible: false` to display a static (hidden) cursor.
    pub(super) fn update_glyph_buffer_with_cursor_visible(&mut self, view: &dyn BufferView, cursor_visible: bool) {
        // Get the fractional scroll offset for smooth scrolling
        let y_offset = self.viewport.scroll_fraction_px();

        // Create wrap layout for current content width (viewport - RAIL_WIDTH).
        // Using content_width_px ensures the same cols_per_row value is computed
        // here as in wrap_layout(), which is used for click hit-testing.
        let wrap_layout = WrapLayout::new(self.content_width_px, &self.font.metrics);

        // Use wrap-aware rendering with mutable atlas for on-demand glyph addition
        self.glyph_buffer.update_from_buffer_with_wrap(
            &self.device,
            &mut self.atlas,
            &self.font,
            view,
            &self.viewport,
            &wrap_layout,
            cursor_visible,
            y_offset,
        );
    }

    /// Sets the text content to display
    ///
    /// # Arguments
    /// * `lines` - The text lines to render
    pub fn set_content(&mut self, lines: &[&str]) {
        self.glyph_buffer.update(&self.device, &mut self.atlas, &self.font, lines);
    }

    // Chunk: docs/chunks/text_selection_rendering - Three-pass draw with separate fragment color uniforms per quad category
    /// Renders the text content using the glyph pipeline
    ///
    /// Draws quads in three passes with different colors:
    /// 1. Selection highlight quads (SELECTION_COLOR)
    /// 2. Glyph quads (TEXT_COLOR)
    /// 3. Cursor quad (TEXT_COLOR)
    pub(super) fn render_text(
        &self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
    ) {
        // Get buffers
        let vertex_buffer = match self.glyph_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match self.glyph_buffer.index_buffer() {
            Some(b) => b,
            None => return,
        };

        // Set the render pipeline state
        encoder.setRenderPipelineState(self.pipeline.pipeline_state());

        // Set the vertex buffer at index 0
        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(vertex_buffer), 0, 0);
        }

        // Create and set uniforms (viewport size)
        let frame = view.frame();
        let scale = view.scale_factor();
        let uniforms = Uniforms {
            viewport_size: [
                (frame.size.width * scale) as f32,
                (frame.size.height * scale) as f32,
            ],
        };

        // Set uniforms at buffer index 1
        let uniforms_ptr =
            NonNull::new(&uniforms as *const Uniforms as *mut std::ffi::c_void).unwrap();
        unsafe {
            encoder.setVertexBytes_length_atIndex(
                uniforms_ptr,
                std::mem::size_of::<Uniforms>(),
                1,
            );
        }

        // Set the atlas texture at texture index 0
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(self.atlas.texture()), 0);
        }

        // Chunk: docs/chunks/renderer_styled_content - Per-vertex colors, no per-draw uniforms needed
        // With per-vertex colors, we draw all quads in a single pass with no uniform changes.
        // Draw order: background → selection → glyphs → underlines → cursor

        // ==================== Draw Background Quads ====================
        let background_range = self.glyph_buffer.background_range();
        if !background_range.is_empty() {
            let index_offset = background_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    background_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Selection Quads ====================
        let selection_range = self.glyph_buffer.selection_range();
        if !selection_range.is_empty() {
            let index_offset = selection_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    selection_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Border Quads ====================
        // Chunk: docs/chunks/line_wrap_rendering - Draw continuation row borders
        let border_range = self.glyph_buffer.border_range();
        if !border_range.is_empty() {
            // Set border color (black)
            let border_color_ptr =
                NonNull::new(BORDER_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(
                    border_color_ptr,
                    std::mem::size_of::<[f32; 4]>(),
                    0,
                );
            }

            // Draw border quads
            let index_offset = border_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    border_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Glyph Quads ====================
        let glyph_range = self.glyph_buffer.glyph_range();
        if !glyph_range.is_empty() {
            let index_offset = glyph_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    glyph_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Underline Quads ====================
        let underline_range = self.glyph_buffer.underline_range();
        if !underline_range.is_empty() {
            let index_offset = underline_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    underline_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Cursor Quad ====================
        let cursor_range = self.glyph_buffer.cursor_range();
        if !cursor_range.is_empty() {
            let index_offset = cursor_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    cursor_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }
    }
}
