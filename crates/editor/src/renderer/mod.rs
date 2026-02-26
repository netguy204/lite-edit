// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/text_selection_rendering - Selection highlight rendering
// Chunk: docs/chunks/selector_rendering - Selector overlay rendering
// Chunk: docs/chunks/line_wrap_rendering - Soft line wrapping support
// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
// Chunk: docs/chunks/renderer_decomposition - Module decomposition for maintainability
//!
//! Metal rendering pipeline
//!
//! This module provides the core Metal rendering functionality.
//! It clears the surface to a dark editor background color and renders
//! text using a glyph atlas and textured quads.
//!
//! The renderer manages a Viewport for determining which buffer lines are visible
//! and supports rendering from a TextBuffer with cursor display. Long lines are
//! soft-wrapped at the viewport width boundary.
//!
//! When a selector (e.g., file picker, command palette) is active, the renderer
//! draws an overlay panel on top of the editor content.
//!
//! The left rail (workspace tiles) is always visible on the left edge, shifting
//! the content area to the right.
//!
//! ## Module Organization
//!
//! The renderer is split into focused sub-modules:
//! - `constants` - Color constants and uniform types
//! - `scissor` - Scissor rect helper functions
//! - `content` - Text buffer content rendering
//! - `tab_bar` - Tab bar rendering (global and per-pane)
//! - `left_rail` - Left rail (workspace tiles) rendering
//! - `overlay` - Selector and confirm dialog overlays
//! - `find_strip` - Find-in-file strip rendering
//! - `panes` - Multi-pane layout rendering
//! - `welcome` - Welcome screen rendering

mod constants;
mod content;
mod find_strip;
mod left_rail;
mod overlay;
mod panes;
mod scissor;
mod tab_bar;
mod welcome;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLDrawable,
    MTLLoadAction, MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLStoreAction,
};
use objc2_quartz_core::CAMetalDrawable;

// Subsystem: docs/subsystems/renderer - GPU-accelerated text and UI rendering
use crate::confirm_dialog::{ConfirmDialog, ConfirmDialogGlyphBuffer};
use crate::dirty_region::DirtyRegion;
use crate::font::Font;
use crate::glyph_atlas::GlyphAtlas;
use crate::glyph_buffer::GlyphBuffer;
use crate::highlighted_buffer::HighlightedBufferView;
use crate::left_rail::{LeftRailGlyphBuffer, RAIL_WIDTH};
use crate::metal_view::MetalView;
use crate::pane_frame_buffer::PaneFrameBuffer;
use crate::pane_layout::{calculate_pane_rects, PaneId, PaneRect};
use crate::selector::SelectorWidget;
// Chunk: docs/chunks/renderer_styled_content - Per-vertex colors, overlay colors now in vertices
// Chunk: docs/chunks/find_in_file - Find strip rendering
// Chunk: docs/chunks/find_strip_multi_pane - Pane-aware find strip rendering
use crate::selector_overlay::{FindStripGlyphBuffer, FindStripState, SelectorGlyphBuffer};
use crate::shader::GlyphPipeline;
// Chunk: docs/chunks/content_tab_bar - Content tab bar rendering
use crate::tab_bar::{TabBarGlyphBuffer, TAB_BAR_HEIGHT};
use crate::viewport::Viewport;
use crate::workspace::Editor;
use crate::wrap_layout::WrapLayout;
// Chunk: docs/chunks/renderer_polymorphic_buffer - Import BufferView for polymorphic rendering
use lite_edit_buffer::DirtyLines;

use constants::BACKGROUND_COLOR;
use scissor::{buffer_content_scissor_rect, full_viewport_scissor_rect};

// =============================================================================
// Renderer
// =============================================================================

