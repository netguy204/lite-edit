// Chunk: docs/chunks/content_tab_bar - Content tab bar rendering and interaction
// Chunk: docs/chunks/tab_bar_interaction - Label derivation and left-truncation
//!
//! Tab bar layout and rendering for content tabs within a workspace.
//!
//! This module provides layout calculation and vertex buffer construction for
//! rendering the horizontal tab bar at the top of the content area. Following
//! the project's Humble View Architecture, geometry calculations are pure
//! functions that can be unit tested without Metal dependencies.
//!
//! ## Layout
//!
//! The tab bar is a fixed-height horizontal strip at the top of the content area
//! (to the right of the left rail). Each tab is represented by a rectangular
//! button containing:
//! - A label (filename or terminal title)
//! - An optional dirty/unread indicator
//! - A close button
//!
//! The active tab is visually highlighted. When tabs overflow the available width,
//! horizontal scrolling is supported via `view_offset`.

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::glyph_buffer::{GlyphLayout, GlyphVertex, QuadRange};
use crate::left_rail::RAIL_WIDTH;
use crate::shader::VERTEX_SIZE;
use crate::workspace::{Tab, TabKind, Workspace};

// =============================================================================
// Layout Constants
// =============================================================================

/// Height of the tab bar in pixels (scaled)
pub const TAB_BAR_HEIGHT: f32 = 32.0;

/// Minimum width of a tab in pixels
pub const TAB_MIN_WIDTH: f32 = 80.0;

/// Maximum width of a tab in pixels
pub const TAB_MAX_WIDTH: f32 = 200.0;

/// Horizontal padding inside each tab
pub const TAB_PADDING_H: f32 = 12.0;

/// Vertical padding inside each tab
pub const TAB_PADDING_V: f32 = 6.0;

/// Size of the close button (square)
pub const CLOSE_BUTTON_SIZE: f32 = 16.0;

/// Gap between close button and tab label
pub const CLOSE_BUTTON_GAP: f32 = 4.0;

/// Size of the dirty/unread indicator dot
pub const INDICATOR_SIZE: f32 = 6.0;

/// Gap between indicator and label
pub const INDICATOR_GAP: f32 = 4.0;

/// Spacing between tabs
pub const TAB_SPACING: f32 = 1.0;

// =============================================================================
// Colors (Catppuccin Mocha theme, consistent with left_rail.rs)
// =============================================================================

/// Background color for the tab bar strip
pub const TAB_BAR_BACKGROUND_COLOR: [f32; 4] = [
    0.12, // Darker than editor background
    0.12,
    0.14,
    1.0,
];

/// Inactive tab background color
pub const TAB_INACTIVE_COLOR: [f32; 4] = [
    0.15,
    0.15,
    0.18,
    1.0,
];

/// Active tab highlight color
pub const TAB_ACTIVE_COLOR: [f32; 4] = [
    0.22,
    0.22,
    0.28,
    1.0,
];

/// Tab label text color
pub const TAB_LABEL_COLOR: [f32; 4] = [
    0.7,
    0.7,
    0.75,
    1.0,
];

/// Dirty indicator color (yellow/orange)
pub const DIRTY_INDICATOR_COLOR: [f32; 4] = [
    0.9,
    0.8,
    0.1,
    1.0,
];

/// Unread indicator color (blue)
pub const UNREAD_INDICATOR_COLOR: [f32; 4] = [
    0.2,
    0.6,
    0.9,
    1.0,
];

/// Close button color (dimmed)
pub const CLOSE_BUTTON_COLOR: [f32; 4] = [
    0.5,
    0.5,
    0.55,
    1.0,
];

/// Close button hover color (brighter)
pub const CLOSE_BUTTON_HOVER_COLOR: [f32; 4] = [
    0.8,
    0.3,
    0.3,
    1.0,
];

// =============================================================================
// Geometry Types
// =============================================================================

/// Rectangle describing a close button's hit area.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloseButtonRect {
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

impl CloseButtonRect {
    /// Creates a new close button rectangle.
    pub fn new(x: f32, y: f32, size: f32) -> Self {
        Self { x, y, size }
    }

    /// Returns true if the point (px, py) is inside this close button.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.size && py >= self.y && py < self.y + self.size
    }
}

/// A tab rectangle with hit areas for the tab body and close button.
#[derive(Debug, Clone, Copy)]
pub struct TabRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// The close button hit area within this tab
    pub close_button: CloseButtonRect,
    /// Index of the tab in the workspace
    pub tab_index: usize,
}

