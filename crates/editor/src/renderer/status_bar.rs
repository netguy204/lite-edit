// Chunk: docs/chunks/gotodef_status_render - Status bar rendering extracted from renderer.rs

//! Status bar rendering implementation.
//!
//! This module contains the methods for rendering the status bar overlay
//! at the bottom of the viewport or within pane bounds. The status bar shows
//! transient messages like "Definition not found" or "Indexing workspace...".

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder, MTLScissorRect,
};

use crate::glyph_buffer::GlyphLayout;
use crate::metal_view::MetalView;
use crate::pane_layout::PaneRect;
use crate::selector_overlay::{
    calculate_status_bar_geometry, calculate_status_bar_geometry_in_pane,
    StatusBarGlyphBuffer,
};

use super::constants::Uniforms;
use super::Renderer;

impl Renderer {
    // =========================================================================
    // Status Bar Rendering (Chunk: docs/chunks/gotodef_status_render)
    // =========================================================================

    /// Draws the status bar at the bottom of the viewport.
    ///
    /// The status bar is a one-line-tall bar that shows the status message text.
    /// It has no cursor or input capability - it's display-only.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `text` - The status message text to display
    pub(super) fn draw_status_bar(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        text: &str,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let line_height = self.font.metrics.line_height as f32;
        let glyph_width = self.font.metrics.advance_width as f32;

        // Calculate status bar geometry
        let geometry = calculate_status_bar_geometry(
            view_width,
            view_height,
            line_height,
            glyph_width,
        );

        // Ensure status bar buffer is initialized
        if self.status_bar_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.status_bar_buffer = Some(StatusBarGlyphBuffer::new(layout));
        }

        // Update the status bar buffer with current content
        let status_bar_buffer = self.status_bar_buffer.as_mut().unwrap();
        status_bar_buffer.update(
            &self.device,
            &self.atlas,
            text,
            &geometry,
        );

        // Get buffers
        let vertex_buffer = match status_bar_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match status_bar_buffer.index_buffer() {
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
        let bg_range = status_bar_buffer.background_range();
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

        // Draw text
        let text_range = status_bar_buffer.text_range();
        if !text_range.is_empty() {
            let index_offset = text_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    text_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }
    }

    /// Draws the status bar within a specific pane's bounds.
    ///
    /// This method is used for multi-pane layouts where the status bar should
    /// appear within the focused pane rather than spanning the entire viewport.
    /// It sets a scissor rect to clip rendering to the pane bounds.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `text` - The status message text to display
    /// * `pane_rect` - The bounds of the pane to render within
    /// * `view_width` - Full viewport width (for uniforms)
    /// * `view_height` - Full viewport height (for uniforms)
    pub(super) fn draw_status_bar_in_pane(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        _view: &MetalView, // Unused but kept for API consistency with draw_status_bar
        text: &str,
        pane_rect: &PaneRect,
        view_width: f32,
        view_height: f32,
    ) {
        let line_height = self.font.metrics.line_height as f32;
        let glyph_width = self.font.metrics.advance_width as f32;

        // Calculate status bar geometry within the pane bounds
        let geometry = calculate_status_bar_geometry_in_pane(
            pane_rect.x,
            pane_rect.y,
            pane_rect.width,
            pane_rect.height,
            line_height,
            glyph_width,
        );

        // Set scissor rect to clip rendering to pane bounds
        let pane_scissor = MTLScissorRect {
            x: pane_rect.x as usize,
            y: pane_rect.y as usize,
            width: pane_rect.width as usize,
            height: pane_rect.height as usize,
        };
        encoder.setScissorRect(pane_scissor);

        // Ensure status bar buffer is initialized
        if self.status_bar_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.status_bar_buffer = Some(StatusBarGlyphBuffer::new(layout));
        }

        // Update the status bar buffer with current content
        let status_bar_buffer = self.status_bar_buffer.as_mut().unwrap();
        status_bar_buffer.update(
            &self.device,
            &self.atlas,
            text,
            &geometry,
        );

        // Get buffers
        let vertex_buffer = match status_bar_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match status_bar_buffer.index_buffer() {
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
        let bg_range = status_bar_buffer.background_range();
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

        // Draw text
        let text_range = status_bar_buffer.text_range();
        if !text_range.is_empty() {
            let index_offset = text_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    text_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }
    }
}
