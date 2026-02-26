// Chunk: docs/chunks/renderer_decomposition - Pane rendering extracted from renderer.rs

//! Multi-pane layout rendering implementation.
//!
//! This module contains the methods for rendering pane layouts:
//! - Per-pane content rendering
//! - Pane frame rendering (dividers and focus borders)
//! - Viewport configuration for panes

use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLIndexType, MTLPrimitiveType, MTLRenderCommandEncoder,
};

use crate::highlighted_buffer::HighlightedBufferView;
use crate::metal_view::MetalView;
use crate::pane_frame_buffer::PaneFrameBuffer;
use crate::pane_layout::{PaneId, PaneRect};
use crate::tab_bar::TAB_BAR_HEIGHT;
use crate::viewport::Viewport;
use crate::workspace::Workspace;

use super::constants::{FOCUSED_PANE_BORDER_COLOR, PANE_DIVIDER_COLOR, Uniforms};
use super::scissor::{pane_content_scissor_rect, pane_scissor_rect};
use super::Renderer;

impl Renderer {
    // Chunk: docs/chunks/pane_scroll_isolation - Per-pane viewport configuration
    /// Configures the renderer's viewport for rendering a specific pane.
    ///
    /// This copies the scroll state from the tab's viewport and updates dimensions
    /// to match the pane's actual size. Must be called before `update_glyph_buffer_*`
    /// for each pane in multi-pane mode.
    ///
    /// # Arguments
    /// * `tab_viewport` - The viewport from the pane's active tab
    /// * `pane_content_height` - The pane's content area height (excluding tab bar)
    /// * `pane_width` - The pane's width
    pub(super) fn configure_viewport_for_pane(
        &mut self,
        tab_viewport: &Viewport,
        pane_content_height: f32,
        pane_width: f32,
    ) {
        // Copy scroll offset from tab (tab is authoritative)
        self.viewport.set_scroll_offset_px_unclamped(tab_viewport.scroll_offset_px());

        // Update visible lines for this pane's height
        // The tab's viewport already has correct clamping for its content
        let line_height = self.viewport.line_height();
        let visible_lines = if line_height > 0.0 {
            (pane_content_height / line_height).floor() as usize
        } else {
            0
        };
        self.viewport.set_visible_lines(visible_lines);

        // Update wrap width for this pane
        self.content_width_px = pane_width;
    }

    // =========================================================================
    // Pane Frame Rendering (Chunk: docs/chunks/tiling_multi_pane_render)
    // =========================================================================

    /// Draws pane divider lines and focus border.
    ///
    /// Divider lines appear between adjacent panes (1px).
    /// A focus border appears around the active pane when multiple panes exist (2px).
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `pane_rects` - The computed pane rectangles
    /// * `focused_pane_id` - The ID of the currently focused pane
    pub(super) fn draw_pane_frames(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        pane_rects: &[PaneRect],
        focused_pane_id: PaneId,
    ) {
        // Skip if only one pane (no dividers or focus border needed)
        if pane_rects.len() <= 1 {
            return;
        }

        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;

        // Ensure pane frame buffer is initialized
        if self.pane_frame_buffer.is_none() {
            self.pane_frame_buffer = Some(PaneFrameBuffer::new());
        }

        // Update the pane frame buffer
        let pane_frame_buffer = self.pane_frame_buffer.as_mut().unwrap();
        pane_frame_buffer.update(
            &self.device,
            pane_rects,
            focused_pane_id,
            &self.atlas,
            PANE_DIVIDER_COLOR,
            FOCUSED_PANE_BORDER_COLOR,
        );

        // Get buffers
        let vertex_buffer = match pane_frame_buffer.vertex_buffer() {
            Some(b) => b,
            None => return,
        };
        let index_buffer = match pane_frame_buffer.index_buffer() {
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

        // Draw divider lines (colors are baked into vertices)
        let divider_range = pane_frame_buffer.divider_range();
        if !divider_range.is_empty() {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    divider_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    divider_range.start * std::mem::size_of::<u32>(),
                );
            }
        }