impl TabRect {
    /// Creates a new tab rectangle.
    pub fn new(x: f32, y: f32, width: f32, height: f32, close_button: CloseButtonRect, tab_index: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
            close_button,
            tab_index,
        }
    }

    /// Returns true if the point (px, py) is inside this tab rectangle.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    /// Returns true if the point is in the close button area.
    pub fn is_close_button(&self, px: f32, py: f32) -> bool {
        self.close_button.contains(px, py)
    }
}

/// Computed geometry for the tab bar.
///
/// All values are in screen coordinates (pixels).
#[derive(Debug, Clone)]
pub struct TabBarGeometry {
    /// X position of the tab bar (typically RAIL_WIDTH)
    pub x: f32,
    /// Y position of the tab bar (typically 0)
    pub y: f32,
    /// Total width of the tab bar (view_width - RAIL_WIDTH)
    pub width: f32,
    /// Height of the tab bar
    pub height: f32,
    /// Rectangles for each visible tab
    pub tab_rects: Vec<TabRect>,
    /// View offset for horizontal scrolling (in pixels)
    pub view_offset: f32,
    /// Total width of all tabs (may exceed visible width)
    pub total_tabs_width: f32,
}

/// Information about a tab for rendering.
#[derive(Debug, Clone)]
pub struct TabInfo {
    /// Display label (potentially truncated)
    pub label: String,
    /// Whether this tab is the active tab
    pub is_active: bool,
    /// Whether this tab has unsaved changes
    pub is_dirty: bool,
    /// Whether this tab has unread content (for terminals)
    pub is_unread: bool,
    /// Tab index in the workspace
    pub index: usize,
}

impl TabInfo {
    // Chunk: docs/chunks/tab_bar_interaction - Derive label from associated_file for file tabs
    /// Creates a TabInfo from a Tab.
    ///
    /// For file tabs (`TabKind::File`), the label is derived from the `associated_file`
    /// path at render time, using the filename component. This ensures the label always
    /// reflects the current file path rather than a stale snapshot.
    ///
    /// For non-file tabs (Terminal, AgentOutput, Diff), the static `tab.label` is used.
    pub fn from_tab(tab: &Tab, index: usize, is_active: bool) -> Self {
        let label = match tab.kind {
            TabKind::File => {
                // Derive label from associated_file for file tabs
                tab.associated_file
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Untitled".to_string())
            }
            // Non-file tabs use the static label
            TabKind::Terminal | TabKind::AgentOutput | TabKind::Diff => {
                tab.label.clone()
            }
        };

        Self {
            label,
            is_active,
            is_dirty: tab.dirty,
            is_unread: tab.unread,
            index,
        }
    }
}

/// Calculates the width of a tab based on its label.
///
/// Returns a width clamped between TAB_MIN_WIDTH and TAB_MAX_WIDTH.
pub fn calculate_tab_width(label: &str, glyph_width: f32) -> f32 {
    // Tab width = padding + indicator + gap + label + gap + close button + padding
    let label_width = label.chars().count() as f32 * glyph_width;
    let content_width = TAB_PADDING_H + INDICATOR_SIZE + INDICATOR_GAP + label_width + CLOSE_BUTTON_GAP + CLOSE_BUTTON_SIZE + TAB_PADDING_H;
    content_width.clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
}

/// Calculates the geometry for the tab bar.
///
/// This is a pure function suitable for unit testing.
///
/// # Arguments
/// * `view_width` - The total viewport width in pixels
/// * `tabs` - Information about each tab
/// * `active_tab_index` - Index of the currently active tab
/// * `glyph_width` - Width of a single character glyph
/// * `view_offset` - Current horizontal scroll offset for overflow
///
/// # Returns
/// A `TabBarGeometry` struct with all layout measurements
pub fn calculate_tab_bar_geometry(
    view_width: f32,
    tabs: &[TabInfo],
    glyph_width: f32,
    view_offset: f32,
) -> TabBarGeometry {
    let bar_x = RAIL_WIDTH;
    let bar_y = 0.0;
    let bar_width = (view_width - RAIL_WIDTH).max(0.0);
    let bar_height = TAB_BAR_HEIGHT;

    let mut tab_rects = Vec::with_capacity(tabs.len());
    let mut x = bar_x - view_offset;
    let y = bar_y;

    for (idx, tab_info) in tabs.iter().enumerate() {
        let tab_width = calculate_tab_width(&tab_info.label, glyph_width);

        // Only add tabs that are at least partially visible
        let tab_right = x + tab_width;
        let visible_left = bar_x;
        let visible_right = bar_x + bar_width;

        if tab_right > visible_left && x < visible_right {
            // Calculate close button position (right side of tab)
            let close_x = x + tab_width - TAB_PADDING_H - CLOSE_BUTTON_SIZE;
            let close_y = y + (bar_height - CLOSE_BUTTON_SIZE) / 2.0;
            let close_button = CloseButtonRect::new(close_x, close_y, CLOSE_BUTTON_SIZE);

            tab_rects.push(TabRect::new(x, y, tab_width, bar_height, close_button, idx));
        }

        x += tab_width + TAB_SPACING;
    }

    // Calculate total width of all tabs
    let total_tabs_width: f32 = tabs.iter()
        .map(|t| calculate_tab_width(&t.label, glyph_width) + TAB_SPACING)
        .sum::<f32>()
        .max(0.0)
        - TAB_SPACING; // Remove trailing spacing

    TabBarGeometry {
        x: bar_x,
        y: bar_y,
        width: bar_width,
        height: bar_height,
        tab_rects,
        view_offset,
        total_tabs_width: total_tabs_width.max(0.0),
    }
}

