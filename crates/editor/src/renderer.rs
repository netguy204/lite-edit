// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/text_selection_rendering - Selection highlight rendering
// Chunk: docs/chunks/selector_rendering - Selector overlay rendering
// Chunk: docs/chunks/line_wrap_rendering - Soft line wrapping support
// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
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

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLDrawable,
    MTLIndexType, MTLLoadAction, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLScissorRect, MTLStoreAction,
};
use objc2_quartz_core::CAMetalDrawable;

use crate::dirty_region::DirtyRegion;
use crate::font::Font;
use crate::glyph_atlas::GlyphAtlas;
use crate::glyph_buffer::{GlyphBuffer, GlyphLayout};
use crate::left_rail::{calculate_left_rail_geometry, status_color, LeftRailGlyphBuffer, RAIL_WIDTH};
use crate::metal_view::MetalView;
use crate::selector::SelectorWidget;
// Chunk: docs/chunks/renderer_styled_content - Per-vertex colors, overlay colors now in vertices
// Chunk: docs/chunks/find_in_file - Find strip rendering
use crate::selector_overlay::{
    calculate_find_strip_geometry, calculate_overlay_geometry, FindStripGlyphBuffer,
    OverlayGeometry, SelectorGlyphBuffer,
};
use crate::shader::GlyphPipeline;
// Chunk: docs/chunks/content_tab_bar - Content tab bar rendering
use crate::tab_bar::{
    calculate_tab_bar_geometry, tabs_from_workspace, TabBarGlyphBuffer, TAB_BAR_HEIGHT,
};
use crate::viewport::Viewport;
use crate::workspace::Editor;
use crate::wrap_layout::WrapLayout;
use lite_edit_buffer::{DirtyLines, TextBuffer};

// =============================================================================
// Background Color
// =============================================================================

/// The editor background color: #1e1e2e (Catppuccin Mocha base)
/// Converted to normalized RGB values
const BACKGROUND_COLOR: MTLClearColor = MTLClearColor {
    red: 0.118,   // 0x1e / 255
    green: 0.118, // 0x1e / 255
    blue: 0.180,  // 0x2e / 255
    alpha: 1.0,
};

/// The text foreground color: #cdd6f4 (Catppuccin Mocha text)
/// Stored as [R, G, B, A] for passing to the shader
const TEXT_COLOR: [f32; 4] = [
    0.804, // 0xcd / 255
    0.839, // 0xd6 / 255
    0.957, // 0xf4 / 255
    1.0,
];

// Chunk: docs/chunks/text_selection_rendering - Selection highlight color constant
/// The selection highlight color: #585b70 (Catppuccin Mocha surface2) at 40% alpha
/// This provides a visible background for selected text without overwhelming it.
const SELECTION_COLOR: [f32; 4] = [
    0.345, // 0x58 / 255
    0.357, // 0x5b / 255
    0.439, // 0x70 / 255
    0.4,   // 40% opacity
];

// Chunk: docs/chunks/line_wrap_rendering - Continuation row border color
/// The border color for continuation rows: black (solid)
/// This provides a subtle visual indicator that a line has wrapped.
const BORDER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

// =============================================================================
// Uniforms
// =============================================================================

/// Uniforms passed to the vertex shader
#[repr(C)]
struct Uniforms {
    /// Viewport size in pixels
    viewport_size: [f32; 2],
}

// =============================================================================
// Scissor Rect Helpers
// =============================================================================

// Chunk: docs/chunks/selector_list_clipping - Clip item list to panel bounds
/// Creates a scissor rect for clipping the selector item list.
///
/// The rect spans from `list_origin_y` to `panel_y + panel_height`,
/// clipped to the viewport bounds.
fn selector_list_scissor_rect(
    geometry: &OverlayGeometry,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    // Y coordinate: list_origin_y (top of list region)
    let y = (geometry.list_origin_y as usize).min(view_height as usize);

    // Height: from list_origin_y to panel bottom
    let bottom = geometry.panel_y + geometry.panel_height;
    let height = ((bottom - geometry.list_origin_y).max(0.0) as usize)
        .min((view_height as usize).saturating_sub(y));

    MTLScissorRect {
        x: 0,
        y,
        width: view_width as usize,
        height,
    }
}

// Chunk: docs/chunks/selector_list_clipping - Reset scissor for subsequent rendering
/// Creates a scissor rect covering the entire viewport.
fn full_viewport_scissor_rect(view_width: f32, view_height: f32) -> MTLScissorRect {
    MTLScissorRect {
        x: 0,
        y: 0,
        width: view_width as usize,
        height: view_height as usize,
    }
}

