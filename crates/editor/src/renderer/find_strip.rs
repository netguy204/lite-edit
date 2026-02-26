// Chunk: docs/chunks/renderer_decomposition - Find strip rendering extracted from renderer.rs

//! Find strip rendering implementation.
//!
//! This module contains the methods for rendering the find-in-file strip
//! at the bottom of the viewport or within pane bounds.

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder, MTLScissorRect,
};

use crate::glyph_buffer::GlyphLayout;
use crate::metal_view::MetalView;
use crate::pane_layout::PaneRect;
use crate::selector_overlay::{
    calculate_find_strip_geometry, calculate_find_strip_geometry_in_pane,
    FindStripGlyphBuffer,
};

use super::constants::Uniforms;
use super::Renderer;

impl Renderer {
    // =========================================================================
    // Find Strip Rendering (Chunk: docs/chunks/find_in_file)
    // =========================================================================

    // Chunk: docs/chunks/find_strip_multi_pane - render_with_find_strip removed
    // The render_with_find_strip method has been removed. Find strip rendering is now
    // handled by render_with_editor with an optional FindStripState parameter.
    // This fixes the multi-pane bug where the focused pane would expand to fill the
    // entire window when find-in-file was active.

    /// Draws the find strip at the bottom of the viewport.
    ///
    /// The find strip is a one-line-tall bar that shows "find:" followed by
    /// the query text and a blinking cursor.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `query` - The find query text
    /// * `cursor_col` - The cursor column position in the query
    /// * `cursor_visible` - Whether to render the cursor
    // Chunk: docs/chunks/find_in_file - Find strip rendering
    pub(super) fn draw_find_strip(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        query: &str,
        cursor_col: usize,
        cursor_visible: bool,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let line_height = self.font.metrics.line_height as f32;
        let glyph_width = self.font.metrics.advance_width as f32;

        // Calculate find strip geometry
        let geometry = calculate_find_strip_geometry(
            view_width,
            view_height,
            line_height,
            glyph_width,
            cursor_col,
        );

        // Ensure find strip buffer is initialized
        if self.find_strip_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.find_strip_buffer = Some(FindStripGlyphBuffer::new(layout));
        }

        // Update the find strip buffer with current content
        let find_strip_buffer = self.find_strip_buffer.as_mut().unwrap();
        find_strip_buffer.update(
            &self.device,
            &self.atlas,
            query,
            &geometry,
            cursor_visible,
        );

        // Get buffers
        let vertex_buffer = match find_strip_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match find_strip_buffer.index_buffer() {
            Some(b) => b,
            None => return,
        };

        // Set the render pipeline state
        encoder.setRenderPipelineState(self.pipeline.pipeline_state());

        // Set the vertex buffer
        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(vertex_buffer), 0, 0);
        }

        // Set uniforms (viewport size)
        let uniforms = Uniforms {
            viewport_size: [view_width, view_height],
        };
        let uniforms_ptr =
            NonNull::new(&uniforms as *const Uniforms as *mut std::ffi::c_void).unwrap();
        unsafe {
            encoder.setVertexBytes_length_atIndex(
                uniforms_ptr,
                std::mem::size_of::<Uniforms>(),
                1,
            );
        }

        // Set the atlas texture
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(self.atlas.texture()), 0);
        }

        // Draw background
        let bg_range = find_strip_buffer.background_range();
        if !bg_range.is_empty() {
            let index_offset = bg_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw label
        let label_range = find_strip_buffer.label_range();
        if !label_range.is_empty() {
            let index_offset = label_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    label_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw query text
        let query_range = find_strip_buffer.query_text_range();
        if !query_range.is_empty() {
            let index_offset = query_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    query_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw cursor
        let cursor_range = find_strip_buffer.cursor_range();
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

    // Chunk: docs/chunks/find_strip_multi_pane - Pane-constrained find strip rendering
    /// Draws the find strip within a specific pane's bounds.
    ///
    /// This method is used for multi-pane layouts where the find strip should
    /// appear within the focused pane rather than spanning the entire viewport.
    /// It sets a scissor rect to clip rendering to the pane bounds.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `query` - The find query text
    /// * `cursor_col` - The cursor column position in the query
    /// * `cursor_visible` - Whether to render the cursor
    /// * `pane_rect` - The bounds of the pane to render within
    /// * `view_width` - Full viewport width (for uniforms)
    /// * `view_height` - Full viewport height (for uniforms)
    pub(super) fn draw_find_strip_in_pane(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        _view: &MetalView, // Unused but kept for API consistency with draw_find_strip
        query: &str,
        cursor_col: usize,
        cursor_visible: bool,
        pane_rect: &PaneRect,
        view_width: f32,
        view_height: f32,
    ) {
        let line_height = self.font.metrics.line_height as f32;
        let glyph_width = self.font.metrics.advance_width as f32;

        // Calculate find strip geometry within the pane bounds
        let geometry = calculate_find_strip_geometry_in_pane(
            pane_rect.x,
            pane_rect.y,
            pane_rect.width,
            pane_rect.height,
            line_height,
            glyph_width,
            cursor_col,
        );

        // Set scissor rect to clip rendering to pane bounds
        let pane_scissor = MTLScissorRect {
            x: pane_rect.x as usize,
            y: pane_rect.y as usize,
            width: pane_rect.width as usize,
            height: pane_rect.height as usize,
        };
        encoder.setScissorRect(pane_scissor);

        // Ensure find strip buffer is initialized
        if self.find_strip_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.find_strip_buffer = Some(FindStripGlyphBuffer::new(layout));
        }

        // Update the find strip buffer with current content
        let find_strip_buffer = self.find_strip_buffer.as_mut().unwrap();
        find_strip_buffer.update(
            &self.device,
            &self.atlas,
            query,
            &geometry,
            cursor_visible,
        );

        // Get buffers
        let vertex_buffer = match find_strip_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match find_strip_buffer.index_buffer() {
            Some(b) => b,
            None => return,
        };

        // Set the render pipeline state
        encoder.setRenderPipelineState(self.pipeline.pipeline_state());

        // Set the vertex buffer
        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(vertex_buffer), 0, 0);
        }

        // Set uniforms (full viewport size - the scissor rect handles clipping)
        let uniforms = Uniforms {
            viewport_size: [view_width, view_height],
        };
        let uniforms_ptr =
            NonNull::new(&uniforms as *const Uniforms as *mut std::ffi::c_void).unwrap();
        unsafe {
            encoder.setVertexBytes_length_atIndex(
                uniforms_ptr,
                std::mem::size_of::<Uniforms>(),
                1,
            );
        }

        // Set the atlas texture
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(self.atlas.texture()), 0);
        }

        // Draw background
        let bg_range = find_strip_buffer.background_range();
        if !bg_range.is_empty() {
            let index_offset = bg_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw label
        let label_range = find_strip_buffer.label_range();
        if !label_range.is_empty() {
            let index_offset = label_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    label_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw query text
        let query_range = find_strip_buffer.query_text_range();
        if !query_range.is_empty() {
            let index_offset = query_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    query_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw cursor
        let cursor_range = find_strip_buffer.cursor_range();
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
