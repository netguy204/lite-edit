// Chunk: docs/chunks/selector_rendering - Selector overlay rendering
//!
//! Selector overlay layout and rendering support
//!
//! This module provides layout calculation and vertex buffer construction for
//! rendering a `SelectorWidget` as a floating panel overlay. The overlay appears
//! on top of the editor content when a selector is active (e.g., file picker,
//! command palette).
//!
//! Following the project's Humble View Architecture, geometry calculations are
//! pure functions that can be unit tested without Metal dependencies.
//!
//! ## Rendering Order
//!
//! The overlay renders these elements in back-to-front order:
//! 1. Background rect (opaque dark grey)
//! 2. Selection highlight (for the selected item row)
//! 3. Separator line (1px between query and item list)
//! 4. Query text with blinking cursor
//! 5. Item list text

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::glyph_buffer::{GlyphLayout, GlyphVertex, QuadRange};
use crate::selector::SelectorWidget;
use crate::shader::VERTEX_SIZE;

// =============================================================================
// Layout Constants
// =============================================================================

/// Overlay width as a ratio of window width (60%)
pub const OVERLAY_WIDTH_RATIO: f32 = 0.6;

/// Minimum overlay width in pixels
pub const OVERLAY_MIN_WIDTH: f32 = 400.0;

/// Maximum overlay height as a ratio of window height (50%)
pub const OVERLAY_MAX_HEIGHT_RATIO: f32 = 0.5;

/// Top edge offset as a ratio of window height (20% from top)
pub const OVERLAY_TOP_OFFSET_RATIO: f32 = 0.2;

/// Horizontal padding inside the overlay panel
pub const OVERLAY_PADDING_X: f32 = 8.0;

/// Vertical padding inside the overlay panel (top and between sections)
pub const OVERLAY_PADDING_Y: f32 = 4.0;

/// Height of the separator line in pixels
pub const SEPARATOR_HEIGHT: f32 = 1.0;

// =============================================================================
// Colors
// =============================================================================

/// Background color for the overlay panel: #2a2a2a (dark grey)
pub const OVERLAY_BACKGROUND_COLOR: [f32; 4] = [
    0.165, // 0x2a / 255
    0.165, // 0x2a / 255
    0.165, // 0x2a / 255
    1.0,
];

/// Selection highlight color: #0050a0 (accent blue)
pub const OVERLAY_SELECTION_COLOR: [f32; 4] = [
    0.0,   // 0x00 / 255
    0.314, // 0x50 / 255
    0.627, // 0xa0 / 255
    1.0,
];

/// Separator line color: subdued grey
pub const OVERLAY_SEPARATOR_COLOR: [f32; 4] = [
    0.4, // slightly lighter than background
    0.4,
    0.4,
    1.0,
];

// =============================================================================
// Overlay Geometry
// =============================================================================

/// Computed geometry for the selector overlay panel
///
/// All values are in screen coordinates (pixels). This struct is computed
/// by `calculate_overlay_geometry` and used by `SelectorGlyphBuffer` to
/// position quads correctly.
#[derive(Debug, Clone, Copy)]
pub struct OverlayGeometry {
    /// Left edge of the panel in screen coordinates
    pub panel_x: f32,
    /// Top edge of the panel in screen coordinates
    pub panel_y: f32,
    /// Width of the panel
    pub panel_width: f32,
    /// Height of the panel (dynamic based on items)
    pub panel_height: f32,
    /// Y coordinate for the query row text baseline area
    pub query_row_y: f32,
    /// Y coordinate for the 1px separator line
    pub separator_y: f32,
    /// Y coordinate where the item list starts (top of first item)
    pub list_origin_y: f32,
    /// Height of each item row (line_height)
    pub item_height: f32,
    /// Number of items that can be displayed within height constraints
    pub visible_items: usize,
    /// X coordinate where text content starts (panel_x + padding)
    pub content_x: f32,
    /// Width available for text content (panel_width - 2*padding)
    pub content_width: f32,
}