/// Extracts TabInfo list from a workspace.
pub fn tabs_from_workspace(workspace: &Workspace) -> Vec<TabInfo> {
    // Collect raw tab info
    let mut tabs: Vec<TabInfo> = workspace.tabs
        .iter()
        .enumerate()
        .map(|(idx, tab)| TabInfo::from_tab(tab, idx, idx == workspace.active_tab))
        .collect();

    // Apply disambiguation to labels with duplicate filenames
    disambiguate_labels(&mut tabs, workspace);

    tabs
}

/// Disambiguates tab labels when multiple tabs have the same filename.
///
/// When two or more tabs have the same base filename (e.g., "main.rs"), this
/// function adds parent directory information to make them unique.
///
/// For example:
/// - "main.rs" and "main.rs" become "src/main.rs" and "tests/main.rs"
fn disambiguate_labels(tabs: &mut [TabInfo], workspace: &Workspace) {
    use std::collections::HashMap;

    // Count occurrences of each label
    let mut label_counts: HashMap<String, usize> = HashMap::new();
    for tab in tabs.iter() {
        *label_counts.entry(tab.label.clone()).or_insert(0) += 1;
    }

    // For labels that appear more than once, add parent directory info
    for (idx, tab_info) in tabs.iter_mut().enumerate() {
        if label_counts.get(&tab_info.label).copied().unwrap_or(0) > 1 {
            // Get the associated file path
            if let Some(path) = workspace.tabs.get(idx).and_then(|t| t.associated_file.as_ref()) {
                // Try to add parent directory
                if let Some(parent) = path.parent() {
                    if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                        tab_info.label = format!("{}/{}", parent_name, tab_info.label);
                    }
                }
            }
        }
    }
}

// =============================================================================
// TabBarGlyphBuffer
// =============================================================================

/// Manages vertex and index buffers for rendering the tab bar.
///
/// This is analogous to `LeftRailGlyphBuffer` but specialized for the
/// horizontal tab bar at the top of the content area.
pub struct TabBarGlyphBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,
    /// Layout calculator for glyph positioning
    layout: GlyphLayout,

    // Quad ranges for different draw phases
    /// Tab bar background rect
    background_range: QuadRange,
    /// Inactive tab backgrounds
    tab_background_range: QuadRange,
    /// Active tab highlight
    active_tab_range: QuadRange,
    /// Dirty/unread indicator dots
    indicator_range: QuadRange,
    /// Close button icons
    close_button_range: QuadRange,
    /// Tab labels
    label_range: QuadRange,
}