        // Draw focus border (colors are baked into vertices)
        let focus_range = pane_frame_buffer.focus_border_range();
        if !focus_range.is_empty() {
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    focus_range.count,
                    MTLIndexType::UInt32,
                    index_buffer,
                    focus_range.start * std::mem::size_of::<u32>(),
                );
            }
        }
    }

    // Chunk: docs/chunks/tiling_multi_pane_render - Per-pane rendering
    /// Renders a single pane's content.
    ///
    /// This method handles:
    /// 1. Drawing the pane's tab bar at the top of the pane rect
    /// 2. Applying content scissor (below tab bar)
    /// 3. Drawing the pane's buffer content with correct offsets
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `workspace` - The workspace containing the pane
    /// * `pane_rect` - The rectangle for this pane
    /// * `view_width` - The viewport width
    /// * `view_height` - The viewport height
    pub(super) fn render_pane(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        workspace: &Workspace,
        pane_rect: &PaneRect,
        view_width: f32,
        view_height: f32,
    ) {
        // Get the pane
        let pane = match workspace.pane_root.get_pane(pane_rect.pane_id) {
            Some(p) => p,
            None => return,
        };

        // Skip empty panes
        if pane.tab_count() == 0 {
            return;
        }

        // Apply scissor for entire pane
        let pane_scissor = pane_scissor_rect(pane_rect, view_width, view_height);
        encoder.setScissorRect(pane_scissor);

        // Draw this pane's tab bar
        self.draw_pane_tab_bar(encoder, view, pane, pane_rect, view_width, view_height);

        // Apply content scissor (below tab bar)
        let content_scissor = pane_content_scissor_rect(pane_rect, TAB_BAR_HEIGHT, view_width, view_height);
        encoder.setScissorRect(content_scissor);

        // Get the active tab
        let tab = match pane.active_tab() {
            Some(t) => t,
            None => return,
        };

        // Check if welcome screen should be shown for this pane
        let is_focused = pane_rect.pane_id == workspace.active_pane_id;
        let should_show_welcome = is_focused
            && tab.kind == crate::workspace::TabKind::File
            && tab.as_text_buffer().map(|b| b.is_empty()).unwrap_or(false);

        if should_show_welcome {
            // Render welcome screen within pane bounds
            let scroll = tab.welcome_scroll_offset_px();
            self.draw_welcome_screen_in_pane(encoder, view, pane_rect, scroll);
        } else {
            // Set content offsets for this pane
            self.set_content_x_offset(pane_rect.x);
            self.set_content_y_offset(pane_rect.y + TAB_BAR_HEIGHT);

            // Chunk: docs/chunks/pane_scroll_isolation - Configure viewport for this pane
            // Copy tab's scroll state and update dimensions for pane size
            let pane_content_height = pane_rect.height - TAB_BAR_HEIGHT;
            self.configure_viewport_for_pane(&tab.viewport, pane_content_height, pane_rect.width);

            // Chunk: docs/chunks/cursor_blink_pane_focus - Only show blinking cursor in focused pane
            // Focused pane: cursor blinks (shows/hides based on self.cursor_visible)
            // Unfocused pane: static cursor (always visible) - provides clear visual feedback
            let pane_cursor_visible = if is_focused { self.cursor_visible } else { true };

            // Chunk: docs/chunks/pane_mirror_restore - Clear styled line cache between pane renders
            // The styled line cache is indexed by line number, not by pane. Without clearing
            // it between pane renders, a cached line from pane A (e.g., line 5) could be
            // incorrectly served when rendering pane B's line 5, causing content mirroring.
            self.clear_styled_line_cache();

            // Update glyph buffer from tab's buffer with pane-specific cursor visibility
            if tab.is_agent_tab() {
                if let Some(terminal) = workspace.agent_terminal() {
                    self.update_glyph_buffer_with_cursor_visible(terminal, pane_cursor_visible);
                }
            } else if let Some(text_buffer) = tab.as_text_buffer() {
                let highlighted_view = HighlightedBufferView::new(
                    text_buffer,
                    tab.highlighter(),
                );
                self.update_glyph_buffer_with_cursor_visible(&highlighted_view, pane_cursor_visible);
            } else {
                self.update_glyph_buffer_with_cursor_visible(tab.buffer(), pane_cursor_visible);
            }

            // Render text
            if self.glyph_buffer.index_count() > 0 {
                self.render_text(encoder, view);
            }
        }
    }
}