/// The Metal renderer responsible for drawing to the surface
pub struct Renderer {
    /// The Metal command queue for submitting work
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    /// The font used for text rendering
    font: Font,
    /// The glyph atlas containing rasterized characters
    atlas: GlyphAtlas,
    /// The glyph vertex buffer manager
    glyph_buffer: GlyphBuffer,
    /// The compiled shader pipeline
    pipeline: GlyphPipeline,
    /// The device reference for buffer creation
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    /// The viewport for buffer-to-screen coordinate mapping
    viewport: Viewport,
    // Chunk: docs/chunks/renderer_polymorphic_buffer - Removed buffer: Option<TextBuffer>
    // The renderer no longer owns a buffer copy. Instead, it receives a &dyn BufferView
    // reference at render time from the active tab.
    /// Whether the cursor should be visible
    cursor_visible: bool,
    /// The glyph buffer for selector overlay rendering (lazy-initialized)
    selector_buffer: Option<SelectorGlyphBuffer>,
    /// The glyph buffer for left rail (workspace tiles) rendering (lazy-initialized)
    left_rail_buffer: Option<LeftRailGlyphBuffer>,
    // Chunk: docs/chunks/content_tab_bar - Tab bar rendering
    /// The glyph buffer for content tab bar rendering (lazy-initialized)
    tab_bar_buffer: Option<TabBarGlyphBuffer>,
    /// The glyph buffer for find strip rendering (lazy-initialized)
    find_strip_buffer: Option<FindStripGlyphBuffer>,
    // Chunk: docs/chunks/welcome_screen - Welcome screen rendering
    /// The glyph buffer for welcome screen rendering (lazy-initialized)
    welcome_screen_buffer: Option<crate::welcome_screen::WelcomeScreenGlyphBuffer>,
    // Chunk: docs/chunks/tiling_multi_pane_render - Pane frame rendering
    /// The buffer for pane dividers and focus borders (lazy-initialized)
    pane_frame_buffer: Option<PaneFrameBuffer>,
    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog rendering
    /// The glyph buffer for confirm dialog rendering (lazy-initialized)
    confirm_dialog_buffer: Option<ConfirmDialogGlyphBuffer>,
    /// Current viewport width in pixels (for wrap layout calculation)
    viewport_width_px: f32,
    // Chunk: docs/chunks/wrap_click_offset - Content width for consistent wrap calculation
    /// Content area width in pixels (viewport_width - RAIL_WIDTH).
    ///
    /// This is the width available for text rendering, excluding the left rail.
    /// Both the renderer and click handler must use this same value when creating
    /// WrapLayout instances to ensure consistent `cols_per_row` calculations.
    content_width_px: f32,
    // Chunk: docs/chunks/invalidation_separation - Cached pane layout
    /// Cached pane rectangles from the last layout calculation.
    /// Only recomputed when Layout invalidation is signaled.
    cached_pane_rects: Vec<PaneRect>,
    /// Focused pane ID from the last layout calculation.
    cached_focused_pane_id: PaneId,
    /// Whether the cached pane rects are valid (false until first layout)
    pane_rects_valid: bool,
    // Chunk: docs/chunks/invalidation_separation - Perf instrumentation counters
    /// Counter for frames where layout recalculation was skipped
    #[cfg(feature = "perf-instrumentation")]
    layout_recalc_skipped: usize,
    /// Counter for frames where layout recalculation was performed
    #[cfg(feature = "perf-instrumentation")]
    layout_recalc_performed: usize,
}

impl Renderer {
    /// Creates a new renderer using the device from the given MetalView
    pub fn new(view: &MetalView) -> Self {
        let device = view.device();

        // Create the command queue
        let command_queue = device
            .newCommandQueue()
            .expect("Failed to create Metal command queue");

        // Get the scale factor for proper glyph sizing
        let scale_factor = view.scale_factor();

        // Load the bundled Intel One Mono font at the appropriate scale
        const FONT_DATA: &[u8] = include_bytes!("../../../../resources/IntelOneMono-Regular.ttf");
        let font = Font::from_data(FONT_DATA, 14.0, scale_factor);

        // Create the glyph atlas (pre-populates ASCII)
        let atlas = GlyphAtlas::new(device, &font);

        // Create the glyph buffer
        let glyph_buffer = GlyphBuffer::new(&font.metrics);

        // Create the shader pipeline
        let pipeline = GlyphPipeline::new(device);

        // Clone the device for later use
        // We need to use unsafe since the MTLDevice trait doesn't have Clone
        // Actually we should retain a reference to it
        extern "C" {
            fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
        }
        let device_ptr = unsafe { MTLCreateSystemDefaultDevice() };
        let device_retained =
            unsafe { Retained::from_raw(device_ptr).expect("Failed to get device") };

        // Create the viewport with the font's line height
        let viewport = Viewport::new(font.metrics.line_height as f32);

        // Get initial viewport width from view (will be updated on resize)
        let frame = view.frame();
        let scale = view.scale_factor();
        let viewport_width_px = (frame.size.width * scale) as f32;
        // Chunk: docs/chunks/wrap_click_offset - Initialize content width
        let content_width_px = (viewport_width_px - RAIL_WIDTH).max(0.0);

        Self {
            command_queue,
            font,
            atlas,
            glyph_buffer,
            pipeline,
            device: device_retained,
            viewport,
            // Chunk: docs/chunks/renderer_polymorphic_buffer - No longer owns buffer
            cursor_visible: true,
            selector_buffer: None,
            left_rail_buffer: None,
            tab_bar_buffer: None,
            find_strip_buffer: None,
            welcome_screen_buffer: None,
            pane_frame_buffer: None,
            confirm_dialog_buffer: None,
            viewport_width_px,
            content_width_px,
            // Chunk: docs/chunks/invalidation_separation - Initialize cached pane layout
            cached_pane_rects: Vec::new(),
            cached_focused_pane_id: 0,
            pane_rects_valid: false,
            #[cfg(feature = "perf-instrumentation")]
            layout_recalc_skipped: 0,
            #[cfg(feature = "perf-instrumentation")]
            layout_recalc_performed: 0,
        }
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Removed set_buffer, buffer_mut, buffer methods
    // The renderer no longer owns a buffer copy. BufferView is passed at render time.

    /// Returns a mutable reference to the viewport
    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.viewport
    }