impl TabBarGlyphBuffer {
    /// Creates a new empty tab bar glyph buffer.
    pub fn new(layout: GlyphLayout) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout,
            background_range: QuadRange::default(),
            tab_background_range: QuadRange::default(),
            active_tab_range: QuadRange::default(),
            indicator_range: QuadRange::default(),
            close_button_range: QuadRange::default(),
            label_range: QuadRange::default(),
        }
    }

    /// Returns the vertex buffer, if any.
    pub fn vertex_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.vertex_buffer.as_deref()
    }

    /// Returns the index buffer, if any.
    pub fn index_buffer(&self) -> Option<&ProtocolObject<dyn MTLBuffer>> {
        self.index_buffer.as_deref()
    }

    /// Returns the total number of indices.
    pub fn index_count(&self) -> usize {
        self.index_count
    }

    /// Returns the index range for the tab bar background.
    pub fn background_range(&self) -> QuadRange {
        self.background_range
    }

    /// Returns the index range for tab backgrounds.
    pub fn tab_background_range(&self) -> QuadRange {
        self.tab_background_range
    }

    /// Returns the index range for the active tab highlight.
    pub fn active_tab_range(&self) -> QuadRange {
        self.active_tab_range
    }

    /// Returns the index range for indicator dots.
    pub fn indicator_range(&self) -> QuadRange {
        self.indicator_range
    }

    /// Returns the index range for close buttons.
    pub fn close_button_range(&self) -> QuadRange {
        self.close_button_range
    }

    /// Returns the index range for labels.
    pub fn label_range(&self) -> QuadRange {
        self.label_range
    }

    /// Updates the buffers from the workspace and geometry.
    ///
    /// Builds vertex data in this order:
    /// 1. Tab bar background strip
    /// 2. Inactive tab backgrounds
    /// 3. Active tab highlight
    /// 4. Dirty/unread indicators
    /// 5. Close button icons
    /// 6. Tab labels
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas for text rendering
    /// * `tabs` - Tab information for each tab
    /// * `geometry` - The computed tab bar geometry
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        tabs: &[TabInfo],
        geometry: &TabBarGeometry,
    ) {
        // Estimate capacity: 1 background + tabs + indicators + close buttons + label chars
        let label_chars: usize = tabs.iter().map(|t| t.label.chars().count()).sum();
        let tab_count = geometry.tab_rects.len();
        let estimated_quads = 1 + tab_count * 3 + label_chars; // bg + tabs + indicators + close + labels

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(estimated_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(estimated_quads * 6);
        let mut vertex_offset: u32 = 0;

        // Reset ranges
        self.background_range = QuadRange::default();
        self.tab_background_range = QuadRange::default();
        self.active_tab_range = QuadRange::default();
        self.indicator_range = QuadRange::default();
        self.close_button_range = QuadRange::default();
        self.label_range = QuadRange::default();

        let solid_glyph = atlas.solid_glyph();

        // ==================== Phase 1: Tab Bar Background ====================
        let bg_start = indices.len();
        {
            let quad = self.create_rect_quad(
                geometry.x,
                geometry.y,
                geometry.width,
                geometry.height,
                solid_glyph,
                TAB_BAR_BACKGROUND_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.background_range = QuadRange::new(bg_start, indices.len() - bg_start);

        // ==================== Phase 2: Inactive Tab Backgrounds ====================
        let tab_bg_start = indices.len();
        for tab_rect in &geometry.tab_rects {
            let tab_info = &tabs[tab_rect.tab_index];
            if tab_info.is_active {
                continue; // Skip active tab, it gets its own highlight
            }

            let quad = self.create_rect_quad(
                tab_rect.x,
                tab_rect.y,
                tab_rect.width,
                tab_rect.height,
                solid_glyph,
                TAB_INACTIVE_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.tab_background_range = QuadRange::new(tab_bg_start, indices.len() - tab_bg_start);

        // ==================== Phase 3: Active Tab Highlight ====================
        let active_start = indices.len();
        for tab_rect in &geometry.tab_rects {
            let tab_info = &tabs[tab_rect.tab_index];
            if !tab_info.is_active {
                continue;
            }

            let quad = self.create_rect_quad(
                tab_rect.x,
                tab_rect.y,
                tab_rect.width,
                tab_rect.height,
                solid_glyph,
                TAB_ACTIVE_COLOR,
            );
            vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.active_tab_range = QuadRange::new(active_start, indices.len() - active_start);

        // ==================== Phase 4: Dirty/Unread Indicators ====================
        let indicator_start = indices.len();
        for tab_rect in &geometry.tab_rects {
            let tab_info = &tabs[tab_rect.tab_index];

            // Only show indicator if dirty or unread
            let indicator_color = if tab_info.is_dirty {
                Some(DIRTY_INDICATOR_COLOR)
            } else if tab_info.is_unread {
                Some(UNREAD_INDICATOR_COLOR)
            } else {
                None
            };

            if let Some(color) = indicator_color {
                // Position indicator on the left side of the tab
                let indicator_x = tab_rect.x + TAB_PADDING_H;
                let indicator_y = tab_rect.y + (geometry.height - INDICATOR_SIZE) / 2.0;

                let quad = self.create_rect_quad(
                    indicator_x,
                    indicator_y,
                    INDICATOR_SIZE,
                    INDICATOR_SIZE,
                    solid_glyph,
                    color,
                );
                vertices.extend_from_slice(&quad);
                Self::push_quad_indices(&mut indices, vertex_offset);
                vertex_offset += 4;
            }
        }
        self.indicator_range = QuadRange::new(indicator_start, indices.len() - indicator_start);

        // ==================== Phase 5: Close Buttons ====================
        let close_start = indices.len();
        for tab_rect in &geometry.tab_rects {
            // Draw close button as an "×" character
            let close_rect = &tab_rect.close_button;

            // Use the '×' or 'x' character for the close button
            if let Some(glyph) = atlas.get_glyph('×').or_else(|| atlas.get_glyph('x')) {
                // Center the glyph in the close button area
                let glyph_x = close_rect.x + (close_rect.size - glyph.width) / 2.0;
                let glyph_y = close_rect.y + (close_rect.size - glyph.height) / 2.0;

                let quad = self.create_glyph_quad_at(glyph_x, glyph_y, glyph, CLOSE_BUTTON_COLOR);
                vertices.extend_from_slice(&quad);
                Self::push_quad_indices(&mut indices, vertex_offset);
                vertex_offset += 4;
            }
        }
        self.close_button_range = QuadRange::new(close_start, indices.len() - close_start);

        // ==================== Phase 6: Tab Labels ====================
        let label_start = indices.len();
        for tab_rect in &geometry.tab_rects {
            let tab_info = &tabs[tab_rect.tab_index];

            // Calculate label position (after indicator if present)
            let label_x = if tab_info.is_dirty || tab_info.is_unread {
                tab_rect.x + TAB_PADDING_H + INDICATOR_SIZE + INDICATOR_GAP
            } else {
                tab_rect.x + TAB_PADDING_H
            };
            let label_y = tab_rect.y + (geometry.height - self.layout.line_height) / 2.0;

            // Calculate available width for label
            let close_button_left = tab_rect.close_button.x;
            let available_width = close_button_left - label_x - CLOSE_BUTTON_GAP;
            let max_chars = (available_width / self.layout.glyph_width).floor() as usize;

            // Chunk: docs/chunks/tab_bar_interaction - Left-truncation to preserve file extension
            // Truncate from the left when label exceeds available space, preserving the
            // end of the filename (which typically contains the extension and distinguishing chars)
            let label: String = {
                let char_count = tab_info.label.chars().count();
                if char_count > max_chars && max_chars > 1 {
                    // Left-truncate: skip (char_count - max_chars + 1) chars, prepend ellipsis
                    let skip = char_count - max_chars + 1;
                    let truncated: String = tab_info.label.chars().skip(skip).collect();
                    format!("…{}", truncated)
                } else if char_count > max_chars {
                    // max_chars is 0 or 1, and label is longer - just show ellipsis or nothing
                    if max_chars >= 1 {
                        "…".to_string()
                    } else {
                        String::new()
                    }
                } else {
                    // Label fits within max_chars
                    tab_info.label.clone()
                }
            };

            for (char_idx, c) in label.chars().enumerate() {
                if c == ' ' {
                    continue;
                }

                if let Some(glyph) = atlas.get_glyph(c) {
                    let x = label_x + char_idx as f32 * self.layout.glyph_width;
                    let quad = self.create_glyph_quad_at(x, label_y, glyph, TAB_LABEL_COLOR);
                    vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
            }
        }
        self.label_range = QuadRange::new(label_start, indices.len() - label_start);

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

    /// Creates a solid rectangle quad at the given position.
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
            GlyphVertex::new(x, y, u0, v0, color),                  // top-left
            GlyphVertex::new(x + width, y, u1, v0, color),          // top-right
            GlyphVertex::new(x + width, y + height, u1, v1, color), // bottom-right
            GlyphVertex::new(x, y + height, u0, v1, color),         // bottom-left
        ]
    }

    /// Creates a glyph quad at an absolute position.
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

    /// Pushes indices for a quad (two triangles).
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

    fn test_glyph_width() -> f32 {
        8.0 // Standard test glyph width
    }

    // =========================================================================
    // Geometry Tests
    // =========================================================================

    #[test]
    fn test_geometry_with_zero_tabs() {
        let tabs: Vec<TabInfo> = vec![];
        let geom = calculate_tab_bar_geometry(800.0, &tabs, test_glyph_width(), 0.0);

        assert_eq!(geom.x, RAIL_WIDTH);
        assert_eq!(geom.y, 0.0);
        assert_eq!(geom.height, TAB_BAR_HEIGHT);
        assert!(geom.tab_rects.is_empty());
        assert_eq!(geom.total_tabs_width, 0.0);
    }

    #[test]
    fn test_geometry_with_one_tab() {
        let tabs = vec![TabInfo {
            label: "test.rs".to_string(),
            is_active: true,
            is_dirty: false,
            is_unread: false,
            index: 0,
        }];
        let geom = calculate_tab_bar_geometry(800.0, &tabs, test_glyph_width(), 0.0);

        assert_eq!(geom.tab_rects.len(), 1);
        let tab_rect = &geom.tab_rects[0];
        assert_eq!(tab_rect.x, RAIL_WIDTH);
        assert_eq!(tab_rect.y, 0.0);
        assert_eq!(tab_rect.height, TAB_BAR_HEIGHT);
        assert_eq!(tab_rect.tab_index, 0);
    }

    #[test]
    fn test_geometry_with_five_tabs() {
        let tabs: Vec<TabInfo> = (0..5)
            .map(|i| TabInfo {
                label: format!("file{}.rs", i),
                is_active: i == 2,
                is_dirty: false,
                is_unread: false,
                index: i,
            })
            .collect();
        let geom = calculate_tab_bar_geometry(800.0, &tabs, test_glyph_width(), 0.0);

        assert_eq!(geom.tab_rects.len(), 5);

        // Tabs should be laid out horizontally
        for i in 1..geom.tab_rects.len() {
            assert!(geom.tab_rects[i].x > geom.tab_rects[i - 1].x);
        }
    }

    #[test]
    fn test_tab_rects_dont_overlap() {
        let tabs: Vec<TabInfo> = (0..3)
            .map(|i| TabInfo {
                label: format!("file{}.rs", i),
                is_active: false,
                is_dirty: false,
                is_unread: false,
                index: i,
            })
            .collect();
        let geom = calculate_tab_bar_geometry(800.0, &tabs, test_glyph_width(), 0.0);

        for i in 1..geom.tab_rects.len() {
            let prev_right = geom.tab_rects[i - 1].x + geom.tab_rects[i - 1].width;
            let curr_left = geom.tab_rects[i].x;
            assert!(
                curr_left >= prev_right,
                "Tab {} overlaps with tab {}: prev_right={}, curr_left={}",
                i,
                i - 1,
                prev_right,
                curr_left
            );
        }
    }

    #[test]
    fn test_tab_rect_contains() {
        let close_button = CloseButtonRect::new(100.0, 8.0, CLOSE_BUTTON_SIZE);
        let rect = TabRect::new(50.0, 0.0, 120.0, TAB_BAR_HEIGHT, close_button, 0);

        // Inside tab
        assert!(rect.contains(60.0, 10.0));
        assert!(rect.contains(50.0, 0.0)); // Top-left corner
        assert!(rect.contains(169.0, 31.0)); // Just inside bottom-right

        // Outside tab
        assert!(!rect.contains(40.0, 10.0)); // Left of tab
        assert!(!rect.contains(180.0, 10.0)); // Right of tab
        assert!(!rect.contains(60.0, -5.0)); // Above tab
        assert!(!rect.contains(60.0, 40.0)); // Below tab
    }

    #[test]
    fn test_close_button_contains() {
        let close_button = CloseButtonRect::new(100.0, 8.0, CLOSE_BUTTON_SIZE);

        // Inside close button
        assert!(close_button.contains(105.0, 12.0));
        assert!(close_button.contains(100.0, 8.0)); // Top-left corner

        // Outside close button
        assert!(!close_button.contains(95.0, 12.0)); // Left of button
        assert!(!close_button.contains(120.0, 12.0)); // Right of button
    }

    #[test]
    fn test_tab_labels_truncated_to_max_width() {
        // A very long label should be clamped to TAB_MAX_WIDTH
        let long_label = "this_is_a_very_long_filename_that_should_be_truncated.rs";
        let tab_width = calculate_tab_width(long_label, test_glyph_width());
        assert!(tab_width <= TAB_MAX_WIDTH);
    }

    #[test]
    fn test_tab_width_minimum() {
        // A short label should still have minimum width
        let short_label = "a";
        let tab_width = calculate_tab_width(short_label, test_glyph_width());
        assert!(tab_width >= TAB_MIN_WIDTH);
    }

    #[test]
    fn test_view_offset_scrolls_tabs() {
        let tabs: Vec<TabInfo> = (0..5)
            .map(|i| TabInfo {
                label: format!("longfilename{}.rs", i),
                is_active: false,
                is_dirty: false,
                is_unread: false,
                index: i,
            })
            .collect();

        let geom_no_scroll = calculate_tab_bar_geometry(800.0, &tabs, test_glyph_width(), 0.0);
        let geom_with_scroll = calculate_tab_bar_geometry(800.0, &tabs, test_glyph_width(), 50.0);

        // With scroll offset, tabs should be shifted left
        if !geom_no_scroll.tab_rects.is_empty() && !geom_with_scroll.tab_rects.is_empty() {
            assert!(geom_with_scroll.tab_rects[0].x < geom_no_scroll.tab_rects[0].x);
        }
    }

    // =========================================================================
    // TabInfo Tests
    // =========================================================================

    #[test]
    fn test_tab_info_dirty() {
        let tabs = vec![TabInfo {
            label: "test.rs".to_string(),
            is_active: false,
            is_dirty: true,
            is_unread: false,
            index: 0,
        }];

        assert!(tabs[0].is_dirty);
        assert!(!tabs[0].is_unread);
    }

    #[test]
    fn test_tab_info_unread() {
        let tabs = vec![TabInfo {
            label: "Terminal".to_string(),
            is_active: false,
            is_dirty: false,
            is_unread: true,
            index: 0,
        }];

        assert!(!tabs[0].is_dirty);
        assert!(tabs[0].is_unread);
    }

    // =========================================================================
    // Disambiguation Tests
    // =========================================================================

    #[test]
    fn test_no_disambiguation_for_unique_names() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_buffer::TextBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Add tabs with unique names
        let tab1 = Tab::new_file(1, TextBuffer::new(), "file1.rs".to_string(), Some(PathBuf::from("/test/file1.rs")), 16.0);
        let tab2 = Tab::new_file(2, TextBuffer::new(), "file2.rs".to_string(), Some(PathBuf::from("/test/file2.rs")), 16.0);

        ws.tabs.push(tab1);
        ws.tabs.push(tab2);

        let tabs = tabs_from_workspace(&ws);

        // Labels should remain unchanged
        assert_eq!(tabs[0].label, "file1.rs");
        assert_eq!(tabs[1].label, "file2.rs");
    }

    #[test]
    fn test_disambiguation_for_duplicate_names() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_buffer::TextBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Add tabs with the same name but different paths
        let tab1 = Tab::new_file(1, TextBuffer::new(), "main.rs".to_string(), Some(PathBuf::from("/test/src/main.rs")), 16.0);
        let tab2 = Tab::new_file(2, TextBuffer::new(), "main.rs".to_string(), Some(PathBuf::from("/test/tests/main.rs")), 16.0);

        ws.tabs.push(tab1);
        ws.tabs.push(tab2);

        let tabs = tabs_from_workspace(&ws);

        // Labels should include parent directory
        assert_eq!(tabs[0].label, "src/main.rs");
        assert_eq!(tabs[1].label, "tests/main.rs");
    }

    #[test]
    fn test_disambiguation_with_three_duplicates() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_buffer::TextBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Add three tabs with the same name
        let tab1 = Tab::new_file(1, TextBuffer::new(), "lib.rs".to_string(), Some(PathBuf::from("/test/a/lib.rs")), 16.0);
        let tab2 = Tab::new_file(2, TextBuffer::new(), "lib.rs".to_string(), Some(PathBuf::from("/test/b/lib.rs")), 16.0);
        let tab3 = Tab::new_file(3, TextBuffer::new(), "lib.rs".to_string(), Some(PathBuf::from("/test/c/lib.rs")), 16.0);

        ws.tabs.push(tab1);
        ws.tabs.push(tab2);
        ws.tabs.push(tab3);

        let tabs = tabs_from_workspace(&ws);

        // All labels should include parent directory
        assert_eq!(tabs[0].label, "a/lib.rs");
        assert_eq!(tabs[1].label, "b/lib.rs");
        assert_eq!(tabs[2].label, "c/lib.rs");
    }

    #[test]
    fn test_disambiguation_mixed_unique_and_duplicate() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_buffer::TextBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Add a unique tab and two duplicates
        let tab1 = Tab::new_file(1, TextBuffer::new(), "unique.rs".to_string(), Some(PathBuf::from("/test/unique.rs")), 16.0);
        let tab2 = Tab::new_file(2, TextBuffer::new(), "mod.rs".to_string(), Some(PathBuf::from("/test/foo/mod.rs")), 16.0);
        let tab3 = Tab::new_file(3, TextBuffer::new(), "mod.rs".to_string(), Some(PathBuf::from("/test/bar/mod.rs")), 16.0);

        ws.tabs.push(tab1);
        ws.tabs.push(tab2);
        ws.tabs.push(tab3);

        let tabs = tabs_from_workspace(&ws);

        // Unique label stays unchanged, duplicates get parent
        assert_eq!(tabs[0].label, "unique.rs");
        assert_eq!(tabs[1].label, "foo/mod.rs");
        assert_eq!(tabs[2].label, "bar/mod.rs");
    }

    // =========================================================================
    // Derived Label Tests (Chunk: docs/chunks/tab_bar_interaction)
    // =========================================================================

    #[test]
    fn test_file_tab_label_derived_from_associated_file() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_buffer::TextBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Create a file tab with a stale label but valid associated_file
        // The label should be derived from associated_file, not from tab.label
        let tab = Tab::new_file(
            1,
            TextBuffer::new(),
            "stale_label.txt".to_string(), // This is the stale snapshot
            Some(PathBuf::from("/test/actual_filename.rs")), // This is the truth
            16.0,
        );
        ws.tabs.push(tab);

        let tabs = tabs_from_workspace(&ws);

        // Label should come from associated_file's file_name(), not the stale label
        assert_eq!(tabs[0].label, "actual_filename.rs");
    }

    #[test]
    fn test_file_tab_label_untitled_when_no_associated_file() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_buffer::TextBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Create a file tab with no associated file
        let tab = Tab::new_file(
            1,
            TextBuffer::new(),
            "SomeLabel".to_string(),
            None, // No associated file
            16.0,
        );
        ws.tabs.push(tab);

        let tabs = tabs_from_workspace(&ws);

        // Label should fall back to "Untitled" when associated_file is None
        assert_eq!(tabs[0].label, "Untitled");
    }

    #[test]
    fn test_non_file_tab_uses_static_label() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};
        use lite_edit_terminal::TerminalBuffer;

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Create a terminal tab - should use the static label
        let terminal = TerminalBuffer::new(80, 24, 1000);
        let tab = Tab::new_terminal(1, terminal, "My Terminal".to_string(), 16.0);
        ws.tabs.push(tab);

        let tabs = tabs_from_workspace(&ws);

        // Terminal tabs should use the static tab.label
        assert_eq!(tabs[0].label, "My Terminal");
    }

    #[test]
    fn test_agent_tab_uses_static_label() {
        use std::path::PathBuf;
        use crate::workspace::{Tab, Workspace};

        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Create an agent tab - should use the static label
        let tab = Tab::new_agent(1, "Claude Agent".to_string(), 16.0);
        ws.tabs.push(tab);

        let tabs = tabs_from_workspace(&ws);

        // Agent tabs should use the static tab.label
        assert_eq!(tabs[0].label, "Claude Agent");
    }

    // =========================================================================
    // Left-Truncation Tests (Chunk: docs/chunks/tab_bar_interaction)
    // =========================================================================

    #[test]
    fn test_left_truncation_preserves_end() {
        // Test helper function for left-truncation behavior
        // This mirrors the logic that will be implemented in TabBarGlyphBuffer::update
        fn left_truncate(label: &str, max_chars: usize) -> String {
            let char_count = label.chars().count();
            if char_count > max_chars && max_chars > 1 {
                // Left-truncate: skip (char_count - max_chars + 1) chars, prepend ellipsis
                let skip = char_count - max_chars + 1;
                let truncated: String = label.chars().skip(skip).collect();
                format!("…{}", truncated)
            } else if char_count > max_chars {
                // max_chars is 0 or 1, and label is longer - just show ellipsis or nothing
                if max_chars >= 1 {
                    "…".to_string()
                } else {
                    String::new()
                }
            } else {
                // Label fits within max_chars
                label.to_string()
            }
        }

        // Long filename should be left-truncated
        assert_eq!(
            left_truncate("very_long_module_name.rs", 10),
            "…e_name.rs"
        );

        // Disambiguated path should also be left-truncated
        assert_eq!(
            left_truncate("src/main.rs", 8),
            "…main.rs"
        );

        // Short labels should be unchanged
        assert_eq!(
            left_truncate("short.rs", 10),
            "short.rs"
        );

        // Exact fit should be unchanged
        assert_eq!(
            left_truncate("exactly10!", 10),
            "exactly10!"
        );

        // Edge case: max_chars = 1 should just be ellipsis
        assert_eq!(
            left_truncate("anything", 1),
            "…"
        );

        // Edge case: max_chars = 0 should be empty
        assert_eq!(
            left_truncate("anything", 0),
            ""
        );
    }
}
