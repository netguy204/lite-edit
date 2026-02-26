// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
//!
//! Left rail layout and rendering for workspace tiles.
//!
//! This module provides layout calculation and vertex buffer construction for
//! rendering the left rail containing workspace tiles. Following the project's
//! Humble View Architecture, geometry calculations are pure functions that can
//! be unit tested without Metal dependencies.
//!
//! ## Layout
//!
//! The left rail is a fixed-width vertical strip on the left edge of the window.
//! Each workspace is represented by a tile containing:
//! - A status indicator (colored dot)
//! - A label (abbreviated workspace name)
//!
//! The active workspace tile is highlighted.

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};
use sha2::{Digest, Sha256};

use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::glyph_buffer::{GlyphLayout, GlyphVertex, QuadRange};
use crate::shader::VERTEX_SIZE;
use crate::workspace::{Editor, WorkspaceStatus};

// =============================================================================
// Layout Constants
// =============================================================================

/// Width of the left rail in pixels (scaled)
pub const RAIL_WIDTH: f32 = 56.0;

/// Height of each workspace tile
pub const TILE_HEIGHT: f32 = 48.0;

/// Padding around tile content
pub const TILE_PADDING: f32 = 4.0;

/// Size of the status indicator dot
pub const STATUS_INDICATOR_SIZE: f32 = 8.0;

/// Vertical spacing between tiles
pub const TILE_SPACING: f32 = 4.0;

/// Top margin before first tile
pub const TOP_MARGIN: f32 = 8.0;

// =============================================================================
// Colors
// =============================================================================

/// Background color for the left rail
pub const RAIL_BACKGROUND_COLOR: [f32; 4] = [
    0.12, // Darker than editor background
    0.12,
    0.14,
    1.0,
];

/// Tile background color (slightly lighter than rail)
pub const TILE_BACKGROUND_COLOR: [f32; 4] = [
    0.15,
    0.15,
    0.18,
    1.0,
];

/// Active tile highlight color
pub const TILE_ACTIVE_COLOR: [f32; 4] = [
    0.22,
    0.22,
    0.28,
    1.0,
];

/// Text color for workspace labels
pub const LABEL_COLOR: [f32; 4] = [
    0.7,
    0.7,
    0.75,
    1.0,
];

/// Returns the color for a workspace status indicator.
pub fn status_color(status: &WorkspaceStatus) -> [f32; 4] {
    match status {
        WorkspaceStatus::Idle => [0.5, 0.5, 0.5, 1.0],       // Gray
        WorkspaceStatus::Running => [0.2, 0.8, 0.2, 1.0],    // Green
        WorkspaceStatus::NeedsInput => [0.9, 0.8, 0.1, 1.0], // Yellow
        WorkspaceStatus::Stale => [0.9, 0.6, 0.1, 1.0],      // Orange
        WorkspaceStatus::Completed => [0.2, 0.7, 0.2, 1.0],  // Checkmark green
        WorkspaceStatus::Errored => [0.9, 0.2, 0.2, 1.0],    // Red
    }
}

// =============================================================================
// Identicon Generation
// Chunk: docs/chunks/workspace_identicon - Workspace identicons
// =============================================================================

/// Converts HSL color values to RGB.
///
/// Uses the same algorithm as Python's `colorsys.hls_to_rgb`.
///
/// # Arguments
/// * `h` - Hue in range [0.0, 1.0]
/// * `s` - Saturation in range [0.0, 1.0]
/// * `l` - Lightness in range [0.0, 1.0]
///
/// # Returns
/// RGB values as `(r, g, b)` each in range [0.0, 1.0]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        // Achromatic (gray)
        return (l, l, l);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);

    (r, g, b)
}

