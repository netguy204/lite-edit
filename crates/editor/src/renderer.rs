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
// Chunk: docs/chunks/tiling_multi_pane_render - Pane layout rendering
use crate::pane_layout::{calculate_pane_rects, PaneId, PaneRect};
use crate::pane_frame_buffer::PaneFrameBuffer;
// Chunk: docs/chunks/renderer_polymorphic_buffer - Import BufferView for polymorphic rendering
use lite_edit_buffer::{BufferView, DirtyLines};
// Chunk: docs/chunks/syntax_highlighting - Highlighted buffer view for syntax coloring
use crate::highlighted_buffer::HighlightedBufferView;

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

// Chunk: docs/chunks/tiling_multi_pane_render - Pane divider and focus border colors
/// Pane divider color: #313244 (Catppuccin Mocha surface0)
/// A subtle line between adjacent panes.
const PANE_DIVIDER_COLOR: [f32; 4] = [
    0.192, // 0x31 / 255
    0.196, // 0x32 / 255
    0.267, // 0x44 / 255
    1.0,
];

/// Focused pane border color: #89b4fa at 60% (Catppuccin Mocha blue)
/// A colored border to indicate which pane is active.
const FOCUSED_PANE_BORDER_COLOR: [f32; 4] = [
    0.537, // 0x89 / 255
    0.706, // 0xb4 / 255
    0.980, // 0xfa / 255
    0.6,   // 60% opacity
];

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