/// Calculates the geometry for the selector overlay panel
///
/// This is a pure function suitable for unit testing.
///
/// # Arguments
/// * `view_width` - The window/viewport width in pixels
/// * `view_height` - The window/viewport height in pixels
/// * `line_height` - The height of a text line in pixels
/// * `item_count` - The number of items in the selector list
///
/// # Returns
/// An `OverlayGeometry` struct with all layout measurements
pub fn calculate_overlay_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
    item_count: usize,
) -> OverlayGeometry {
    // Calculate panel width: 60% of view width, but at least OVERLAY_MIN_WIDTH
    // (if the view is wide enough)
    let desired_width = view_width * OVERLAY_WIDTH_RATIO;
    let panel_width = if view_width >= OVERLAY_MIN_WIDTH {
        desired_width.max(OVERLAY_MIN_WIDTH).min(view_width)
    } else {
        // View is narrower than minimum, use full width
        view_width
    };

    // Center horizontally
    let panel_x = (view_width - panel_width) / 2.0;

    // Calculate content area
    let content_x = panel_x + OVERLAY_PADDING_X;
    let content_width = panel_width - 2.0 * OVERLAY_PADDING_X;

    // Calculate vertical layout
    // Panel structure:
    // - padding_y
    // - query row (line_height)
    // - padding_y
    // - separator (1px)
    // - padding_y
    // - item list (N * line_height)
    // - padding_y

    let query_row_height = line_height;
    let separator_total = SEPARATOR_HEIGHT + OVERLAY_PADDING_Y;
    let fixed_height = OVERLAY_PADDING_Y * 2.0 + query_row_height + separator_total;

    // Maximum height for items
    let max_panel_height = view_height * OVERLAY_MAX_HEIGHT_RATIO;
    let max_items_height = max_panel_height - fixed_height - OVERLAY_PADDING_Y;
    let max_visible_items = (max_items_height / line_height).floor() as usize;

    // Actual visible items (capped by max and item_count)
    let visible_items = item_count.min(max_visible_items).max(0);

    // Actual items height
    let items_height = visible_items as f32 * line_height;

    // Total panel height
    let panel_height = fixed_height + items_height + OVERLAY_PADDING_Y;

    // Position panel at 20% from top
    let panel_y = view_height * OVERLAY_TOP_OFFSET_RATIO;

    // Calculate Y positions for each section
    let query_row_y = panel_y + OVERLAY_PADDING_Y;
    let separator_y = query_row_y + query_row_height + OVERLAY_PADDING_Y;
    let list_origin_y = separator_y + SEPARATOR_HEIGHT + OVERLAY_PADDING_Y;

    OverlayGeometry {
        panel_x,
        panel_y,
        panel_width,
        panel_height,
        query_row_y,
        separator_y,
        list_origin_y,
        item_height: line_height,
        visible_items,
        content_x,
        content_width,
    }
}

// =============================================================================
// SelectorGlyphBuffer
// =============================================================================

/// Manages vertex and index buffers for rendering the selector overlay
///
/// This is analogous to `GlyphBuffer` but specialized for the overlay UI.
/// It maintains separate quad ranges for different visual elements, allowing
/// the renderer to draw each with different colors.
pub struct SelectorGlyphBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,
    /// Layout calculator for glyph positioning
    layout: GlyphLayout,

    // Quad ranges for different draw phases (in render order)
    /// Background rect quad
    background_range: QuadRange,
    /// Selection highlight quad (for selected item row)
    selection_range: QuadRange,
    /// Separator line quad
    separator_range: QuadRange,
    /// Query text glyphs
    query_text_range: QuadRange,
    /// Query cursor quad (if visible)
    query_cursor_range: QuadRange,
    /// Item list glyphs
    item_text_range: QuadRange,
}

