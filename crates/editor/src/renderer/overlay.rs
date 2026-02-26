// Chunk: docs/chunks/renderer_decomposition - Overlay rendering extracted from renderer.rs

//! Overlay rendering implementation.
//!
//! This module contains the methods for rendering overlay panels:
//! - Selector overlay (file picker, command palette)
//! - Confirm dialog

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
};

use crate::confirm_dialog::{
    calculate_confirm_dialog_geometry, ConfirmDialog, ConfirmDialogGlyphBuffer,
};
use crate::glyph_buffer::GlyphLayout;
use crate::metal_view::MetalView;
use crate::selector::SelectorWidget;
use crate::selector_overlay::{
    calculate_overlay_geometry, SelectorGlyphBuffer,
};

use super::constants::Uniforms;
use super::scissor::{full_viewport_scissor_rect, selector_list_scissor_rect};
use super::Renderer;

impl Renderer {
    /// Draws the selector overlay panel
    // Chunk: docs/chunks/selector_rendering - Selector overlay rendering
    ///
    /// This method renders the selector as a floating panel overlay on top of
    /// the editor content. The panel contains:
    /// - Background rect
    /// - Query row with blinking cursor
    /// - Separator line
    /// - Item list with selection highlight
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `widget` - The selector widget state
    /// * `cursor_visible` - Whether to render the query cursor
    pub(super) fn draw_selector_overlay(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        widget: &SelectorWidget,
        cursor_visible: bool,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let line_height = self.font.metrics.line_height as f32;

        // Calculate overlay geometry
        let geometry = calculate_overlay_geometry(
            view_width,
            view_height,
            line_height,
            widget.items().len(),
        );

        // Ensure selector buffer is initialized
        if self.selector_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.selector_buffer = Some(SelectorGlyphBuffer::new(layout));
        }

        // Update the selector buffer with current widget state
        let selector_buffer = self.selector_buffer.as_mut().unwrap();
        selector_buffer.update_from_widget(
            &self.device,
            &self.atlas,
            widget,
            &geometry,
            cursor_visible,
        );

        // Get buffers
        let vertex_buffer = match selector_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match selector_buffer.index_buffer() {
            Some(b) => b,
            None => return,
        };

        // Set the render pipeline state (same pipeline as text rendering)
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

        // Chunk: docs/chunks/renderer_styled_content - Per-vertex colors, no per-draw uniforms needed
        // Chunk: docs/chunks/selector_list_clipping - Reordered draws for scissor rect clipping
        // With per-vertex colors, we draw all selector quads in order with no uniform changes.
        // Draw order: Background, Separator, Query Text, Query Cursor (unclipped),
        // then Selection Highlight and Item Text (clipped to list region).

        // ==================== Draw Background ====================
        let bg_range = selector_buffer.background_range();
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

        // ==================== Draw Separator Line ====================
        let sep_range = selector_buffer.separator_range();
        if !sep_range.is_empty() {
            let index_offset = sep_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    sep_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Query Text ====================
        let query_range = selector_buffer.query_text_range();
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

        // ==================== Draw Query Cursor ====================
        let cursor_range = selector_buffer.query_cursor_range();
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

        // Chunk: docs/chunks/selector_list_clipping - Apply scissor rect for list region
        // Clip selection highlight and item text to the list region, preventing
        // fractionally-scrolled items from bleeding into query/separator area.
        let list_scissor = selector_list_scissor_rect(&geometry, view_width, view_height);
        encoder.setScissorRect(list_scissor);

        // ==================== Draw Selection Highlight (clipped) ====================
        let sel_range = selector_buffer.selection_range();
        if !sel_range.is_empty() {
            let index_offset = sel_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    sel_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // ==================== Draw Item Text (clipped) ====================
        let item_range = selector_buffer.item_text_range();
        if !item_range.is_empty() {
            let index_offset = item_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    item_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Chunk: docs/chunks/selector_list_clipping - Reset scissor for subsequent rendering
        // Restore full viewport scissor so other render passes are not clipped.
        let full_scissor = full_viewport_scissor_rect(view_width, view_height);
        encoder.setScissorRect(full_scissor);
    }

    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog rendering (draw method)
    /// Draws the confirm dialog overlay.
    ///
    /// The dialog is centered in the viewport and shows:
    /// - A semi-transparent background panel
    /// - A prompt message
    /// - Cancel and Abandon buttons with selection highlight
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `dialog` - The confirm dialog state
    pub(super) fn draw_confirm_dialog(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        dialog: &ConfirmDialog,
    ) {
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let line_height = self.font.metrics.line_height as f32;
        let glyph_width = self.font.metrics.advance_width as f32;

        // Calculate dialog geometry
        // Chunk: docs/chunks/generic_yes_no_modal - Pass dialog reference for dynamic labels
        let geometry = calculate_confirm_dialog_geometry(
            view_width,
            view_height,
            line_height,
            glyph_width,
            dialog,
        );

        // Ensure confirm dialog buffer is initialized
        if self.confirm_dialog_buffer.is_none() {
            let layout = GlyphLayout::from_metrics(&self.font.metrics);
            self.confirm_dialog_buffer = Some(ConfirmDialogGlyphBuffer::new(layout));
        }

        // Update the confirm dialog buffer with current content
        let confirm_dialog_buffer = self.confirm_dialog_buffer.as_mut().unwrap();
        confirm_dialog_buffer.update(
            &self.device,
            &self.atlas,
            dialog,
            &geometry,
        );

        // Get buffers
        let vertex_buffer = match confirm_dialog_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match confirm_dialog_buffer.index_buffer() {
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

        // Draw panel background
        let panel_range = confirm_dialog_buffer.panel_range();
        if !panel_range.is_empty() {
            let index_offset = panel_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    panel_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw Cancel button background
        let cancel_bg_range = confirm_dialog_buffer.cancel_bg_range();
        if !cancel_bg_range.is_empty() {
            let index_offset = cancel_bg_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    cancel_bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw Abandon button background
        let abandon_bg_range = confirm_dialog_buffer.abandon_bg_range();
        if !abandon_bg_range.is_empty() {
            let index_offset = abandon_bg_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    abandon_bg_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw prompt text
        let prompt_range = confirm_dialog_buffer.prompt_range();
        if !prompt_range.is_empty() {
            let index_offset = prompt_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    prompt_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw Cancel button text
        let cancel_text_range = confirm_dialog_buffer.cancel_text_range();
        if !cancel_text_range.is_empty() {
            let index_offset = cancel_text_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    cancel_text_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }

        // Draw Abandon button text
        let abandon_text_range = confirm_dialog_buffer.abandon_text_range();
        if !abandon_text_range.is_empty() {
            let index_offset = abandon_text_range.start * std::mem::size_of::<u32>();
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    abandon_text_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    index_offset,
                );
            }
        }
    }
}