// Chunk: docs/chunks/tab_bar_content_clip - Clip buffer content below tab bar
/// Creates a scissor rect for clipping buffer content to the area below the tab bar.
///
/// The rect starts at `tab_bar_height` and extends to the bottom of the viewport,
/// preventing buffer content from bleeding into the tab bar region.
fn buffer_content_scissor_rect(
    tab_bar_height: f32,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    // Y coordinate: tab_bar_height (top of buffer region)
    let y = (tab_bar_height as usize).min(view_height as usize);

    // Height: from tab_bar_height to bottom of viewport
    let height = (view_height as usize).saturating_sub(y);

    MTLScissorRect {
        x: 0,
        y,
        width: view_width as usize,
        height,
    }
}

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
    /// The text buffer being edited (if any)
    buffer: Option<TextBuffer>,
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
    /// Current viewport width in pixels (for wrap layout calculation)
    viewport_width_px: f32,
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

        // Load the font at the appropriate scale
        // Using 14pt as the default font size
        let font = Font::new("Menlo-Regular", 14.0, scale_factor);

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

        Self {
            command_queue,
            font,
            atlas,
            glyph_buffer,
            pipeline,
            device: device_retained,
            viewport,
            buffer: None,
            cursor_visible: true,
            selector_buffer: None,
            left_rail_buffer: None,
            tab_bar_buffer: None,
            find_strip_buffer: None,
            viewport_width_px,
        }
    }

    /// Sets the text buffer to render
    ///
    /// This replaces any existing buffer and marks the full viewport dirty.
    pub fn set_buffer(&mut self, buffer: TextBuffer) {
        self.buffer = Some(buffer);
    }

    /// Returns a mutable reference to the text buffer, if any
    pub fn buffer_mut(&mut self) -> Option<&mut TextBuffer> {
        self.buffer.as_mut()
    }

    /// Returns a reference to the text buffer, if any
    pub fn buffer(&self) -> Option<&TextBuffer> {
        self.buffer.as_ref()
    }

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
    /// Creates a WrapLayout for the current viewport width and font metrics.
    ///
    /// This is used by hit-testing code to convert screen positions to buffer positions.
    pub fn wrap_layout(&self) -> WrapLayout {
        WrapLayout::new(self.viewport_width_px, &self.font.metrics)
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
    pub fn update_viewport_size(&mut self, window_width: f32, window_height: f32) {
        // Use usize::MAX since scroll is synced externally from EditorState
        self.viewport.update_size(window_height, usize::MAX);
        self.viewport_width_px = window_width;
    }

    /// Sets cursor visibility
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
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

    /// Converts buffer-space DirtyLines to screen-space DirtyRegion
    ///
    /// This is used in the drain-all-then-render loop to determine what
    /// portion of the screen needs re-rendering.
    pub fn apply_mutation(&self, dirty_lines: &DirtyLines) -> DirtyRegion {
        if let Some(buffer) = &self.buffer {
            self.viewport.dirty_lines_to_region(dirty_lines, buffer.line_count())
        } else {
            DirtyRegion::None
        }
    }

    /// Renders based on dirty region
    ///
    /// For now, any dirty region triggers a full redraw. This is acceptable
    /// because full viewport redraws are <1ms (per H3 investigation).
    /// The dirty region tracking is in place for future optimization.
    pub fn render_dirty(&mut self, view: &MetalView, dirty: &DirtyRegion) {
        match dirty {
            DirtyRegion::None => {
                // No redraw needed
            }
            DirtyRegion::FullViewport | DirtyRegion::Lines { .. } => {
                // Rebuild the glyph buffer and render
                self.update_glyph_buffer();
                self.render(view);
            }
        }
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Pass y_offset for smooth scrolling
    // Chunk: docs/chunks/line_wrap_rendering - Use WrapLayout for soft wrapping
    /// Updates the glyph buffer from the current buffer and viewport
    fn update_glyph_buffer(&mut self) {
        if let Some(buffer) = &self.buffer {
            // Get the fractional scroll offset for smooth scrolling
            let y_offset = self.viewport.scroll_fraction_px();

            // Create wrap layout for current viewport width
            let wrap_layout = WrapLayout::new(self.viewport_width_px, &self.font.metrics);

            // Use wrap-aware rendering
            self.glyph_buffer.update_from_buffer_with_wrap(
                &self.device,
                &self.atlas,
                buffer,
                &self.viewport,
                &wrap_layout,
                self.cursor_visible,
                y_offset,
            );
        }
    }

    /// Sets the text content to display
    ///
    /// # Arguments
    /// * `lines` - The text lines to render
    pub fn set_content(&mut self, lines: &[&str]) {
        self.glyph_buffer.update(&self.device, &self.atlas, lines);
    }

    /// Renders a frame to the given MetalView
    ///
    /// This clears the surface to the background color and renders
    /// any text content that has been set.
    ///
    /// If a TextBuffer is set, this method updates the glyph buffer
    /// from the buffer content before rendering.
    pub fn render(&mut self, view: &MetalView) {
        // Update glyph buffer from TextBuffer if available
        self.update_glyph_buffer();
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

    // Chunk: docs/chunks/text_selection_rendering - Three-pass draw with separate fragment color uniforms per quad category
    /// Renders the text content using the glyph pipeline
    ///
    /// Draws quads in three passes with different colors:
    /// 1. Selection highlight quads (SELECTION_COLOR)
    /// 2. Glyph quads (TEXT_COLOR)
    /// 3. Cursor quad (TEXT_COLOR)
    fn render_text(
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

    /// Renders a frame with an optional selector overlay
    ///
    /// This is the primary entry point when a selector (file picker, command palette)
    /// may be active. It renders the editor content first, then overlays the selector
    /// panel if one is provided.
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
    pub fn render_with_selector(
        &mut self,
        view: &MetalView,
        selector: Option<&SelectorWidget>,
        selector_cursor_visible: bool,
    ) {
        // Update glyph buffer from TextBuffer if available
        self.update_glyph_buffer();
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

    /// Draws the selector overlay panel
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
    fn draw_selector_overlay(
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
    pub fn render_with_editor(
        &mut self,
        view: &MetalView,
        editor: &Editor,
        selector: Option<&SelectorWidget>,
        selector_cursor_visible: bool,
    ) {
        // Set content area offset to account for left rail and tab bar
        self.set_content_x_offset(RAIL_WIDTH);
        // Chunk: docs/chunks/content_tab_bar - Content area y offset for tab bar
        self.set_content_y_offset(TAB_BAR_HEIGHT);

        // Update glyph buffer from TextBuffer if available
        self.update_glyph_buffer();
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

        // Chunk: docs/chunks/content_tab_bar - Draw tab bar after left rail
        // Render tab bar at top of content area
        self.draw_tab_bar(&encoder, view, editor);

        // Chunk: docs/chunks/tab_bar_content_clip - Clip buffer content to area below tab bar
        // Apply scissor rect to prevent buffer text from bleeding into tab bar region.
        let content_scissor = buffer_content_scissor_rect(TAB_BAR_HEIGHT, view_width, view_height);
        encoder.setScissorRect(content_scissor);

        // Render editor text content (offset by RAIL_WIDTH and TAB_BAR_HEIGHT)
        if self.glyph_buffer.index_count() > 0 {
            self.render_text(&encoder, view);
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
    fn draw_left_rail(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        editor: &Editor,
    ) {
        use crate::left_rail::{
            LABEL_COLOR, RAIL_BACKGROUND_COLOR, TILE_ACTIVE_COLOR, TILE_BACKGROUND_COLOR,
        };

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

        // Draw labels
        let label_range = left_rail_buffer.label_range();
        if !label_range.is_empty() {
            let color_ptr = NonNull::new(LABEL_COLOR.as_ptr() as *mut std::ffi::c_void).unwrap();
            unsafe {
                encoder.setFragmentBytes_length_atIndex(color_ptr, std::mem::size_of::<[f32; 4]>(), 0);
            }
            unsafe {
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

    // =========================================================================
    // Find Strip Rendering (Chunk: docs/chunks/find_in_file)
    // =========================================================================

    /// Renders the editor with left rail and find strip (when find-in-file is active).
    ///
    /// This is similar to `render_with_editor` but draws the find strip at the bottom
    /// instead of a selector overlay.
    ///
    /// # Arguments
    /// * `view` - The Metal view to render to
    /// * `editor` - The Editor state containing workspace information
    /// * `find_query` - The current find query text
    /// * `find_cursor_col` - The cursor column position in the query
    /// * `find_cursor_visible` - Whether the find strip cursor should be visible
    pub fn render_with_find_strip(
        &mut self,
        view: &MetalView,
        editor: &Editor,
        find_query: &str,
        find_cursor_col: usize,
        find_cursor_visible: bool,
    ) {
        // Set content area offset to account for left rail
        self.set_content_x_offset(RAIL_WIDTH);

        // Update glyph buffer from TextBuffer if available
        self.update_glyph_buffer();
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

        // Render left rail first (background layer)
        self.draw_left_rail(&encoder, view, editor);

        // Render editor text content (offset by RAIL_WIDTH)
        if self.glyph_buffer.index_count() > 0 {
            self.render_text(&encoder, view);
        }

        // Render find strip at the bottom
        self.draw_find_strip(&encoder, view, find_query, find_cursor_col, find_cursor_visible);

        // End encoding
        encoder.endEncoding();

        // Present the drawable
        let mtl_drawable: &ProtocolObject<dyn MTLDrawable> = ProtocolObject::from_ref(&*drawable);
        command_buffer.presentDrawable(mtl_drawable);

        // Commit the command buffer
        command_buffer.commit();
    }

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
    fn draw_find_strip(
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
    fn draw_tab_bar(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        editor: &Editor,
    ) {
        use crate::tab_bar::{
            CLOSE_BUTTON_COLOR, TAB_ACTIVE_COLOR,
            TAB_BAR_BACKGROUND_COLOR, TAB_INACTIVE_COLOR, TAB_LABEL_COLOR,
        };

        // Only draw tab bar if there's an active workspace with tabs
        let workspace = match editor.active_workspace() {
            Some(ws) => ws,
            None => return,
        };

        if workspace.tabs.is_empty() {
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
        let geometry = calculate_tab_bar_geometry(view_width, &tabs, glyph_width, workspace.tab_bar_view_offset);

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
}