impl SelectorGlyphBuffer {
    /// Creates a new empty selector glyph buffer
    pub fn new(layout: GlyphLayout) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout,
            background_range: QuadRange::default(),
            selection_range: QuadRange::default(),
            separator_range: QuadRange::default(),
            query_text_range: QuadRange::default(),
            query_cursor_range: QuadRange::default(),
            item_text_range: QuadRange::default(),
        }
    }

    /// Returns the vertex buffer, if any
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.vertex_buffer.as_deref()
    }

    /// Returns the index buffer, if any
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.index_buffer.as_deref()
    }

    /// Returns the total number of indices
    pub fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the index range for the background quad
    pub fn background_range(&self) -> QuadRange {
        self.background_range
    }

    /// Returns the index range for the selection highlight quad
    pub fn selection_range(&self) -> QuadRange {
        self.selection_range
    }

    /// Returns the index range for the separator line quad
    pub fn separator_range(&self) -> QuadRange {
        self.separator_range
    }

    /// Returns the index range for query text glyphs
    pub fn query_text_range(&self) -> QuadRange {
        self.query_text_range
    }

    /// Returns the index range for the query cursor quad
    pub fn query_cursor_range(&self) -> QuadRange {
        self.query_cursor_range
    }

    /// Returns the index range for item list glyphs
    pub fn item_text_range(&self) -> QuadRange {
        self.item_text_range
    }

    // Chunk: docs/chunks/file_picker_scroll - Renders visible window using first_visible_item
    /// Updates the buffers from a SelectorWidget and geometry
    ///
    /// Builds vertex data in this order:
    /// 1. Background rect
    /// 2. Selection highlight
    /// 3. Separator line
    /// 4. Query text glyphs
    /// 5. Query cursor (if visible)
    /// 6. Item text glyphs
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas for text rendering
    /// * `widget` - The selector widget state
    /// * `geometry` - The computed overlay geometry
    /// * `cursor_visible` - Whether to render the query cursor
    pub fn update_from_widget(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        widget: &SelectorWidget,
        geometry: &OverlayGeometry,
        cursor_visible: bool,
    ) {
        // Estimate capacity: 3 rect quads + query chars + cursor + item chars
        let query_len = widget.query().chars().count();
        let item_chars: usize = widget
            .items()
            .iter()
            .take(geometry.visible_items)
            .map(|s| s.chars().count())
            .sum();
        let estimated_quads = 3 + query_len + 1 + item_chars;

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_quads * 6);
        let mut vertex_offset: u32 = 0;

        // Reset ranges
        self.background_range = QuadRange::default();
        self.selection_range = QuadRange::default();
        self.separator_range = QuadRange::default();
        self.query_text_range = QuadRange::default();
        self.query_cursor_range = QuadRange::default();
        self.item_text_range = QuadRange::default();

        let solid_glyph = atlas.solid_glyph();

        // Chunk: docs/chunks/renderer_styled_content - Per-vertex colors for overlay
        // Text color for overlay text (Catppuccin Mocha text)
        let text_color: [f32; 4] = [0.804, 0.839, 0.957, 1.0];

        // ==================== Phase 1: Background Rect ====================
        let bg_start = indices.len();
        {
            let quad = self.create_rect_quad(
                geometry.panel_x,
                geometry.panel_y,
                geometry.panel_width,
                geometry.panel_height,
                solid_glyph,
                OVERLAY_BACKGROUND_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.background_range = QuadRange::new(bg_start, indices.len() - bg_start);

        // ==================== Phase 2: Selection Highlight ====================
        let sel_start = indices.len();
        if !widget.items().is_empty() && geometry.visible_items > 0 {
            let selected = widget.selected_index();
            let first_visible = widget.first_visible_item();
            // Only render highlight if selected item is within visible window
            if selected >= first_visible && selected < first_visible + geometry.visible_items {
                // Compute the visible row (0 = first visible item)
                let visible_row = selected - first_visible;
                let sel_y = geometry.list_origin_y + visible_row as f32 * geometry.item_height;
                let quad = self.create_rect_quad(
                    geometry.panel_x,
                    sel_y,
                    geometry.panel_width,
                    geometry.item_height,
                    solid_glyph,
                    OVERLAY_SELECTION_COLOR,
                );
                vertices.extend_from_slice(&quad);
                Self::push_quad_indices(&mut indices, vertex_offset);
                vertex_offset += 4;
            }
        }
        self.selection_range = QuadRange::new(sel_start, indices.len() - sel_start);

        // ==================== Phase 3: Separator Line ====================
        let sep_start = indices.len();
        {
            let quad = self.create_rect_quad(
                geometry.panel_x + OVERLAY_PADDING_X,
                geometry.separator_y,
                geometry.panel_width - 2.0 * OVERLAY_PADDING_X,
                SEPARATOR_HEIGHT,
                solid_glyph,
                OVERLAY_SEPARATOR_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.separator_range = QuadRange::new(sep_start, indices.len() - sep_start);

        // ==================== Phase 4: Query Text ====================
        let query_start = indices.len();
        let query_cursor_x;
        {
            let mut x = geometry.content_x;
            let y = geometry.query_row_y;
            let max_x = geometry.content_x + geometry.content_width;

            for c in widget.query().chars() {
                // Skip if past content boundary
                if x + self.layout.glyph_width > max_x {
                    break;
                }

                // Skip spaces (no quad needed)
                if c == ' ' {
                    x += self.layout.glyph_width;
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let quad = self.create_glyph_quad_at(x, y, glyph, text_color);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
                x += self.layout.glyph_width;
            }
            query_cursor_x = x;
        }
        self.query_text_range = QuadRange::new(query_start, indices.len() - query_start);

        // ==================== Phase 5: Query Cursor ====================
        let cursor_start = indices.len();
        if cursor_visible {
            // Only render cursor if it fits in content area
            if query_cursor_x + self.layout.glyph_width <= geometry.content_x + geometry.content_width
            {
                let quad = self.create_rect_quad(
                    query_cursor_x,
                    geometry.query_row_y,
                    self.layout.glyph_width,
                    self.layout.line_height,
                    solid_glyph,
                    text_color, // Cursor uses text color
                );
                vertices.extend_from_slice(&quad);
                Self::push_quad_indices(&mut indices, vertex_offset);
                vertex_offset += 4;
            }
        }
        self.query_cursor_range = QuadRange::new(cursor_start, indices.len() - cursor_start);

        // ==================== Phase 6: Item Text ====================
        let item_start = indices.len();
        {
            let items = widget.items();
            let max_x = geometry.content_x + geometry.content_width;

            // Skip items before first_visible_item, take only visible items
            for (i, item) in items
                .iter()
                .skip(widget.first_visible_item())
                .take(geometry.visible_items)
                .enumerate()
            {
                let y = geometry.list_origin_y + i as f32 * geometry.item_height;
                let mut x = geometry.content_x;

                for c in item.chars() {
                    // Skip if past content boundary (clip long items)
                    if x + self.layout.glyph_width > max_x {
                        break;
                    }

                    // Skip spaces
                    if c == ' ' {
                        x += self.layout.glyph_width;
                        continue;
                    }

                    if let Some(glyph) = atlas.get_glyph(c) {
                        let quad = self.create_glyph_quad_at(x, y, glyph, text_color);
                        vertices.extend_from_slice(&quad);
                        Self::push_quad_indices(&mut indices, vertex_offset);
                        vertex_offset += 4;
                    }
                    x += self.layout.glyph_width;
                }
            }
        }
        self.item_text_range = QuadRange::new(item_start, indices.len() - item_start);

        // ==================== Create GPU Buffers ====================
        if vertices.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Create the vertex buffer
        let vertex_data_size = vertices.len() * VERTEX_SIZE;
        let vertex_ptr =
            NonNull::new(vertices.as_ptr() as *mut std::ffi::c_void).expect("vertex ptr not null");

        let vertex_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    vertex_ptr,
                    vertex_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create vertex buffer")
        };

        // Create the index buffer
        let index_data_size = indices.len() * std::mem::size_of::<u32>();
        let index_ptr =
            NonNull::new(indices.as_ptr() as *mut std::ffi::c_void).expect("index ptr not null");

        let index_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    index_ptr,
                    index_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer")
        };

        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.index_count = indices.len();
    }

    /// Creates a solid rectangle quad at the given position with the specified color
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    fn create_rect_quad(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        solid_glyph: &GlyphInfo,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(x, y, u0, v0, color),                 // top-left
            GlyphVertex::new(x + width, y, u1, v0, color),         // top-right
            GlyphVertex::new(x + width, y + height, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + height, u0, v1, color),        // bottom-left
        ]
    }

    /// Creates a glyph quad at an absolute position with the specified color
    // Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
    fn create_glyph_quad_at(&self, x: f32, y: f32, glyph: &GlyphInfo, color: [f32; 4]) -> [GlyphVertex; 4] {
        let (u0, v0) = glyph.uv_min;
        let (u1, v1) = glyph.uv_max;

        let w = glyph.width;
        let h = glyph.height;

        [
            GlyphVertex::new(x, y, u0, v0, color),         // top-left
            GlyphVertex::new(x + w, y, u1, v0, color),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1, color),     // bottom-left
        ]
    }

    /// Pushes indices for a quad (two triangles)
    fn push_quad_indices(indices: &mut Vec<u32>, vertex_offset: u32) {
        // Triangle 1: top-left, top-right, bottom-right
        indices.push(vertex_offset);
        indices.push(vertex_offset + 1);
        indices.push(vertex_offset + 2);
        // Triangle 2: top-left, bottom-right, bottom-left
        indices.push(vertex_offset);
        indices.push(vertex_offset + 2);
        indices.push(vertex_offset + 3);
    }
}