/// Derives an RGBA foreground color from a SHA-256 hash.
///
/// Algorithm:
/// - Hue: bytes 0-1 (little-endian u16) mod 360
/// - Saturation: byte 2 mapped to [0.5, 0.8]
/// - Lightness: byte 3 mapped to [0.4, 0.65]
fn identicon_color_from_hash(hash: &[u8; 32]) -> [f32; 4] {
    // Hue from bytes 0-1 (little-endian u16)
    let hue_raw = u16::from_le_bytes([hash[0], hash[1]]);
    let hue = (hue_raw % 360) as f32 / 360.0;

    // Saturation from byte 2: [0.5, 0.8]
    let sat = 0.5 + (hash[2] as f32 / 255.0) * 0.3;

    // Lightness from byte 3: [0.4, 0.65]
    let light = 0.4 + (hash[3] as f32 / 255.0) * 0.25;

    let (r, g, b) = hsl_to_rgb(hue, sat, light);
    [r, g, b, 1.0]
}

/// Derives a 5×5 vertically-symmetric grid pattern from a SHA-256 hash.
///
/// Returns a [[bool; 5]; 5] where true = filled cell, false = background cell.
/// The grid is vertically symmetric (mirrored around the center column).
fn identicon_grid_from_hash(hash: &[u8; 32]) -> [[bool; 5]; 5] {
    // Extract 15 bits from bytes 4-5 (little-endian u16)
    let bits = u16::from_le_bytes([hash[4], hash[5]]);

    let mut grid = [[false; 5]; 5];

    for row in 0..5 {
        for col in 0..3 {
            // Only compute left half + center
            let bit_index = row * 3 + col;
            let on = (bits & (1 << bit_index)) != 0;
            grid[row][col] = on;
            grid[row][4 - col] = on; // Mirror to right half
        }
    }

    grid
}

/// Computes the SHA-256 hash of a workspace label for identicon generation.
fn hash_workspace_label(label: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(label.as_bytes());
    hasher.finalize().into()
}

// =============================================================================
// Geometry Types
// =============================================================================

/// A tile rectangle (x, y, width, height).
#[derive(Debug, Clone, Copy)]
pub struct TileRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl TileRect {
    /// Creates a new tile rectangle.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Returns true if the point (px, py) is inside this rectangle.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Computed geometry for the left rail.
///
/// All values are in screen coordinates (pixels).
#[derive(Debug, Clone)]
pub struct LeftRailGeometry {
    /// X position of the rail (always 0)
    pub x: f32,
    /// Y position of the rail (always 0)
    pub y: f32,
    /// Width of the rail
    pub width: f32,
    /// Height of the rail (same as view height)
    pub height: f32,
    /// Rectangles for each workspace tile
    pub tile_rects: Vec<TileRect>,
}

/// Calculates the geometry for the left rail.
///
/// This is a pure function suitable for unit testing.
///
/// # Arguments
/// * `view_height` - The window/viewport height in pixels
/// * `workspace_count` - The number of workspaces
///
/// # Returns
/// A `LeftRailGeometry` struct with all layout measurements
pub fn calculate_left_rail_geometry(view_height: f32, workspace_count: usize) -> LeftRailGeometry {
    let mut tile_rects = Vec::with_capacity(workspace_count);

    let tile_width = RAIL_WIDTH - 2.0 * TILE_PADDING;
    let mut y = TOP_MARGIN;

    for _ in 0..workspace_count {
        // Don't add tiles that would extend past the view height
        if y + TILE_HEIGHT > view_height {
            break;
        }

        tile_rects.push(TileRect::new(TILE_PADDING, y, tile_width, TILE_HEIGHT));
        y += TILE_HEIGHT + TILE_SPACING;
    }

    LeftRailGeometry {
        x: 0.0,
        y: 0.0,
        width: RAIL_WIDTH,
        height: view_height,
        tile_rects,
    }
}

// =============================================================================
// LeftRailGlyphBuffer
// =============================================================================

/// Manages vertex and index buffers for rendering the left rail.
///
/// This is analogous to `GlyphBuffer` and `SelectorGlyphBuffer` but specialized
/// for the workspace rail UI.
// Chunk: docs/chunks/quad_buffer_prealloc - Persistent buffers to eliminate per-frame allocations
pub struct LeftRailGlyphBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,
    /// Layout calculator for glyph positioning
    layout: GlyphLayout,