// Chunk: docs/chunks/tiling_multi_pane_render - Pane clipping helpers
/// Creates a scissor rect for clipping a pane's content.
///
/// The rect covers the pane's bounds, constrained to the viewport.
fn pane_scissor_rect(
    pane_rect: &PaneRect,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    // Clamp to viewport bounds
    let x = (pane_rect.x as usize).min(view_width as usize);
    let y = (pane_rect.y as usize).min(view_height as usize);
    let right = ((pane_rect.x + pane_rect.width) as usize).min(view_width as usize);
    let bottom = ((pane_rect.y + pane_rect.height) as usize).min(view_height as usize);

    MTLScissorRect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

/// Creates a scissor rect for a pane's content area (below its tab bar).
fn pane_content_scissor_rect(
    pane_rect: &PaneRect,
    tab_bar_height: f32,
    view_width: f32,
    view_height: f32,
) -> MTLScissorRect {
    let x = (pane_rect.x as usize).min(view_width as usize);
    let y = ((pane_rect.y + tab_bar_height) as usize).min(view_height as usize);
    let right = ((pane_rect.x + pane_rect.width) as usize).min(view_width as usize);
    let bottom = ((pane_rect.y + pane_rect.height) as usize).min(view_height as usize);

    MTLScissorRect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
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
    pane_frame_buffer: Option<crate::pane_frame_buffer::PaneFrameBuffer>,
    /// Current viewport width in pixels (for wrap layout calculation)
    viewport_width_px: f32,
    // Chunk: docs/chunks/wrap_click_offset - Content width for consistent wrap calculation
    /// Content area width in pixels (viewport_width - RAIL_WIDTH).
    ///
    /// This is the width available for text rendering, excluding the left rail.
    /// Both the renderer and click handler must use this same value when creating
    /// WrapLayout instances to ensure consistent `cols_per_row` calculations.
    content_width_px: f32,
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
            viewport_width_px,
            content_width_px,
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
    pub fn update_viewport_size(&mut self, window_width: f32, window_height: f32) {
        // Use usize::MAX since scroll is synced externally from EditorState
        self.viewport.update_size(window_height, usize::MAX);
        self.viewport_width_px = window_width;
        // Content width is viewport minus the left rail
        self.content_width_px = (window_width - RAIL_WIDTH).max(0.0);
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

    // Chunk: docs/chunks/viewport_fractional_scroll - Pass y_offset for smooth scrolling
    // Chunk: docs/chunks/line_wrap_rendering - Use WrapLayout for soft wrapping
    // Chunk: docs/chunks/renderer_polymorphic_buffer - Accept &dyn BufferView for polymorphic rendering
    // Chunk: docs/chunks/wrap_click_offset - Use content_width_px for consistent cols_per_row
    // Chunk: docs/chunks/terminal_background_box_drawing - Pass mutable atlas and font for on-demand glyph addition
    /// Updates the glyph buffer from the given buffer view and viewport
    fn update_glyph_buffer(&mut self, view: &dyn BufferView) {
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
            self.cursor_visible,
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

        // Chunk: docs/chunks/renderer_polymorphic_buffer - Get BufferView from Editor
        // Chunk: docs/chunks/syntax_highlighting - Use HighlightedBufferView for syntax coloring
        // Update glyph buffer from active tab's BufferView with syntax highlighting
        if let Some(ws) = editor.active_workspace() {
            if let Some(tab) = ws.active_tab() {
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
        // Calculate pane rectangles for the active workspace
        let pane_rects: Vec<PaneRect>;
        let focused_pane_id: PaneId;
        if let Some(ws) = editor.active_workspace() {
            // Bounds for the pane area: starts after left rail
            // Note: For multi-pane, each pane has its own tab bar, so y=0
            let bounds = (
                RAIL_WIDTH,               // x: after left rail
                0.0,                      // y: top of window
                view_width - RAIL_WIDTH,  // width: remaining horizontal space
                view_height,              // height: full height
            );
            pane_rects = calculate_pane_rects(bounds, &ws.pane_root);
            focused_pane_id = ws.active_pane_id;
        } else {
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
                self.draw_welcome_screen(&encoder, view);
            } else {
                // Render editor text content (offset by RAIL_WIDTH and TAB_BAR_HEIGHT)
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
            RAIL_BACKGROUND_COLOR, TILE_ACTIVE_COLOR, TILE_BACKGROUND_COLOR,
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
    fn draw_pane_frames(
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
    fn render_pane(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        workspace: &crate::workspace::Workspace,
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
            self.draw_welcome_screen_in_pane(encoder, view, pane_rect);
        } else {
            // Set content offsets for this pane
            self.set_content_x_offset(pane_rect.x);
            self.set_content_y_offset(pane_rect.y + TAB_BAR_HEIGHT);

            // Update glyph buffer with pane-local content width
            let pane_content_width = pane_rect.width;
            self.content_width_px = pane_content_width;

            // Update glyph buffer from tab's buffer
            if tab.is_agent_tab() {
                if let Some(terminal) = workspace.agent_terminal() {
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

            // Render text
            if self.glyph_buffer.index_count() > 0 {
                self.render_text(encoder, view);
            }
        }
    }

    /// Draws a pane's tab bar at the specified position.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    /// * `pane` - The pane whose tabs to render
    /// * `pane_rect` - The rectangle for this pane
    /// * `view_width` - The viewport width
    /// * `view_height` - The viewport height
    fn draw_pane_tab_bar(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        _view: &MetalView,
        pane: &crate::pane_layout::Pane,
        pane_rect: &PaneRect,
        view_width: f32,
        view_height: f32,
    ) {
        use crate::tab_bar::{
            calculate_pane_tab_bar_geometry, tabs_from_pane,
            CLOSE_BUTTON_COLOR, TAB_ACTIVE_COLOR,
            TAB_BAR_BACKGROUND_COLOR, TAB_INACTIVE_COLOR, TAB_LABEL_COLOR,
        };

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

    /// Draws the welcome screen within a pane's bounds.
    ///
    /// This is similar to `draw_welcome_screen` but positions the content
    /// within the specified pane rectangle.
    fn draw_welcome_screen_in_pane(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
        pane_rect: &PaneRect,
    ) {
        use crate::welcome_screen::{calculate_welcome_geometry, WelcomeScreenGlyphBuffer};

        let frame = view.frame();
        let scale = view.scale_factor();
        let view_width = (frame.size.width * scale) as f32;
        let view_height = (frame.size.height * scale) as f32;
        let glyph_width = self.font.metrics.advance_width as f32;
        let line_height = self.font.metrics.line_height as f32;

        // Calculate content area within pane (excluding pane's tab bar)
        let content_width = pane_rect.width;
        let content_height = pane_rect.height - TAB_BAR_HEIGHT;

        // Calculate geometry centered in pane
        let mut geometry = calculate_welcome_geometry(
            content_width,
            content_height,
            glyph_width,
            line_height,
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

        // Chunk: docs/chunks/renderer_polymorphic_buffer - Get BufferView from Editor
        // Chunk: docs/chunks/syntax_highlighting - Use HighlightedBufferView for syntax coloring
        // Update glyph buffer from active tab's BufferView with syntax highlighting
        if let Some(ws) = editor.active_workspace() {
            if let Some(tab) = ws.active_tab() {
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

    // Chunk: docs/chunks/welcome_screen - Welcome screen rendering
    /// Draws the welcome screen when the active tab has an empty buffer.
    ///
    /// The welcome screen shows a feather ASCII art logo, the editor name,
    /// tagline, and a hotkey reference table. The content is centered both
    /// horizontally and vertically within the content viewport area.
    ///
    /// # Arguments
    /// * `encoder` - The active render command encoder
    /// * `view` - The Metal view (for viewport dimensions)
    fn draw_welcome_screen(
        &mut self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view: &MetalView,
    ) {
        use crate::welcome_screen::{calculate_welcome_geometry, WelcomeScreenGlyphBuffer};

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

        // Calculate welcome screen geometry (centered in content area)
        let glyph_width = self.font.metrics.advance_width as f32;
        let line_height = self.font.metrics.line_height as f32;
        let mut geometry = calculate_welcome_geometry(
            content_width,
            content_height,
            glyph_width,
            line_height,
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
}