    /// Returns a reference to the viewport
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Returns the font metrics
    pub fn font_metrics(&self) -> crate::font::FontMetrics {
        self.font.metrics
    }

    /// Returns the current viewport width in pixels
    pub fn viewport_width_px(&self) -> f32 {
        self.viewport_width_px
    }

    // Chunk: docs/chunks/line_wrap_rendering - Create WrapLayout for hit-testing
    // Chunk: docs/chunks/wrap_click_offset - Use content_width_px for consistent cols_per_row
    /// Creates a WrapLayout for the current content width and font metrics.
    ///
    /// This is used by hit-testing code to convert screen positions to buffer positions.
    /// The content width (viewport - RAIL_WIDTH) is used to ensure the same `cols_per_row`
    /// value is computed here as in the rendering code, preventing click offset errors
    /// on continuation rows.
    pub fn wrap_layout(&self) -> WrapLayout {
        WrapLayout::new(self.content_width_px, &self.font.metrics)
    }

    /// Updates the viewport size based on window dimensions
    ///
    /// Call this when the window resizes. Both width and height are needed
    /// for line wrapping calculations.
    ///
    /// Note: The renderer's viewport scroll offset is synced from EditorState
    /// before each render via `set_scroll_offset_px`, so we don't need to
    /// clamp here. We pass usize::MAX as the buffer line count to prevent
    /// any spurious clamping until the sync occurs.
    // Chunk: docs/chunks/resize_click_alignment - Viewport update_size now takes line count
    // Chunk: docs/chunks/wrap_click_offset - Update content_width_px on resize
    // Chunk: docs/chunks/invalidation_separation - Invalidate pane layout on resize
    pub fn update_viewport_size(&mut self, window_width: f32, window_height: f32) {
        // Use usize::MAX since scroll is synced externally from EditorState
        self.viewport.update_size(window_height, usize::MAX);
        self.viewport_width_px = window_width;
        // Content width is viewport minus the left rail
        self.content_width_px = (window_width - RAIL_WIDTH).max(0.0);
        // Viewport size change invalidates cached pane rects
        self.pane_rects_valid = false;
    }

    // Chunk: docs/chunks/invalidation_separation - Layout cache invalidation
    /// Marks the cached pane rects as invalid, forcing recalculation on next render.
    ///
    /// Call this when Layout invalidation is signaled. The renderer will
    /// recompute pane rects on the next frame.
    pub fn invalidate_pane_layout(&mut self) {
        self.pane_rects_valid = false;
    }

    // Chunk: docs/chunks/invalidation_separation - Perf instrumentation
    /// Returns the layout skip rate (skipped / total frames).
    ///
    /// This measures how effectively the invalidation system is avoiding
    /// unnecessary layout recalculations. A higher rate means more frames
    /// are using cached pane rects. Target: >90% during normal editing.
    #[cfg(feature = "perf-instrumentation")]
    pub fn layout_skip_rate(&self) -> f64 {
        let total = self.layout_recalc_skipped + self.layout_recalc_performed;
        if total == 0 {
            0.0
        } else {
            self.layout_recalc_skipped as f64 / total as f64
        }
    }

    /// Returns the raw layout recalc counters (skipped, performed).
    #[cfg(feature = "perf-instrumentation")]
    pub fn layout_recalc_counters(&self) -> (usize, usize) {
        (self.layout_recalc_skipped, self.layout_recalc_performed)
    }

    /// Sets cursor visibility
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    /// Takes the last styled_line timing from the glyph buffer (perf-instrumentation only).
    #[cfg(feature = "perf-instrumentation")]
    pub fn take_styled_line_timing(&mut self) -> Option<(std::time::Duration, usize)> {
        self.glyph_buffer.take_styled_line_timing()
    }

    // Chunk: docs/chunks/workspace_model - Content area x offset for left rail
    /// Sets the content area horizontal offset.
    ///
    /// When the left rail is visible, pass `RAIL_WIDTH` to shift all editor content
    /// to the right, making room for the workspace tiles on the left.
    pub fn set_content_x_offset(&mut self, offset: f32) {
        self.glyph_buffer.set_x_offset(offset);
    }