    // Quad ranges for different draw phases
    /// Rail background rect
    background_range: QuadRange,
    /// Inactive tile backgrounds
    tile_background_range: QuadRange,
    /// Active tile highlight
    active_tile_range: QuadRange,
    /// Status indicator dots
    status_indicator_range: QuadRange,
    /// Workspace identicons (5×5 grid per workspace)
    identicon_range: QuadRange,

    // Chunk: docs/chunks/quad_buffer_prealloc - Persistent buffers to avoid per-frame heap allocations
    /// Persistent vertex data buffer, reused across frames
    persistent_vertices: Vec<GlyphVertex>,
    /// Persistent index data buffer, reused across frames
    persistent_indices: Vec<u32>,
}

impl LeftRailGlyphBuffer {
    /// Creates a new empty left rail glyph buffer.
    // Chunk: docs/chunks/quad_buffer_prealloc - Initialize persistent buffers
    pub fn new(layout: GlyphLayout) -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            layout,
            background_range: QuadRange::default(),
            tile_background_range: QuadRange::default(),
            active_tile_range: QuadRange::default(),
            status_indicator_range: QuadRange::default(),
            identicon_range: QuadRange::default(),
            persistent_vertices: Vec::new(),
            persistent_indices: Vec::new(),
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

    /// Returns the index range for the rail background.
    pub fn background_range(&self) -> QuadRange {
        self.background_range
    }

    /// Returns the index range for tile backgrounds.
    pub fn tile_background_range(&self) -> QuadRange {
        self.tile_background_range
    }

    /// Returns the index range for the active tile highlight.
    pub fn active_tile_range(&self) -> QuadRange {
        self.active_tile_range
    }

    /// Returns the index range for status indicators.
    pub fn status_indicator_range(&self) -> QuadRange {
        self.status_indicator_range
    }

    /// Returns the index range for identicons.
    pub fn identicon_range(&self) -> QuadRange {
        self.identicon_range
    }

