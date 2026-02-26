// Chunk: docs/chunks/renderer_decomposition - Welcome screen rendering extracted from renderer.rs

//! Welcome screen rendering implementation.
//!
//! This module contains the methods for rendering the welcome screen
//! shown when an empty buffer is active:
//! - Full-viewport welcome screen
//! - Per-pane welcome screen (multi-pane layouts)

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
};

use crate::glyph_buffer::GlyphLayout;
use crate::left_rail::RAIL_WIDTH;
use crate::metal_view::MetalView;
use crate::pane_layout::PaneRect;
use crate::tab_bar::TAB_BAR_HEIGHT;
use crate::welcome_screen::{calculate_welcome_geometry, WelcomeScreenGlyphBuffer};

use super::constants::Uniforms;
use super::Renderer;

impl Renderer {
    // Chunk: docs/chunks/welcome_screen - Welcome screen rendering
    // Chunk: docs/chunks/welcome_screen - Renders welcome screen content using Metal glyph pipeline
    // Chunk: docs/chunks/welcome_scroll - Welcome screen vertical scrolling in draw function
    /// Draws the welcome screen when the active tab has an empty buffer.
    ///
    /// The welcome screen shows a feather ASCII art logo, the editor name,
    /// tagline, and a hotkey reference table. Content is centered horizontally
    /// and vertically (or scrolled when content overflows the viewport).
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `scroll_offset_px` - Vertical scroll offset from the active tab's welcome scroll state
    pub(super) fn draw_welcome_screen(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        scroll_offset_px: f32,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;

        // Calculate content area (excluding left rail and tab bar)
        let content_width = view_width - RAIL_WIDTH;
        let content_height = view_height - TAB_BAR_HEIGHT;

        // Skip if content area is too small
        if content_width <= 0.0 || content_height <= 0.0 {
            return;
        }

        // Calculate welcome screen geometry (centered or scrolled in content area)
        let glyph_width = self.font.metrics.advance_width as f32;
        let line_height = self.font.metrics.line_height as f32;
        let mut geometry = calculate_welcome_geometry(
            content_width,
            content_height,
            glyph_width,
            line_height,
            scroll_offset_px,
        );

        // Offset the geometry to account for left rail and tab bar
        geometry.content_x += RAIL_WIDTH;
        geometry.content_y += TAB_BAR_HEIGHT;

        // Ensure welcome screen buffer is initialized
        if self.welcome_screen_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.welcome_screen_buffer = Some(WelcomeScreenGlyphBuffer::new(layout));
        }

        // Update the welcome screen buffer
        let welcome_buffer = self.welcome_screen_buffer.as_mut().unwrap();
        welcome_buffer.update(&self.device, &self.atlas, &geometry);

        // Get buffers
        let vertex_buffer = match welcome_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match welcome_buffer.index_buffer() {
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

        // Draw all welcome screen content in a single pass
        // (per-vertex colors are embedded in the vertex data)
        let total_indices = welcome_buffer.index_count();
        if total_indices > 0 {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    total_indices,
                    MTLIndexType::UInt32,
                    index_buffer,
                    0,
                );
            }
        }
    }

    // Chunk: docs/chunks/tiling_multi_pane_render - Pane-local welcome screen
    // Chunk: docs/chunks/welcome_scroll - Welcome screen vertical scrolling in multi-pane rendering
    /// Draws the welcome screen within a pane's bounds.
    ///
    /// This is similar to `draw_welcome_screen` but positions the content
    /// within the specified pane rectangle.
    ///
    /// # Arguments
    /// * `scroll_offset_px` - Vertical scroll offset from the active tab's welcome scroll state
    pub(super) fn draw_welcome_screen_in_pane(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        pane_rect: &PaneRect,
        scroll_offset_px: f32,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let glyph_width = self.font.metrics.advance_width as f32;
        let line_height = self.font.metrics.line_height as f32;

        // Calculate content area within pane (excluding pane's tab bar)
        let content_width = pane_rect.width;
        let content_height = pane_rect.height - TAB_BAR_HEIGHT;

        // Calculate geometry centered or scrolled in pane
        let mut geometry = calculate_welcome_geometry(
            content_width,
            content_height,
            glyph_width,
            line_height,
            scroll_offset_px,
        );

        // Offset to pane position
        geometry.content_x += pane_rect.x;
        geometry.content_y += pane_rect.y + TAB_BAR_HEIGHT;

        // Ensure welcome screen buffer is initialized
        if self.welcome_screen_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.welcome_screen_buffer = Some(WelcomeScreenGlyphBuffer::new(layout));
        }

        // Update and render the welcome screen
        let welcome_buffer = self.welcome_screen_buffer.as_mut().unwrap();
        welcome_buffer.update(&self.device, &self.atlas, &geometry);

        // Get buffers
        let vertex_buffer = match welcome_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match welcome_buffer.index_buffer() {
            Some(b) => b,
            None => return,
        };

        // Set render state and draw
        encoder.setRenderPipelineState(self.pipeline.pipeline_state());
        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(vertex_buffer), 0, 0);
        }

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

        unsafe {
            encoder.setFragmentTexture_atIndex(Some(self.atlas.texture()), 0);
        }

        // Draw all welcome screen content in a single pass
        // (per-vertex colors are embedded in the vertex data)
        let total_indices = welcome_buffer.index_count();
        if total_indices > 0 {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    total_indices,
                    MTLIndexType::UInt32,
                    index_buffer,
                    0,
                );
            }
        }
    }
}
