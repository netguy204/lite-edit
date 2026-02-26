// Chunk: docs/chunks/renderer_decomposition - Tab bar rendering extracted from renderer.rs

//! Tab bar rendering implementation.
//!
//! This module contains the methods for rendering tab bars:
//! - Global tab bar (single-pane mode)
//! - Per-pane tab bars (multi-pane mode)

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
};

use crate::glyph_buffer::GlyphLayout;
use crate::metal_view::MetalView;
use crate::pane_layout::{Pane, PaneRect};
use crate::tab_bar::{
    calculate_pane_tab_bar_geometry, calculate_tab_bar_geometry,
    tabs_from_pane, tabs_from_workspace,
    TabBarGlyphBuffer,
    CLOSE_BUTTON_COLOR, TAB_ACTIVE_COLOR,
    TAB_BAR_BACKGROUND_COLOR, TAB_INACTIVE_COLOR, TAB_LABEL_COLOR,
};
use crate::workspace::Editor;

use super::constants::Uniforms;
use super::Renderer;

impl Renderer {
    // Chunk: docs/chunks/content_tab_bar - Tab bar rendering
    /// Draws the tab bar at the top of the content area.
    ///
    /// The tab bar is positioned to the right of the left rail and spans
    /// the remaining width of the viewport. Each tab shows its label,
    /// and the active tab is highlighted.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `editor` - The Editor state containing workspace information
    pub(super) fn draw_tab_bar(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        editor: &Editor,
    ) {
        // Only draw tab bar if there's an active workspace with tabs
        let workspace = match editor.active_workspace() {
            Some(ws) => ws,
            None => return,
        };

        if workspace.tab_count() == 0 {
            return;
        }

        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let glyph_width = self.font.metrics.advance_width as f32;

        // Get tab info from workspace
        let tabs = tabs_from_workspace(workspace);

        // Calculate tab bar geometry with scroll offset
        let geometry = calculate_tab_bar_geometry(view_width, &tabs, glyph_width, workspace.tab_bar_view_offset());

        // Ensure tab bar buffer is initialized
        if self.tab_bar_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.tab_bar_buffer = Some(TabBarGlyphBuffer::new(layout));
        }

        // Update the tab bar buffer
        let tab_bar_buffer = self.tab_bar_buffer.as_mut().unwrap();
        tab_bar_buffer.update(&self.device, &self.atlas, &tabs, &geometry);

        // Get buffers
        let vertex_buffer = match tab_bar_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match tab_bar_buffer.index_buffer() {
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
        let bg_range = tab_bar_buffer.background_range();
        if !bg_range.is_empty() {
            let color_ptr = NonNull::new(TAB_BAR_BACKGROUND_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    bg_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw inactive tab backgrounds
        let tab_bg_range = tab_bar_buffer.tab_background_range();
        if !tab_bg_range.is_empty() {
            let color_ptr = NonNull::new(TAB_INACTIVE_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    tab_bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    tab_bg_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw active tab highlight
        let active_range = tab_bar_buffer.active_tab_range();
        if !active_range.is_empty() {
            let color_ptr = NonNull::new(TAB_ACTIVE_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    active_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    active_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw indicators (dirty/unread)
        // Colors are baked into vertex data (dirty = yellow, unread = blue)
        let indicator_range = tab_bar_buffer.indicator_range();
        if !indicator_range.is_empty() {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    indicator_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    indicator_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw close buttons
        let close_range = tab_bar_buffer.close_button_range();
        if !close_range.is_empty() {
            let color_ptr = NonNull::new(CLOSE_BUTTON_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    close_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    close_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw labels
        let label_range = tab_bar_buffer.label_range();
        if !label_range.is_empty() {
            let color_ptr = NonNull::new(TAB_LABEL_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    label_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    label_range.start * std::mem::size_of::<u32>(),
                );
            }
        }
    }

    // Chunk: docs/chunks/tiling_multi_pane_render - Pane tab bar rendering
    /// Draws a pane's tab bar at the specified position.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `pane` - The pane whose tabs to render
    /// * `pane_rect` - The rectangle for this pane
    /// * `view_width` - The viewport width
    /// * `view_height` - The viewport height
    pub(super) fn draw_pane_tab_bar(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        _view: &MetalView,
        pane: &Pane,
        pane_rect: &PaneRect,
        view_width: f32,
        view_height: f32,
    ) {
        if pane.tab_count() == 0 {
            return;
        }

        let glyph_width = self.font.metrics.advance_width as f32;

        // Get tab info from pane
        let tabs = tabs_from_pane(pane);

        // Calculate geometry for this pane's tab bar
        let geometry = calculate_pane_tab_bar_geometry(
            pane_rect.x,
            pane_rect.y,
            pane_rect.width,
            &tabs,
            glyph_width,
            pane.tab_bar_view_offset,
        );

        // Ensure tab bar buffer is initialized
        if self.tab_bar_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.tab_bar_buffer = Some(TabBarGlyphBuffer::new(layout));
        }

        // Update the tab bar buffer
        let tab_bar_buffer = self.tab_bar_buffer.as_mut().unwrap();
        tab_bar_buffer.update(&self.device, &self.atlas, &tabs, &geometry);

        // Get buffers
        let vertex_buffer = match tab_bar_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match tab_bar_buffer.index_buffer() {
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
        let bg_range = tab_bar_buffer.background_range();
        if !bg_range.is_empty() {
            let color_ptr = NonNull::new(TAB_BAR_BACKGROUND_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    bg_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw inactive tab backgrounds
        let tab_bg_range = tab_bar_buffer.tab_background_range();
        if !tab_bg_range.is_empty() {
            let color_ptr = NonNull::new(TAB_INACTIVE_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    tab_bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    tab_bg_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw active tab highlight
        let active_range = tab_bar_buffer.active_tab_range();
        if !active_range.is_empty() {
            let color_ptr = NonNull::new(TAB_ACTIVE_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    active_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    active_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw indicator dots
        let indicator_range = tab_bar_buffer.indicator_range();
        if !indicator_range.is_empty() {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    indicator_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    indicator_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw close buttons
        let close_range = tab_bar_buffer.close_button_range();
        if !close_range.is_empty() {
            let color_ptr = NonNull::new(CLOSE_BUTTON_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    close_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    close_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw labels
        let label_range = tab_bar_buffer.label_range();
        if !label_range.is_empty() {
            let color_ptr = NonNull::new(TAB_LABEL_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    label_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    label_range.start * std::mem::size_of::<u32>(),
                );
            }
        }
    }
}