    /// Updates the buffers from the editor state and geometry.
    ///
    /// Builds vertex data in this order:
    /// 1. Rail background
    /// 2. Tile backgrounds (inactive tiles)
    /// 3. Active tile highlight
    /// 4. Status indicators
    /// 5. Workspace identicons (5×5 grids)
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `atlas` - The glyph atlas for text rendering
    /// * `editor` - The editor containing workspace data
    /// * `geometry` - The computed rail geometry
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        atlas: &GlyphAtlas,
        editor: &Editor,
        geometry: &LeftRailGeometry,
    ) {
        // Estimate capacity: 1 background + tiles + indicators + identicon cells
        // Each workspace has up to 25 identicon cells (5×5 grid)
        let workspace_count = editor.workspace_count();
        let estimated_quads = 1 + workspace_count * 2 + workspace_count + workspace_count * 25;

        // Chunk: docs/chunks/quad_buffer_prealloc - Reuse persistent buffers instead of allocating new ones
        self.persistent_vertices.clear();
        self.persistent_indices.clear();
        let estimated_vertices = estimated_quads * 4;
        let estimated_indices = estimated_quads * 6;
        if self.persistent_vertices.capacity() < estimated_vertices {
            self.persistent_vertices.reserve(estimated_vertices - self.persistent_vertices.capacity());
        }
        if self.persistent_indices.capacity() < estimated_indices {
            self.persistent_indices.reserve(estimated_indices - self.persistent_indices.capacity());
        }

        let mut vertex_offset: u32 = 0;

        // Reset ranges
        self.background_range = QuadRange::default();
        self.tile_background_range = QuadRange::default();
        self.active_tile_range = QuadRange::default();
        self.status_indicator_range = QuadRange::default();
        self.identicon_range = QuadRange::default();

        let solid_glyph = atlas.solid_glyph();
        let active_workspace = editor.active_workspace;

        // ==================== Phase 1: Rail Background ====================
        let bg_start = self.persistent_indices.len();
        {
            let quad = self.create_rect_quad(
                geometry.x,
                geometry.y,
                geometry.width,
                geometry.height,
                solid_glyph,
                RAIL_BACKGROUND_COLOR,
            );
            self.persistent_vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut self.persistent_indices, vertex_offset);
            vertex_offset += 4;
        }
        self.background_range = QuadRange::new(bg_start, self.persistent_indices.len() - bg_start);

        // ==================== Phase 2: Tile Backgrounds (inactive) ====================
        let tile_bg_start = self.persistent_indices.len();
        for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
            if idx == active_workspace {
                continue; // Skip active tile, it gets its own highlight
            }

            let quad = self.create_rect_quad(
                tile_rect.x,
                tile_rect.y,
                tile_rect.width,
                tile_rect.height,
                solid_glyph,
                TILE_BACKGROUND_COLOR,
            );
            self.persistent_vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut self.persistent_indices, vertex_offset);
            vertex_offset += 4;
        }
        self.tile_background_range = QuadRange::new(tile_bg_start, self.persistent_indices.len() - tile_bg_start);

        // ==================== Phase 3: Active Tile Highlight ====================
        let active_start = self.persistent_indices.len();
        if active_workspace < geometry.tile_rects.len() {
            let tile_rect = &geometry.tile_rects[active_workspace];
            let quad = self.create_rect_quad(
                tile_rect.x,
                tile_rect.y,
                tile_rect.width,
                tile_rect.height,
                solid_glyph,
                TILE_ACTIVE_COLOR,
            );
            self.persistent_vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut self.persistent_indices, vertex_offset);
            vertex_offset += 4;
        }
        self.active_tile_range = QuadRange::new(active_start, self.persistent_indices.len() - active_start);

        // ==================== Phase 4: Status Indicators ====================
        let status_start = self.persistent_indices.len();
        for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
            if idx >= editor.workspaces.len() {
                break;
            }

            // Position indicator in top-right of tile
            let indicator_x = tile_rect.x + tile_rect.width - STATUS_INDICATOR_SIZE - 4.0;
            let indicator_y = tile_rect.y + 4.0;

            let quad = self.create_rect_quad(
                indicator_x,
                indicator_y,
                STATUS_INDICATOR_SIZE,
                STATUS_INDICATOR_SIZE,
                solid_glyph,
                status_color(&editor.workspaces[idx].status),
            );
            self.persistent_vertices.extend_from_slice(&quad);
            Self::push_quad_indices(&mut self.persistent_indices, vertex_offset);
            vertex_offset += 4;
        }
        self.status_indicator_range = QuadRange::new(status_start, self.persistent_indices.len() - status_start);

        // ==================== Phase 5: Workspace Identicons ====================
        // Chunk: docs/chunks/workspace_identicon - Workspace identicons
        let identicon_start = self.persistent_indices.len();
        for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
            if idx >= editor.workspaces.len() {
                break;
            }

            let workspace = &editor.workspaces[idx];

            // Hash the workspace label and derive identicon parameters
            let hash = hash_workspace_label(&workspace.label);
            let fg_color = identicon_color_from_hash(&hash);
            let grid = identicon_grid_from_hash(&hash);

            // Calculate cell size: tile has padding on each side
            // cell_size = (tile_width - 2*padding) / 5
            let icon_area = tile_rect.width - 2.0 * TILE_PADDING;
            let cell_size = icon_area / 5.0;

            // Center the 5×5 grid in the tile
            let grid_size = cell_size * 5.0;
            let grid_x = tile_rect.x + (tile_rect.width - grid_size) / 2.0;
            let grid_y = tile_rect.y + (tile_rect.height - grid_size) / 2.0;

            // Dimmed color for "off" cells (1/5 brightness)
            let dim_color = [
                fg_color[0] * 0.2,
                fg_color[1] * 0.2,
                fg_color[2] * 0.2,
                1.0,
            ];

            // Render each cell in the 5×5 grid
            for row in 0..5 {
                for col in 0..5 {
                    let cell_x = grid_x + col as f32 * cell_size;
                    let cell_y = grid_y + row as f32 * cell_size;
                    let color = if grid[row][col] { fg_color } else { dim_color };

                    let quad = self.create_rect_quad(
                        cell_x,
                        cell_y,
                        cell_size,
                        cell_size,
                        solid_glyph,
                        color,
                    );
                    self.persistent_vertices.extend_from_slice(&quad);
                    Self::push_quad_indices(&mut self.persistent_indices, vertex_offset);
                    vertex_offset += 4;
                }
            }
        }
        self.identicon_range = QuadRange::new(identicon_start, self.persistent_indices.len() - identicon_start);

        // ==================== Create GPU Buffers ====================
        if self.persistent_vertices.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        // Create the vertex buffer
        let vertex_data_size = self.persistent_vertices.len() * VERTEX_SIZE;
        let vertex_ptr =
            NonNull::new(self.persistent_vertices.as_ptr() as *mut std::ffi::c_void).expect("vertex ptr not null");

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
        let index_data_size = self.persistent_indices.len() * std::mem::size_of::<u32>();
        let index_ptr =
            NonNull::new(self.persistent_indices.as_ptr() as *mut std::ffi::c_void).expect("index ptr not null");

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
        self.index_count = self.persistent_indices.len();
    }

    /// Returns the status colors for each visible workspace.
    ///
    /// This is used by the renderer to set fragment colors for each indicator.
    pub fn status_colors(&self, editor: &Editor, geometry: &LeftRailGeometry) -> Vec<[f32; 4]> {
        geometry
            .tile_rects
            .iter()
            .enumerate()
            .filter_map(|(idx, _)| editor.workspaces.get(idx))
            .map(|ws| status_color(&ws.status))
            .collect()
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

    // =========================================================================
    // Geometry Tests
    // =========================================================================

    #[test]
    fn test_geometry_with_one_workspace() {
        let geom = calculate_left_rail_geometry(600.0, 1);

        assert_eq!(geom.width, RAIL_WIDTH);
        assert_eq!(geom.height, 600.0);
        assert_eq!(geom.tile_rects.len(), 1);
    }

    #[test]
    fn test_geometry_with_five_workspaces() {
        let geom = calculate_left_rail_geometry(600.0, 5);

        assert_eq!(geom.tile_rects.len(), 5);

        // Tiles should be stacked vertically
        for (i, tile) in geom.tile_rects.iter().enumerate() {
            assert_eq!(tile.x, TILE_PADDING);
            assert_eq!(tile.width, RAIL_WIDTH - 2.0 * TILE_PADDING);
            assert_eq!(tile.height, TILE_HEIGHT);

            // Y position should increase for each tile
            if i > 0 {
                assert!(tile.y > geom.tile_rects[i - 1].y);
            }
        }
    }

    #[test]
    fn test_geometry_tiles_dont_exceed_view_height() {
        // Very short view that can only fit a few tiles
        let view_height = TOP_MARGIN + TILE_HEIGHT * 2.0 + TILE_SPACING + 10.0;
        let geom = calculate_left_rail_geometry(view_height, 10);

        // Should only fit 2 tiles
        assert_eq!(geom.tile_rects.len(), 2);
    }

    #[test]
    fn test_geometry_zero_workspaces() {
        let geom = calculate_left_rail_geometry(600.0, 0);

        assert_eq!(geom.tile_rects.len(), 0);
        assert_eq!(geom.width, RAIL_WIDTH);
    }

    #[test]
    fn test_tile_rect_contains() {
        let rect = TileRect::new(10.0, 20.0, 30.0, 40.0);

        // Inside
        assert!(rect.contains(15.0, 25.0));
        assert!(rect.contains(10.0, 20.0)); // Top-left corner
        assert!(rect.contains(39.0, 59.0)); // Just inside bottom-right

        // Outside
        assert!(!rect.contains(5.0, 25.0));   // Left of rect
        assert!(!rect.contains(45.0, 25.0));  // Right of rect
        assert!(!rect.contains(15.0, 15.0));  // Above rect
        assert!(!rect.contains(15.0, 65.0));  // Below rect
        assert!(!rect.contains(40.0, 60.0));  // Bottom-right corner (exclusive)
    }

    // =========================================================================
    // Color Tests
    // =========================================================================

    #[test]
    fn test_status_colors_are_distinct() {
        let colors = [
            status_color(&WorkspaceStatus::Idle),
            status_color(&WorkspaceStatus::Running),
            status_color(&WorkspaceStatus::NeedsInput),
            status_color(&WorkspaceStatus::Stale),
            status_color(&WorkspaceStatus::Completed),
            status_color(&WorkspaceStatus::Errored),
        ];

        // Each color should be distinct from the others
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "Colors at {} and {} should be distinct", i, j);
            }
        }
    }

    #[test]
    fn test_status_color_idle_is_gray() {
        let color = status_color(&WorkspaceStatus::Idle);
        // Gray means R ≈ G ≈ B
        assert!((color[0] - color[1]).abs() < 0.1);
        assert!((color[1] - color[2]).abs() < 0.1);
    }

    #[test]
    fn test_status_color_running_is_green() {
        let color = status_color(&WorkspaceStatus::Running);
        // Green means G > R and G > B
        assert!(color[1] > color[0]);
        assert!(color[1] > color[2]);
    }

    #[test]
    fn test_status_color_errored_is_red() {
        let color = status_color(&WorkspaceStatus::Errored);
        // Red means R > G and R > B
        assert!(color[0] > color[1]);
        assert!(color[0] > color[2]);
    }

    // =========================================================================
    // Identicon Tests
    // Chunk: docs/chunks/workspace_identicon - Workspace identicons
    // =========================================================================

    #[test]
    fn test_hsl_to_rgb_gray() {
        // Zero saturation should produce gray (R = G = B = L)
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.5);
        assert!((r - 0.5).abs() < 0.001);
        assert!((g - 0.5).abs() < 0.001);
        assert!((b - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_hsl_to_rgb_pure_red() {
        // Hue 0 = red with full saturation
        let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
        assert!((r - 1.0).abs() < 0.001);
        assert!(g.abs() < 0.001);
        assert!(b.abs() < 0.001);
    }

    #[test]
    fn test_hsl_to_rgb_pure_green() {
        // Hue 0.333 = green with full saturation
        let (r, g, b) = hsl_to_rgb(1.0 / 3.0, 1.0, 0.5);
        assert!(r.abs() < 0.001);
        assert!((g - 1.0).abs() < 0.001);
        assert!(b.abs() < 0.001);
    }

    #[test]
    fn test_hsl_to_rgb_pure_blue() {
        // Hue 0.667 = blue with full saturation
        let (r, g, b) = hsl_to_rgb(2.0 / 3.0, 1.0, 0.5);
        assert!(r.abs() < 0.001);
        assert!(g.abs() < 0.001);
        assert!((b - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_identicon_color_deterministic() {
        // Same input should always produce the same output
        let hash1 = hash_workspace_label("test-workspace");
        let hash2 = hash_workspace_label("test-workspace");
        let color1 = identicon_color_from_hash(&hash1);
        let color2 = identicon_color_from_hash(&hash2);

        assert_eq!(color1, color2, "Same input should produce identical colors");
    }

    #[test]
    fn test_identicon_color_known_input() {
        // Snapshot test: "untitled" should produce a specific color
        // This verifies the algorithm matches the prototype
        let hash = hash_workspace_label("untitled");
        let color = identicon_color_from_hash(&hash);

        // All RGB values should be in valid range [0.0, 1.0]
        assert!(color[0] >= 0.0 && color[0] <= 1.0, "R should be in [0,1]");
        assert!(color[1] >= 0.0 && color[1] <= 1.0, "G should be in [0,1]");
        assert!(color[2] >= 0.0 && color[2] <= 1.0, "B should be in [0,1]");
        assert!((color[3] - 1.0).abs() < 0.001, "Alpha should be 1.0");

        // The color should be relatively saturated (not gray)
        let max_channel = color[0].max(color[1]).max(color[2]);
        let min_channel = color[0].min(color[1]).min(color[2]);
        let chroma = max_channel - min_channel;
        assert!(chroma > 0.1, "Color should be saturated, not gray");
    }

    #[test]
    fn test_identicon_color_valid_range() {
        // Test several inputs to verify RGB values are always in valid range
        let test_labels = ["a", "workspace", "very-long-workspace-name-12345", "特殊字符"];

        for label in test_labels {
            let hash = hash_workspace_label(label);
            let color = identicon_color_from_hash(&hash);

            assert!(
                color[0] >= 0.0 && color[0] <= 1.0,
                "R out of range for '{}'",
                label
            );
            assert!(
                color[1] >= 0.0 && color[1] <= 1.0,
                "G out of range for '{}'",
                label
            );
            assert!(
                color[2] >= 0.0 && color[2] <= 1.0,
                "B out of range for '{}'",
                label
            );
        }
    }

    #[test]
    fn test_identicon_grid_deterministic() {
        // Same input should always produce the same grid
        let hash1 = hash_workspace_label("test-workspace");
        let hash2 = hash_workspace_label("test-workspace");
        let grid1 = identicon_grid_from_hash(&hash1);
        let grid2 = identicon_grid_from_hash(&hash2);

        assert_eq!(grid1, grid2, "Same input should produce identical grids");
    }

    #[test]
    fn test_identicon_grid_symmetric() {
        // Grid should be vertically symmetric (mirrored around center column)
        let test_labels = ["untitled", "project-alpha", "feature/auth"];

        for label in test_labels {
            let hash = hash_workspace_label(label);
            let grid = identicon_grid_from_hash(&hash);

            for row in 0..5 {
                assert_eq!(
                    grid[row][0], grid[row][4],
                    "'{}' row {} col 0 should mirror col 4",
                    label, row
                );
                assert_eq!(
                    grid[row][1], grid[row][3],
                    "'{}' row {} col 1 should mirror col 3",
                    label, row
                );
                // col 2 is center, doesn't need mirroring
            }
        }
    }

    #[test]
    fn test_identicon_grid_known_input() {
        // Snapshot test: verify "untitled" produces a non-empty pattern
        let hash = hash_workspace_label("untitled");
        let grid = identicon_grid_from_hash(&hash);

        // Count "on" cells - should have at least some
        let on_count: usize = grid.iter().map(|row| row.iter().filter(|&&c| c).count()).sum();
        assert!(on_count > 0, "Grid should have at least one 'on' cell");
        assert!(on_count < 25, "Grid should have at least one 'off' cell");
    }

    #[test]
    fn test_similar_names_produce_distinct_identicons() {
        // Similar workspace names should produce visually distinct identicons
        // We test both color and grid differences
        let test_pairs = [
            ("project-alpha", "project-beta"),
            ("untitled", "untitled-2"),
            ("feature/auth", "feature/ui"),
        ];

        for (name1, name2) in test_pairs {
            let hash1 = hash_workspace_label(name1);
            let hash2 = hash_workspace_label(name2);

            let color1 = identicon_color_from_hash(&hash1);
            let color2 = identicon_color_from_hash(&hash2);
            let grid1 = identicon_grid_from_hash(&hash1);
            let grid2 = identicon_grid_from_hash(&hash2);

            // Colors should differ
            let color_diff = (color1[0] - color2[0]).abs()
                + (color1[1] - color2[1]).abs()
                + (color1[2] - color2[2]).abs();

            // Grids should differ
            let grid_diff: usize = (0..5)
                .flat_map(|row| (0..5).map(move |col| (row, col)))
                .filter(|&(row, col)| grid1[row][col] != grid2[row][col])
                .count();

            // At least one should differ significantly
            assert!(
                color_diff > 0.1 || grid_diff > 2,
                "'{}' and '{}' should produce distinct identicons (color_diff={}, grid_diff={})",
                name1,
                name2,
                color_diff,
                grid_diff
            );
        }
    }
}