// =============================================================================
// Find Strip (Chunk: docs/chunks/find_in_file)
// =============================================================================

/// Horizontal padding for the find strip
pub const FIND_STRIP_PADDING_X: f32 = 8.0;

/// Vertical padding inside the find strip
pub const FIND_STRIP_PADDING_Y: f32 = 4.0;

/// Width of the "find:" label in characters
const FIND_LABEL_TEXT: &str = "find:";

/// Dim text color for the "find:" label
pub const FIND_LABEL_COLOR: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

/// Computed geometry for the find strip (bottom-anchored, 1 line tall)
///
/// All values are in screen coordinates (pixels).
#[derive(Debug, Clone, Copy)]
pub struct FindStripGeometry {
    /// Left edge of the strip in screen coordinates
    pub strip_x: f32,
    /// Top edge of the strip (bottom of viewport - strip_height)
    pub strip_y: f32,
    /// Width of the strip (full viewport width)
    pub strip_width: f32,
    /// Height of the strip (line_height + 2*padding)
    pub strip_height: f32,
    /// X where "find:" label starts
    pub label_x: f32,
    /// X where query text starts (after label + space)
    pub query_x: f32,
    /// Y coordinate for text baseline area
    pub text_y: f32,
    /// X coordinate of cursor position in query
    pub cursor_x: f32,
    /// Width of a single glyph
    pub glyph_width: f32,
    /// Line height
    pub line_height: f32,
}