    // Chunk: docs/chunks/content_tab_bar - Content area y offset for tab bar
    /// Sets the content area vertical offset.
    ///
    /// When the tab bar is visible, pass `TAB_BAR_HEIGHT` to shift all editor content
    /// down, making room for the tab bar at the top.
    pub fn set_content_y_offset(&mut self, offset: f32) {
        self.glyph_buffer.set_y_offset(offset);
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Accept line_count parameter
    /// Converts buffer-space DirtyLines to screen-space DirtyRegion
    ///
    /// This is used in the drain-all-then-render loop to determine what
    /// portion of the screen needs re-rendering.
    ///
    /// # Arguments
    /// * `dirty_lines` - The dirty lines from buffer mutations
    /// * `line_count` - Total number of lines in the buffer
    pub fn apply_mutation(&self, dirty_lines: &DirtyLines, line_count: usize) -> DirtyRegion {
        self.viewport.dirty_lines_to_region(dirty_lines, line_count)
    }

    // Chunk: docs/chunks/styled_line_cache - Cache management methods
    /// Invalidates cached styled lines based on dirty line information.
    ///
    /// Call this before rendering when buffer content has changed. The dirty
    /// lines should come from `BufferView::take_dirty()` on the active buffer.
    /// This ensures that modified lines are recomputed during the next render
    /// while unchanged lines are served from cache.
    pub fn invalidate_styled_lines(&mut self, dirty: &DirtyLines) {
        self.glyph_buffer.invalidate_styled_lines(dirty);
    }

    /// Clears the styled line cache entirely.
    ///
    /// Call this when switching to a different buffer (tab change) to ensure
    /// stale cache entries from the previous buffer don't cause visual artifacts.
    pub fn clear_styled_line_cache(&mut self) {
        self.glyph_buffer.clear_styled_line_cache();
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Legacy method, not used with workspace model
    /// Renders based on dirty region (legacy method - use render_with_editor instead)
    ///
    /// For now, any dirty region triggers a full redraw. This is acceptable
    /// because full viewport redraws are <1ms (per H3 investigation).
    /// The dirty region tracking is in place for future optimization.
    ///
    /// Note: This method does NOT update the glyph buffer - the caller must
    /// pass a BufferView to update_glyph_buffer before calling this method,
    /// or use render_with_editor which handles this automatically.
    #[allow(dead_code)]
    pub fn render_dirty(&mut self, view: &MetalView, dirty: &DirtyRegion) {
        match dirty {
            DirtyRegion::None => {
                // No redraw needed
            }
            DirtyRegion::FullViewport | DirtyRegion::Lines { .. } => {
                // Render whatever is in the glyph buffer
                self.render(view);
            }
        }
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Legacy method, not used with workspace model
    /// Renders a frame to the given MetalView (legacy method - use render_with_editor instead)
    ///
    /// This clears the surface to the background color and renders
    /// any text content that has been set in the glyph buffer.
    ///
    /// Note: This method does NOT update the glyph buffer - the caller must
    /// pass a BufferView to update_glyph_buffer before calling this method,
    /// or use render_with_editor which handles this automatically.
    #[allow(dead_code)]
    pub fn render(&mut self, view: &MetalView) {
        let metal_layer = view.metal_layer();

        // Get the next drawable from the layer
        // This blocks until a drawable is available
        let drawable = match metal_layer.nextDrawable() {
            Some(d) => d,
            None => {
                eprintln!("Failed to get next drawable");
                return;
            }
        };

        // Create a render pass descriptor
        let render_pass_descriptor = MTLRenderPassDescriptor::new();

        // Configure the color attachment
        let color_attachments = render_pass_descriptor.colorAttachments();
        let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(0) };

        // Set the drawable's texture as the render target
        color_attachment.setTexture(Some(drawable.texture().as_ref()));

        // Clear to our background color
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setClearColor(BACKGROUND_COLOR);

        // Store the result
        color_attachment.setStoreAction(MTLStoreAction::Store);

        // Create a command buffer
        let command_buffer = match self.command_queue.commandBuffer() {
            Some(cb) => cb,
            None => {
                eprintln!("Failed to create command buffer");
                return;
            }
        };

        // Create a render command encoder
        let encoder =
            match command_buffer.renderCommandEncoderWithDescriptor(&render_pass_descriptor) {
                Some(e) => e,
                None => {
                    eprintln!("Failed to create render command encoder");
                    return;
                }
            };

        // Render text if we have content
        if self.glyph_buffer.index_count() > 0 {
            self.render_text(&encoder, view);
        }

        // End encoding
        encoder.endEncoding();

        // Present the drawable
        // Cast CAMetalDrawable to MTLDrawable for presentation
        let mtl_drawable: &ProtocolObject<dyn MTLDrawable> = ProtocolObject::from_ref(&*drawable);
        command_buffer.presentDrawable(mtl_drawable);

        // Commit the command buffer (submits work to GPU)
        command_buffer.commit();

        // Note: We don't wait for completion here. The GPU will execute asynchronously
        // and the drawable will be presented at the next vsync.
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Legacy method, not used with workspace model
    /// Renders a frame with an optional selector overlay (legacy method - use render_with_editor instead)
    ///
    /// This is a legacy entry point that has been superseded by render_with_editor.
    /// It renders the editor content first, then overlays the selector panel if one is provided.
    ///
    /// # Arguments
    /// * `view` - The Metal view to render to
    /// * `selector` - Optional selector widget to render as an overlay
    /// * `selector_cursor_visible` - Whether the selector's query cursor should be visible
    ///
    /// # Dirty Region Contract
    /// When the selector opens, closes, or its state changes (query, selected_index),
    /// the caller must mark `DirtyRegion::FullViewport` to ensure both the overlay
    /// and the editor content beneath are redrawn correctly.
    ///
    /// Note: This method does NOT update the glyph buffer - the caller must
    /// pass a BufferView to update_glyph_buffer before calling this method,
    /// or use render_with_editor which handles this automatically.
    #[allow(dead_code)]
    pub fn render_with_selector(
        &mut self,
        view: &MetalView,
        selector: Option<&SelectorWidget>,
        selector_cursor_visible: bool,
    ) {
        let metal_layer = view.metal_layer();

        // Get the next drawable from the layer
        let drawable = match metal_layer.nextDrawable() {
            Some(d) => d,
            None => {
                eprintln!("Failed to get next drawable");
                return;
            }
        };

        // Create a render pass descriptor
        let render_pass_descriptor = MTLRenderPassDescriptor::new();

        // Configure the color attachment
        let color_attachments = render_pass_descriptor.colorAttachments();
        let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(0) };

        // Set the drawable's texture as the render target
        color_attachment.setTexture(Some(drawable.texture().as_ref()));

        // Clear to our background color
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setClearColor(BACKGROUND_COLOR);

        // Store the result
        color_attachment.setStoreAction(MTLStoreAction::Store);

        // Create a command buffer
        let command_buffer = match self.command_queue.commandBuffer() {
            Some(cb) => cb,
            None => {
                eprintln!("Failed to create command buffer");
                return;
            }
        };

        // Create a render command encoder
        let encoder =
            match command_buffer.renderCommandEncoderWithDescriptor(&render_pass_descriptor) {
                Some(e) => e,
                None => {
                    eprintln!("Failed to create render command encoder");
                    return;
                }
            };

        // Render editor text content first (background layer)
        if self.glyph_buffer.index_count() > 0 {
            self.render_text(&encoder, view);
        }

        // Render selector overlay on top if active
        if let Some(widget) = selector {
            self.draw_selector_overlay(&encoder, view, widget, selector_cursor_visible);
        }

        // End encoding
        encoder.endEncoding();

        // Present the drawable
        let mtl_drawable: &ProtocolObject<dyn MTLDrawable> = ProtocolObject::from_ref(&*drawable);
        command_buffer.presentDrawable(mtl_drawable);

        // Commit the command buffer
        command_buffer.commit();
    }

    // Chunk: docs/chunks/workspace_model - Left rail rendering
    /// Renders the editor with left rail (workspace tiles) and optional selector overlay.
    ///
    /// This is the primary entry point for rendering when the workspace model is active.
    /// It draws the left rail first, then the editor content (offset by RAIL_WIDTH),
    /// and finally the selector overlay if active.
    ///
    /// # Arguments
    /// * `view` - The Metal view to render to
    /// * `editor` - The Editor state containing workspace information
    /// * `selector` - Optional selector widget to render as an overlay
    /// * `selector_cursor_visible` - Whether the selector's query cursor should be visible
    /// * `find_strip` - Optional find strip state for rendering find-in-file UI
    // Chunk: docs/chunks/find_strip_multi_pane - Find strip parameter for pane-aware rendering
    pub fn render_with_editor(
        &mut self,
        view: &MetalView,
        editor: &Editor,
        selector: Option<&SelectorWidget>,
        selector_cursor_visible: bool,
        find_strip: Option<FindStripState<'_>>,
    ) {
        // Set content area offset to account for left rail and tab bar
        self.set_content_x_offset(RAIL_WIDTH);
        // Chunk: docs/chunks/content_tab_bar - Content area y offset for tab bar
        self.set_content_y_offset(TAB_BAR_HEIGHT);

        // Chunk: docs/chunks/renderer_polymorphic_buffer - Get BufferView from Editor
        // Chunk: docs/chunks/syntax_highlighting - Use HighlightedBufferView for syntax coloring
        // Chunk: docs/chunks/pane_scroll_isolation - Configure viewport before updating glyph buffer
        // Chunk: docs/chunks/pane_mirror_restore - Only update glyph buffer for single-pane mode
        // In multi-pane mode, render_pane handles glyph buffer updates for each pane.
        // Running this early update would waste work and could cause cache contamination.
        if let Some(ws) = editor.active_workspace() {
            // Only run early glyph buffer update in single-pane mode
            if ws.pane_root.pane_count() <= 1 {
                if let Some(tab) = ws.active_tab() {
                    // Chunk: docs/chunks/pane_scroll_isolation - Configure viewport for single-pane mode
                    // For single-pane mode, configure viewport from the active tab before rendering.
                    // This mirrors what render_pane does for multi-pane mode.
                    let frame = view.frame();
                    let scale = view.scale_factor();
                    let view_width = (frame.size.width * scale) as f32;
                    let view_height = (frame.size.height * scale) as f32;
                    let content_height = view_height - TAB_BAR_HEIGHT;
                    let content_width = view_width - RAIL_WIDTH;
                    self.configure_viewport_for_pane(&tab.viewport, content_height, content_width);

                    if tab.is_agent_tab() {
                        // AgentTerminal is a placeholder - get the actual buffer from workspace
                        if let Some(terminal) = ws.agent_terminal() {
                            self.update_glyph_buffer(terminal);
                        }
                    } else if let Some(text_buffer) = tab.as_text_buffer() {
                        // File tab: use HighlightedBufferView for syntax highlighting
                        let highlighted_view = HighlightedBufferView::new(
                            text_buffer,
                            tab.highlighter(),
                        );
                        self.update_glyph_buffer(&highlighted_view);
                    } else {
                        // Terminal or other buffer type
                        self.update_glyph_buffer(tab.buffer());
                    }
                }
            }
        }
        let metal_layer = view.metal_layer();

        // Get the next drawable from the layer
        let drawable = match metal_layer.nextDrawable() {
            Some(d) => d,
            None => {
                eprintln!("Failed to get next drawable");
                return;
            }
        };

        // Create a render pass descriptor
        let render_pass_descriptor = MTLRenderPassDescriptor::new();

        // Configure the color attachment
        let color_attachments = render_pass_descriptor.colorAttachments();
        let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(0) };

        // Set the drawable's texture as the render target
        color_attachment.setTexture(Some(drawable.texture().as_ref()));

        // Clear to our background color
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setClearColor(BACKGROUND_COLOR);

        // Store the result
        color_attachment.setStoreAction(MTLStoreAction::Store);

        // Create a command buffer
        let command_buffer = match self.command_queue.commandBuffer() {
            Some(cb) => cb,
            None => {
                eprintln!("Failed to create command buffer");
                return;
            }
        };

        // Create a render command encoder
        let encoder =
            match command_buffer.renderCommandEncoderWithDescriptor(&render_pass_descriptor) {
                Some(e) => e,
                None => {
                    eprintln!("Failed to create render command encoder");
                    return;
                }
            };

        // Chunk: docs/chunks/tab_bar_content_clip - Extract view dimensions for scissor rect
        // Get view dimensions early for scissor rect calculation
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;

        // Render left rail first (background layer)
        self.draw_left_rail(&encoder, view, editor);

        // Chunk: docs/chunks/tiling_multi_pane_render - Calculate pane rects for multi-pane rendering
        // Chunk: docs/chunks/invalidation_separation - Conditional pane rect calculation
        // Only recalculate pane rectangles when the cache is invalid (Layout invalidation)
        // or when the focused pane has changed. Content-only frames reuse cached rects.
        //
        // Note: We clone the cached rects to avoid borrow conflicts with &mut self methods
        // called later (render_pane, draw_pane_frames). The clone is cheap (~3-4 PaneRects).
        let pane_rects: Vec<PaneRect>;
        let focused_pane_id: PaneId;
        if let Some(ws) = editor.active_workspace() {
            // Check if we need to recalculate
            let needs_recalc = !self.pane_rects_valid
                || ws.active_pane_id != self.cached_focused_pane_id;

            if needs_recalc {
                // Bounds for the pane area: starts after left rail
                // Note: For multi-pane, each pane has its own tab bar, so y=0
                let bounds = (
                    RAIL_WIDTH,               // x: after left rail
                    0.0,                      // y: top of window
                    view_width - RAIL_WIDTH,  // width: remaining horizontal space
                    view_height,              // height: full height
                );
                self.cached_pane_rects = calculate_pane_rects(bounds, &ws.pane_root);
                self.cached_focused_pane_id = ws.active_pane_id;
                self.pane_rects_valid = true;
                #[cfg(feature = "perf-instrumentation")]
                {
                    self.layout_recalc_performed += 1;
                }
            } else {
                #[cfg(feature = "perf-instrumentation")]
                {
                    self.layout_recalc_skipped += 1;
                }
            }
            // Clone to avoid borrow conflicts with &mut self methods below
            pane_rects = self.cached_pane_rects.clone();
            focused_pane_id = ws.active_pane_id;
        } else {
            // No active workspace - empty rects
            pane_rects = Vec::new();
            focused_pane_id = 0;
        }

        // Chunk: docs/chunks/tiling_multi_pane_render - Multi-pane or single-pane rendering
        if pane_rects.len() <= 1 {
            // Single-pane case: render as before (global tab bar, no dividers)
            // Chunk: docs/chunks/content_tab_bar - Draw tab bar after left rail
            self.draw_tab_bar(&encoder, view, editor);

            // Chunk: docs/chunks/tab_bar_content_clip - Clip buffer content to area below tab bar
            let content_scissor = buffer_content_scissor_rect(TAB_BAR_HEIGHT, view_width, view_height);
            encoder.setScissorRect(content_scissor);

            // Chunk: docs/chunks/welcome_screen - Welcome screen or normal buffer rendering
            if editor.should_show_welcome_screen() {
                let scroll = editor.welcome_scroll_offset_px();
                self.draw_welcome_screen(&encoder, view, scroll);
            } else {
                // Render editor text content (offset by RAIL_WIDTH and TAB_BAR_HEIGHT)
                if self.glyph_buffer.index_count() > 0 {
                    self.render_text(&encoder, view);
                }
            }

            // Chunk: docs/chunks/find_strip_multi_pane - Find strip rendering in single-pane mode
            // Draw find strip at the bottom of the viewport (full width)
            if let Some(ref find_state) = find_strip {
                // Reset scissor for find strip (it draws over the content area)
                let full_scissor = full_viewport_scissor_rect(view_width, view_height);
                encoder.setScissorRect(full_scissor);
                self.draw_find_strip(
                    &encoder,
                    view,
                    find_state.query,
                    find_state.cursor_col,
                    find_state.cursor_visible,
                );
            }
        } else {
            // Multi-pane case: render each pane independently
            if let Some(ws) = editor.active_workspace() {
                for pane_rect in &pane_rects {
                    self.render_pane(&encoder, view, ws, pane_rect, view_width, view_height);
                }
            }

            // Chunk: docs/chunks/find_strip_multi_pane - Find strip rendering in multi-pane mode
            // Draw find strip within the focused pane's bounds
            if let Some(ref find_state) = find_strip {
                // Find the focused pane's rect
                if let Some(focused_rect) = pane_rects.iter().find(|r| r.pane_id == focused_pane_id) {
                    self.draw_find_strip_in_pane(
                        &encoder,
                        view,
                        find_state.query,
                        find_state.cursor_col,
                        find_state.cursor_visible,
                        focused_rect,
                        view_width,
                        view_height,
                    );
                }
            }

            // Reset scissor to full viewport before drawing frames on top
            let full_scissor = full_viewport_scissor_rect(view_width, view_height);
            encoder.setScissorRect(full_scissor);

            // Draw pane dividers and focus border
            self.draw_pane_frames(&encoder, view, &pane_rects, focused_pane_id);
        }

        // Chunk: docs/chunks/tab_bar_content_clip - Reset scissor for selector overlay
        // Restore full viewport scissor so selector overlay renders correctly.
        let full_scissor = full_viewport_scissor_rect(view_width, view_height);
        encoder.setScissorRect(full_scissor);

        // Render selector overlay on top if active
        if let Some(widget) = selector {
            self.draw_selector_overlay(&encoder, view, widget, selector_cursor_visible);
        }

        // End encoding
        encoder.endEncoding();

        // Present the drawable
        let mtl_drawable: &ProtocolObject<dyn MTLDrawable> = ProtocolObject::from_ref(&*drawable);
        command_buffer.presentDrawable(mtl_drawable);

        // Commit the command buffer
        command_buffer.commit();
    }

    // Chunk: docs/chunks/workspace_model - Content area offset
    /// Returns the left rail width for content area offset.
    ///
    /// The content area should be offset by this amount to avoid overlapping
    /// with the left rail.
    pub fn left_rail_width(&self) -> f32 {
        RAIL_WIDTH
    }

    // Chunk: docs/chunks/content_tab_bar - Tab bar height
    /// Returns the tab bar height.
    ///
    /// The content area should be offset vertically by this amount when the
    /// active workspace has tabs.
    pub fn tab_bar_height(&self) -> f32 {
        TAB_BAR_HEIGHT
    }

    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog rendering (entry point)
    /// Renders the editor with a confirm dialog overlay.
    ///
    /// This is similar to `render_with_editor` but adds a modal confirm dialog
    /// on top of the buffer content. The dialog has a semi-transparent backdrop
    /// and centered panel with prompt and buttons.
    ///
    /// # Arguments
    /// * `view` - The Metal view to render to
    /// * `editor` - The Editor state containing workspace information
    /// * `dialog` - Optional confirm dialog to render as an overlay
    pub fn render_with_confirm_dialog(
        &mut self,
        view: &MetalView,
        editor: &Editor,
        dialog: Option<&ConfirmDialog>,
    ) {
        // Set content area offset to account for left rail and tab bar
        self.set_content_x_offset(RAIL_WIDTH);
        self.set_content_y_offset(TAB_BAR_HEIGHT);

        // Chunk: docs/chunks/pane_mirror_restore - Only update glyph buffer for single-pane mode
        // In multi-pane mode, render_pane handles glyph buffer updates for each pane.
        // Running this early update would waste work and could cause cache contamination.
        if let Some(ws) = editor.active_workspace() {
            // Only run early glyph buffer update in single-pane mode
            if ws.pane_root.pane_count() <= 1 {
                if let Some(tab) = ws.active_tab() {
                    // For single-pane mode, configure viewport from the active tab before rendering.
                    let frame = view.frame();
                    let scale = view.scale_factor();
                    let view_width = (frame.size.width * scale) as f32;
                    let view_height = (frame.size.height * scale) as f32;
                    let content_height = view_height - TAB_BAR_HEIGHT;
                    let content_width = view_width - RAIL_WIDTH;
                    self.configure_viewport_for_pane(&tab.viewport, content_height, content_width);

                    if tab.is_agent_tab() {
                        if let Some(terminal) = ws.agent_terminal() {
                            self.update_glyph_buffer(terminal);
                        }
                    } else if let Some(text_buffer) = tab.as_text_buffer() {
                        let highlighted_view = HighlightedBufferView::new(
                            text_buffer,
                            tab.highlighter(),
                        );
                        self.update_glyph_buffer(&highlighted_view);
                    } else {
                        self.update_glyph_buffer(tab.buffer());
                    }
                }
            }
        }
        let metal_layer = view.metal_layer();

        // Get the next drawable from the layer
        let drawable = match metal_layer.nextDrawable() {
            Some(d) => d,
            None => {
                eprintln!("Failed to get next drawable");
                return;
            }
        };

        // Create a render pass descriptor
        let render_pass_descriptor = MTLRenderPassDescriptor::new();

        // Configure the color attachment
        let color_attachments = render_pass_descriptor.colorAttachments();
        let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(0) };

        // Set the drawable's texture as the render target
        color_attachment.setTexture(Some(drawable.texture().as_ref()));

        // Clear to our background color
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setClearColor(BACKGROUND_COLOR);

        // Store the result
        color_attachment.setStoreAction(MTLStoreAction::Store);

        // Create a command buffer
        let command_buffer = match self.command_queue.commandBuffer() {
            Some(cb) => cb,
            None => {
                eprintln!("Failed to create command buffer");
                return;
            }
        };

        // Create a render command encoder
        let encoder =
            match command_buffer.renderCommandEncoderWithDescriptor(&render_pass_descriptor) {
                Some(e) => e,
                None => {
                    eprintln!("Failed to create render command encoder");
                    return;
                }
            };

        // Get view dimensions for scissor rect
        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;

        // Render left rail first (background layer)
        self.draw_left_rail(&encoder, view, editor);

        // Chunk: docs/chunks/invalidation_separation - Conditional pane rect calculation
        // Calculate pane rectangles for the active workspace (with caching)
        // Clone cached rects to avoid borrow conflicts with &mut self methods below.
        let pane_rects: Vec<PaneRect>;
        let focused_pane_id: PaneId;
        if let Some(ws) = editor.active_workspace() {
            // Check if we need to recalculate
            let needs_recalc = !self.pane_rects_valid
                || ws.active_pane_id != self.cached_focused_pane_id;

            if needs_recalc {
                let bounds = (
                    RAIL_WIDTH,
                    0.0,
                    view_width - RAIL_WIDTH,
                    view_height,
                );
                self.cached_pane_rects = calculate_pane_rects(bounds, &ws.pane_root);
                self.cached_focused_pane_id = ws.active_pane_id;
                self.pane_rects_valid = true;
                #[cfg(feature = "perf-instrumentation")]
                {
                    self.layout_recalc_performed += 1;
                }
            } else {
                #[cfg(feature = "perf-instrumentation")]
                {
                    self.layout_recalc_skipped += 1;
                }
            }
            pane_rects = self.cached_pane_rects.clone();
            focused_pane_id = ws.active_pane_id;
        } else {
            // No active workspace - empty rects
            pane_rects = Vec::new();
            focused_pane_id = 0;
        }

        // Render panes (single or multi-pane)
        if pane_rects.len() <= 1 {
            // Single-pane case: render as before (global tab bar, no dividers)
            self.draw_tab_bar(&encoder, view, editor);

            let content_scissor = buffer_content_scissor_rect(TAB_BAR_HEIGHT, view_width, view_height);
            encoder.setScissorRect(content_scissor);

            if editor.should_show_welcome_screen() {
                let scroll = editor.welcome_scroll_offset_px();
                self.draw_welcome_screen(&encoder, view, scroll);
            } else {
                if self.glyph_buffer.index_count() > 0 {
                    self.render_text(&encoder, view);
                }
            }
        } else {
            // Multi-pane case: render each pane independently
            if let Some(ws) = editor.active_workspace() {
                for pane_rect in &pane_rects {
                    self.render_pane(&encoder, view, ws, pane_rect, view_width, view_height);
                }
            }

            // Reset scissor to full viewport before drawing frames on top
            let full_scissor = full_viewport_scissor_rect(view_width, view_height);
            encoder.setScissorRect(full_scissor);

            // Draw pane dividers and focus border
            self.draw_pane_frames(&encoder, view, &pane_rects, focused_pane_id);
        }

        // Reset scissor for overlay
        let full_scissor = full_viewport_scissor_rect(view_width, view_height);
        encoder.setScissorRect(full_scissor);

        // Render confirm dialog overlay on top if present
        if let Some(dlg) = dialog {
            self.draw_confirm_dialog(&encoder, view, dlg);
        }

        // End encoding
        encoder.endEncoding();

        // Present the drawable
        let mtl_drawable: &ProtocolObject<dyn MTLDrawable> = ProtocolObject::from_ref(&*drawable);
        command_buffer.presentDrawable(mtl_drawable);

        // Commit the command buffer
        command_buffer.commit();
    }
}
