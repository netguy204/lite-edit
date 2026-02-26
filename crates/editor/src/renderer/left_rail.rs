// Chunk: docs/chunks/renderer_decomposition - Left rail rendering extracted from renderer.rs

//! Left rail (workspace tiles) rendering implementation.
//!
//! This module contains the methods for rendering the left rail sidebar
//! that shows workspace tiles and status indicators.

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
};

use crate::glyph_buffer::GlyphLayout;
use crate::left_rail::{
    calculate_left_rail_geometry, status_color,
    LeftRailGlyphBuffer,
    RAIL_BACKGROUND_COLOR, TILE_ACTIVE_COLOR, TILE_BACKGROUND_COLOR,
};
use crate::metal_view::MetalView;
use crate::workspace::Editor;

use super::constants::Uniforms;
use super::Renderer;

impl Renderer {
    // Chunk: docs/chunks/workspace_model - Left rail rendering
    /// Draws the left rail (workspace tiles) on the left edge of the viewport.
    ///
    /// Each workspace gets a tile showing its label. The active workspace
    /// tile is highlighted. Colors indicate workspace status.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `editor` - The Editor state containing workspace information
    pub(super) fn draw_left_rail(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        editor: &Editor,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_height = (frame.size.height * scale) as f32;
        let view_width = (frame.size.width * scale) as f32;

        // Calculate left rail geometry
        let workspace_count = editor.workspace_count();
        let geometry = calculate_left_rail_geometry(view_height, workspace_count);

        // Ensure left rail buffer is initialized
        if self.left_rail_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.left_rail_buffer = Some(LeftRailGlyphBuffer::new(layout));
        }

        // Update the left rail buffer with current editor state
        let left_rail_buffer = self.left_rail_buffer.as_mut().unwrap();
        left_rail_buffer.update(&self.device, &self.atlas, editor, &geometry);

        // Get buffers
        let vertex_buffer = match left_rail_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match left_rail_buffer.index_buffer() {
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
        let bg_range = left_rail_buffer.background_range();
        if !bg_range.is_empty() {
            let color_ptr = NonNull::new(RAIL_BACKGROUND_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
            }
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    bg_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw inactive tile backgrounds
        let tile_bg_range = left_rail_buffer.tile_background_range();
        if !tile_bg_range.is_empty() {
            let color_ptr = NonNull::new(TILE_BACKGROUND_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
            }
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    tile_bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    tile_bg_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw active tile highlight
        let active_range = left_rail_buffer.active_tile_range();
        if !active_range.is_empty() {
            let color_ptr = NonNull::new(TILE_ACTIVE_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
            }
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    active_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    active_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw status indicators (using active workspace's status color)
        let status_range = left_rail_buffer.status_indicator_range();
        if !status_range.is_empty() {
            // Use the active workspace's status color, fall back to gray
            let indicator_color = if let Some(ws) = editor.active_workspace() {
                status_color(&ws.status)
            } else {
                [0.5, 0.5, 0.5, 1.0]
            };
            let color_ptr = NonNull::new(indicator_color.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
            }
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    status_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    status_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw identicons (colors are per-vertex, no fragment uniform needed)
        // Chunk: docs/chunks/workspace_identicon - Workspace identicons
        let identicon_range = left_rail_buffer.identicon_range();
        if !identicon_range.is_empty() {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    identicon_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    identicon_range.start * std::mem::size_of::<u32>(),
                );
            }
        }
    }
}