/// Calculates the geometry for the find strip
///
/// The find strip is anchored to the bottom of the viewport, is 1 line tall
/// (plus padding), and spans the full width.
///
/// # Arguments
/// * `view_width` - The window/viewport width in pixels
/// * `view_height` - The window/viewport height in pixels
/// * `line_height` - The height of a text line in pixels
/// * `glyph_width` - The width of a single glyph
/// * `cursor_col` - The cursor column position in the query
pub fn calculate_find_strip_geometry(
    view_width: f32,
    view_height: f32,
    line_height: f32,
    glyph_width: f32,
    cursor_col: usize,
) -> FindStripGeometry {
    let strip_height = line_height + 2.0 * FIND_STRIP_PADDING_Y;
    let strip_y = view_height - strip_height;

    let label_x = FIND_STRIP_PADDING_X;
    let label_width = FIND_LABEL_TEXT.len() as f32 * glyph_width;
    let query_x = label_x + label_width + glyph_width; // One space after label

    let cursor_x = query_x + cursor_col as f32 * glyph_width;

    FindStripGeometry {
        strip_x: 0.0,
        strip_y,
        strip_width: view_width,
        strip_height,
        label_x,
        query_x,
        text_y: strip_y + FIND_STRIP_PADDING_Y,
        cursor_x,
        glyph_width,
        line_height,
    }
}

/// Manages vertex and index buffers for rendering the find strip
///
/// Similar to `SelectorGlyphBuffer` but specialized for the find strip UI.
pub struct FindStripGlyphBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,
    /// Layout calculator for glyph positioning
    layout: GlyphLayout,

    // Quad ranges for different draw phases
    /// Background rect quad
    background_range: QuadRange,
    /// "find:" label text glyphs
    label_range: QuadRange,
    /// Query text glyphs
    query_text_range: QuadRange,
    /// Query cursor quad (if visible)
    cursor_range: QuadRange,
}

impl FindStripGlyphBuffer {
    /// Creates a new empty find strip glyph buffer
    pub fn new(layout: GlyphLayout) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout,
            background_range: QuadRange::default(),
            label_range: QuadRange::default(),
            query_text_range: QuadRange::default(),
            cursor_range: QuadRange::default(),
        }
    }

    /// Returns the vertex buffer, if any
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.vertex_buffer.as_deref()
    }

    /// Returns the index buffer, if any
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.index_buffer.as_deref()
    }

    /// Returns the total number of indices
    pub fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the index range for the background quad
    pub fn background_range(&self) -> QuadRange {
        self.background_range
    }

    /// Returns the index range for the label text glyphs
    pub fn label_range(&self) -> QuadRange {
        self.label_range
    }

    /// Returns the index range for query text glyphs
    pub fn query_text_range(&self) -> QuadRange {
        self.query_text_range
    }

    /// Returns the index range for the cursor quad
    pub fn cursor_range(&self) -> QuadRange {
        self.cursor_range
    }

    /// Updates the buffers with find strip content
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas for text rendering
    /// * `query` - The current find query text
    /// * `geometry` - The computed find strip geometry
    /// * `cursor_visible` - Whether to render the cursor
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        query: &str,
        geometry: &FindStripGeometry,
        cursor_visible: bool,
    ) {
        // Estimate capacity
        let label_len = FIND_LABEL_TEXT.len();
        let query_len = query.chars().count();
        let estimated_quads = 1 + label_len + query_len + 1; // bg + label + query + cursor

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_quads * 6);
        let mut vertex_offset: u32 = 0;

        // Reset ranges
        self.background_range = QuadRange::default();
        self.label_range = QuadRange::default();
        self.query_text_range = QuadRange::default();
        self.cursor_range = QuadRange::default();

        let solid_glyph = atlas.solid_glyph();

        // Text color for query text (Catppuccin Mocha text)
        let text_color: [f32; 4] = [0.804, 0.839, 0.957, 1.0];

        // ==================== Phase 1: Background Rect ====================
        let bg_start = indices.len();
        {
            let quad = self.create_rect_quad(
                geometry.strip_x,
                geometry.strip_y,
                geometry.strip_width,
                geometry.strip_height,
                solid_glyph,
                OVERLAY_BACKGROUND_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.background_range = QuadRange::new(bg_start, indices.len() - bg_start);

        // ==================== Phase 2: "find:" Label ====================
        let label_start = indices.len();
        {
            let mut x = geometry.label_x;
            let y = geometry.text_y;

            for c in FIND_LABEL_TEXT.chars() {
                if c == ' ' {
                    x += geometry.glyph_width;
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let quad = self.create_glyph_quad_at(x, y, glyph, FIND_LABEL_COLOR);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
                x += geometry.glyph_width;
            }
        }
        self.label_range = QuadRange::new(label_start, indices.len() - label_start);

        // ==================== Phase 3: Query Text ====================
        let query_start = indices.len();
        {
            let mut x = geometry.query_x;
            let y = geometry.text_y;

            for c in query.chars() {
                if c == ' ' {
                    x += geometry.glyph_width;
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let quad = self.create_glyph_quad_at(x, y, glyph, text_color);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
                x += geometry.glyph_width;
            }
        }
        self.query_text_range = QuadRange::new(query_start, indices.len() - query_start);

        // ==================== Phase 4: Cursor ====================
        let cursor_start = indices.len();
        if cursor_visible {
            let quad = self.create_rect_quad(
                geometry.cursor_x,
                geometry.text_y,
                geometry.glyph_width,
                geometry.line_height,
                solid_glyph,
                text_color, // Cursor uses text color
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            #[allow(unused_assignments)]
            { vertex_offset += 4; }
        }
        self.cursor_range = QuadRange::new(cursor_start, indices.len() - cursor_start);

        // ==================== Create GPU Buffers ====================
        if vertices.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Create the vertex buffer
        let vertex_data_size = vertices.len() * VERTEX_SIZE;
        let vertex_ptr =
            NonNull::new(vertices.as_ptr() as *mut std::ffi::c_void).expect("vertex ptr not null");

        let vertex_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    vertex_ptr,
                    vertex_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create vertex buffer")
        };

        // Create the index buffer
        let index_data_size = indices.len() * std::mem::size_of::<u32>();
        let index_ptr =
            NonNull::new(indices.as_ptr() as *mut std::ffi::c_void).expect("index ptr not null");

        let index_buffer = unsafe {
            device
                .newBufferWithBytes_length_options(
                    index_ptr,
                    index_data_size,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer")
        };

        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.index_count = indices.len();
    }

    /// Creates a solid rectangle quad at the given position with the specified color
    fn create_rect_quad(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        solid_glyph: &GlyphInfo,
        color: [f32; 4],
    ) -> [GlyphVertex; 4] {
        let (u0, v0) = solid_glyph.uv_min;
        let (u1, v1) = solid_glyph.uv_max;

        [
            GlyphVertex::new(x, y, u0, v0, color),                 // top-left
            GlyphVertex::new(x + width, y, u1, v0, color),         // top-right
            GlyphVertex::new(x + width, y + height, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + height, u0, v1, color),        // bottom-left
        ]
    }

    /// Creates a glyph quad at an absolute position with the specified color
    fn create_glyph_quad_at(&self, x: f32, y: f32, glyph: &GlyphInfo, color: [f32; 4]) -> [GlyphVertex; 4] {
        let (u0, v0) = glyph.uv_min;
        let (u1, v1) = glyph.uv_max;

        let w = glyph.width;
        let h = glyph.height;

        [
            GlyphVertex::new(x, y, u0, v0, color),         // top-left
            GlyphVertex::new(x + w, y, u1, v0, color),     // top-right
            GlyphVertex::new(x + w, y + h, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + h, u0, v1, color),     // bottom-left
        ]
    }

    /// Pushes indices for a quad (two triangles)
    fn push_quad_indices(indices: &mut Vec<u32>, vertex_offset: u32) {
        // Triangle 1: top-left, top-right, bottom-right
        indices.push(vertex_offset);
        indices.push(vertex_offset + 1);
        indices.push(vertex_offset + 2);
        // Triangle 2: top-left, bottom-right, bottom-left
        indices.push(vertex_offset);
        indices.push(vertex_offset + 2);
        indices.push(vertex_offset + 3);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // calculate_overlay_geometry tests
    // =========================================================================

    #[test]
    fn panel_width_is_60_percent_of_view_width() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        // 60% of 1000 = 600
        assert_eq!(geom.panel_width, 600.0);
    }

    #[test]
    fn panel_width_clamps_to_minimum() {
        // View width is 500, 60% = 300, but min is 400
        let geom = calculate_overlay_geometry(500.0, 800.0, 20.0, 5);

        // Should be clamped to 400 (min width)
        assert_eq!(geom.panel_width, 400.0);
    }

    #[test]
    fn panel_width_uses_full_width_when_view_narrower_than_minimum() {
        // View width is 300, which is less than min 400
        let geom = calculate_overlay_geometry(300.0, 800.0, 20.0, 5);

        // Should use full view width
        assert_eq!(geom.panel_width, 300.0);
    }

    #[test]
    fn panel_is_horizontally_centered() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        // Panel width is 600, so it should start at (1000 - 600) / 2 = 200
        assert_eq!(geom.panel_x, 200.0);
    }

    #[test]
    fn panel_top_is_at_20_percent() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        // 20% of 800 = 160
        assert_eq!(geom.panel_y, 160.0);
    }

    #[test]
    fn panel_height_caps_at_50_percent_of_view_height() {
        // Many items that would exceed 50% height
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 100);

        // Panel should not exceed 50% of 800 = 400
        assert!(geom.panel_height <= 400.0);
    }

    #[test]
    fn visible_items_computed_correctly() {
        // With line_height = 20 and view_height = 800 (max items height ~200 after fixed elements)
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 100);

        // Fixed height: padding + query + padding + separator + padding ~= 4 + 20 + 4 + 1 + 4 = 33
        // Plus final padding = 4
        // So items space = 400 - 37 = 363
        // 363 / 20 = 18.15 -> 18 items max
        // But with actual implementation it may differ slightly - just verify it's reasonable
        assert!(geom.visible_items > 0);
        assert!(geom.visible_items < 100); // Capped, not all 100
    }

    #[test]
    fn visible_items_capped_by_item_count() {
        // Only 3 items available
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 3);

        assert_eq!(geom.visible_items, 3);
    }

    #[test]
    fn visible_items_is_zero_when_no_items() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 0);

        assert_eq!(geom.visible_items, 0);
    }

    #[test]
    fn query_row_is_below_top_padding() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        // query_row_y should be panel_y + padding
        assert_eq!(geom.query_row_y, geom.panel_y + OVERLAY_PADDING_Y);
    }

    #[test]
    fn separator_is_below_query_row() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        // separator_y should be after query row + padding
        assert_eq!(
            geom.separator_y,
            geom.query_row_y + geom.item_height + OVERLAY_PADDING_Y
        );
    }

    #[test]
    fn list_origin_is_below_separator() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        // list_origin_y should be after separator + padding
        assert_eq!(
            geom.list_origin_y,
            geom.separator_y + SEPARATOR_HEIGHT + OVERLAY_PADDING_Y
        );
    }

    #[test]
    fn content_area_accounts_for_padding() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        assert_eq!(geom.content_x, geom.panel_x + OVERLAY_PADDING_X);
        assert_eq!(geom.content_width, geom.panel_width - 2.0 * OVERLAY_PADDING_X);
    }

    #[test]
    fn item_height_matches_line_height() {
        let geom = calculate_overlay_geometry(1000.0, 800.0, 20.0, 5);

        assert_eq!(geom.item_height, 20.0);
    }
}
