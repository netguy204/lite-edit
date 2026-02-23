// Chunk: docs/chunks/tiling_multi_pane_render - Pane frame rendering (dividers + focus border)
//!
//! Pane frame layout and rendering for multi-pane tiling.
//!
//! This module provides layout calculation and vertex buffer construction for
//! rendering pane visual elements:
//! - **Divider lines**: 1px lines at the boundary between adjacent panes
//! - **Focus border**: 2px colored border around the focused pane
//!
//! Following the project's Humble View Architecture, geometry calculations are
//! pure functions that can be unit tested without Metal dependencies.

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};

use crate::glyph_atlas::{GlyphAtlas, GlyphInfo};
use crate::glyph_buffer::{GlyphVertex, QuadRange};
use crate::pane_layout::{PaneId, PaneRect};
use crate::shader::VERTEX_SIZE;

// =============================================================================
// Layout Constants
// =============================================================================

/// Width of divider lines between panes (in pixels)
pub const DIVIDER_WIDTH: f32 = 1.0;

/// Width of the focus border around the active pane (in pixels)
pub const FOCUS_BORDER_WIDTH: f32 = 2.0;

// =============================================================================
// Divider Line Calculation
// =============================================================================

/// A divider line between two adjacent panes.
#[derive(Debug, Clone, PartialEq)]
pub struct DividerLine {
    /// X position of the line start
    pub x: f32,
    /// Y position of the line start
    pub y: f32,
    /// Width of the line (1.0 for vertical dividers, full width for horizontal)
    pub width: f32,
    /// Height of the line (1.0 for horizontal dividers, full height for vertical)
    pub height: f32,
}

impl DividerLine {
    /// Creates a vertical divider line at the given x position.
    pub fn vertical(x: f32, y: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: DIVIDER_WIDTH,
            height,
        }
    }

    /// Creates a horizontal divider line at the given y position.
    pub fn horizontal(x: f32, y: f32, width: f32) -> Self {
        Self {
            x,
            y,
            width,
            height: DIVIDER_WIDTH,
        }
    }
}

/// Calculates divider lines from pane rectangles.
///
/// Dividers appear at the boundary between adjacent panes.
/// Deduplicates overlapping dividers at shared edges.
///
/// # Algorithm
///
/// For each pair of panes, check if they share an edge:
/// - If they share a vertical edge: add a vertical divider at that x
/// - If they share a horizontal edge: add a horizontal divider at that y
///
/// Then deduplicate lines that are at the same position and overlap.
pub fn calculate_divider_lines(pane_rects: &[PaneRect]) -> Vec<DividerLine> {
    if pane_rects.len() <= 1 {
        return Vec::new();
    }

    let mut lines = Vec::new();

    // Check each pair of panes for shared edges
    for i in 0..pane_rects.len() {
        for j in (i + 1)..pane_rects.len() {
            let a = &pane_rects[i];
            let b = &pane_rects[j];

            // Check for vertical edge (a's right edge == b's left edge or vice versa)
            let a_right = a.x + a.width;
            let b_right = b.x + b.width;

            // Tolerance for floating point comparison
            const EPSILON: f32 = 0.5;

            if (a_right - b.x).abs() < EPSILON {
                // a is to the left of b, shared vertical edge at a_right
                let y_min = a.y.max(b.y);
                let y_max = (a.y + a.height).min(b.y + b.height);
                if y_max > y_min {
                    lines.push(DividerLine::vertical(a_right - DIVIDER_WIDTH / 2.0, y_min, y_max - y_min));
                }
            } else if (b_right - a.x).abs() < EPSILON {
                // b is to the left of a, shared vertical edge at b_right
                let y_min = a.y.max(b.y);
                let y_max = (a.y + a.height).min(b.y + b.height);
                if y_max > y_min {
                    lines.push(DividerLine::vertical(b_right - DIVIDER_WIDTH / 2.0, y_min, y_max - y_min));
                }
            }

            // Check for horizontal edge (a's bottom edge == b's top edge or vice versa)
            let a_bottom = a.y + a.height;
            let b_bottom = b.y + b.height;

            if (a_bottom - b.y).abs() < EPSILON {
                // a is above b, shared horizontal edge at a_bottom
                let x_min = a.x.max(b.x);
                let x_max = (a.x + a.width).min(b.x + b.width);
                if x_max > x_min {
                    lines.push(DividerLine::horizontal(x_min, a_bottom - DIVIDER_WIDTH / 2.0, x_max - x_min));
                }
            } else if (b_bottom - a.y).abs() < EPSILON {
                // b is above a, shared horizontal edge at b_bottom
                let x_min = a.x.max(b.x);
                let x_max = (a.x + a.width).min(b.x + b.width);
                if x_max > x_min {
                    lines.push(DividerLine::horizontal(x_min, b_bottom - DIVIDER_WIDTH / 2.0, x_max - x_min));
                }
            }
        }
    }

    // Deduplicate overlapping lines at the same position
    deduplicate_divider_lines(&mut lines);

    lines
}

/// Deduplicates divider lines that are at the same position.
///
/// Merges overlapping lines into a single line spanning the combined range.
fn deduplicate_divider_lines(lines: &mut Vec<DividerLine>) {
    if lines.len() <= 1 {
        return;
    }

    // Sort lines by position for grouping
    lines.sort_by(|a, b| {
        // Group by x then y
        a.x.partial_cmp(&b.x).unwrap()
            .then_with(|| a.y.partial_cmp(&b.y).unwrap())
    });

    // Merge overlapping lines
    let mut i = 0;
    while i < lines.len() - 1 {
        let current = &lines[i];
        let next = &lines[i + 1];

        const EPSILON: f32 = 0.5;

        // Check if same position and overlapping
        let same_x = (current.x - next.x).abs() < EPSILON;
        let same_width = (current.width - next.width).abs() < EPSILON;
        let same_y = (current.y - next.y).abs() < EPSILON;
        let same_height = (current.height - next.height).abs() < EPSILON;

        if same_x && same_width && current.width.abs() < 2.0 {
            // Vertical lines at same x - check if overlapping
            let current_bottom = current.y + current.height;
            if next.y <= current_bottom + EPSILON {
                // Merge
                let new_height = (next.y + next.height - current.y).max(current.height);
                lines[i].height = new_height;
                lines.remove(i + 1);
                continue;
            }
        } else if same_y && same_height && current.height.abs() < 2.0 {
            // Horizontal lines at same y - check if overlapping
            let current_right = current.x + current.width;
            if next.x <= current_right + EPSILON {
                // Merge
                let new_width = (next.x + next.width - current.x).max(current.width);
                lines[i].width = new_width;
                lines.remove(i + 1);
                continue;
            }
        }

        i += 1;
    }
}

// =============================================================================
// Focus Border Calculation
// =============================================================================

/// A focus border segment (one of the four sides).
#[derive(Debug, Clone)]
pub struct FocusBorderSegment {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Calculates the four border segments for the focused pane.
///
/// Returns segments for top, right, bottom, left borders.
/// The borders are drawn on the inside of the pane rect.
pub fn calculate_focus_border(pane_rect: &PaneRect) -> [FocusBorderSegment; 4] {
    let x = pane_rect.x;
    let y = pane_rect.y;
    let w = pane_rect.width;
    let h = pane_rect.height;
    let bw = FOCUS_BORDER_WIDTH;

    [
        // Top border
        FocusBorderSegment {
            x,
            y,
            width: w,
            height: bw,
        },
        // Right border
        FocusBorderSegment {
            x: x + w - bw,
            y,
            width: bw,
            height: h,
        },
        // Bottom border
        FocusBorderSegment {
            x,
            y: y + h - bw,
            width: w,
            height: bw,
        },
        // Left border
        FocusBorderSegment {
            x,
            y,
            width: bw,
            height: h,
        },
    ]
}

// =============================================================================
// PaneFrameBuffer
// =============================================================================

/// Manages vertex and index buffers for pane frame rendering.
///
/// Renders:
/// - Divider lines between panes (1px)
/// - Focus border around the active pane (2px)
pub struct PaneFrameBuffer {
    /// The vertex buffer containing quad vertices
    vertex_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// The index buffer for drawing triangles
    index_buffer: Option<Retained<ProtocolObject<dyn MTLBuffer>>>,
    /// Total number of indices
    index_count: usize,

    // Quad ranges for different draw phases
    /// Divider line quads
    divider_range: QuadRange,
    /// Focus border quads
    focus_border_range: QuadRange,
}

impl PaneFrameBuffer {
    /// Creates a new empty pane frame buffer.
    pub fn new() -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            index_count: 0,
            divider_range: QuadRange::default(),
            focus_border_range: QuadRange::default(),
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

    /// Returns the index range for divider lines.
    pub fn divider_range(&self) -> QuadRange {
        self.divider_range
    }

    /// Returns the index range for the focus border.
    pub fn focus_border_range(&self) -> QuadRange {
        self.focus_border_range
    }

    /// Updates the buffers with current pane layout.
    ///
    /// Builds vertex data for divider lines and focus border.
    ///
    /// # Arguments
    /// * `device` - The Metal device for buffer creation
    /// * `pane_rects` - The computed pane rectangles
    /// * `focused_pane_id` - The ID of the focused pane
    /// * `atlas` - The glyph atlas (for solid glyph)
    /// * `divider_color` - Color for divider lines
    /// * `focus_color` - Color for focus border
    pub fn update(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        pane_rects: &[PaneRect],
        focused_pane_id: PaneId,
        atlas: &GlyphAtlas,
        divider_color: [f32; 4],
        focus_color: [f32; 4],
    ) {
        // Calculate divider lines
        let divider_lines = calculate_divider_lines(pane_rects);

        // Find the focused pane rect
        let focused_rect = pane_rects.iter().find(|r| r.pane_id == focused_pane_id);

        // Estimate capacity
        let divider_count = divider_lines.len();
        let border_count = if focused_rect.is_some() && pane_rects.len() > 1 { 4 } else { 0 };
        let total_quads = divider_count + border_count;

        if total_quads == 0 {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            self.divider_range = QuadRange::default();
            self.focus_border_range = QuadRange::default();
            return;
        }

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(total_quads * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(total_quads * 6);
        let mut vertex_offset: u32 = 0;

        let solid_glyph = atlas.solid_glyph();

        // ==================== Divider Lines ====================
        let divider_start = indices.len();
        for line in &divider_lines {
            let quad = create_rect_quad(
                line.x,
                line.y,
                line.width,
                line.height,
                solid_glyph,
                divider_color,
            );
            vertices.extend_from_slice(&quad);
            push_quad_indices(&mut indices, vertex_offset);
            vertex_offset += 4;
        }
        self.divider_range = QuadRange::new(divider_start, indices.len() - divider_start);

        // ==================== Focus Border ====================
        let border_start = indices.len();
        if let Some(rect) = focused_rect {
            // Only draw focus border when there are multiple panes
            if pane_rects.len() > 1 {
                let border_segments = calculate_focus_border(rect);
                for segment in &border_segments {
                    let quad = create_rect_quad(
                        segment.x,
                        segment.y,
                        segment.width,
                        segment.height,
                        solid_glyph,
                        focus_color,
                    );
                    vertices.extend_from_slice(&quad);
                    push_quad_indices(&mut indices, vertex_offset);
                    vertex_offset += 4;
                }
            }
        }
        self.focus_border_range = QuadRange::new(border_start, indices.len() - border_start);

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
}

impl Default for PaneFrameBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Creates a solid rectangle quad at the given position.
fn create_rect_quad(
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // calculate_divider_lines Tests
    // =========================================================================

    #[test]
    fn test_single_pane_no_dividers() {
        let pane_rects = vec![PaneRect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
            pane_id: 1,
        }];

        let lines = calculate_divider_lines(&pane_rects);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_horizontal_split_one_divider() {
        // Two panes side by side (horizontal split)
        let pane_rects = vec![
            PaneRect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 600.0,
                pane_id: 1,
            },
            PaneRect {
                x: 400.0,
                y: 0.0,
                width: 400.0,
                height: 600.0,
                pane_id: 2,
            },
        ];

        let lines = calculate_divider_lines(&pane_rects);
        assert_eq!(lines.len(), 1);

        // Should be a vertical divider at x=400
        let line = &lines[0];
        assert!((line.x - 399.5).abs() < 1.0, "x should be around 399.5, got {}", line.x);
        assert_eq!(line.y, 0.0);
        assert!((line.width - DIVIDER_WIDTH).abs() < 0.01);
        assert!((line.height - 600.0).abs() < 0.01);
    }

    #[test]
    fn test_vertical_split_one_divider() {
        // Two panes stacked (vertical split)
        let pane_rects = vec![
            PaneRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 300.0,
                pane_id: 1,
            },
            PaneRect {
                x: 0.0,
                y: 300.0,
                width: 800.0,
                height: 300.0,
                pane_id: 2,
            },
        ];

        let lines = calculate_divider_lines(&pane_rects);
        assert_eq!(lines.len(), 1);

        // Should be a horizontal divider at y=300
        let line = &lines[0];
        assert_eq!(line.x, 0.0);
        assert!((line.y - 299.5).abs() < 1.0, "y should be around 299.5, got {}", line.y);
        assert!((line.width - 800.0).abs() < 0.01);
        assert!((line.height - DIVIDER_WIDTH).abs() < 0.01);
    }

    #[test]
    fn test_nested_splits_multiple_dividers() {
        // HSplit(A, VSplit(B, C))
        // +---+---+
        // | A | B |
        // |   +---+
        // |   | C |
        // +---+---+
        let pane_rects = vec![
            PaneRect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 600.0,
                pane_id: 1, // A
            },
            PaneRect {
                x: 400.0,
                y: 0.0,
                width: 400.0,
                height: 300.0,
                pane_id: 2, // B
            },
            PaneRect {
                x: 400.0,
                y: 300.0,
                width: 400.0,
                height: 300.0,
                pane_id: 3, // C
            },
        ];

        let lines = calculate_divider_lines(&pane_rects);

        // Should have:
        // - One vertical divider between A and (B, C) at x=400
        // - One horizontal divider between B and C at y=300 (only right half)
        assert_eq!(lines.len(), 2, "Expected 2 dividers, got {:?}", lines);
    }

    #[test]
    fn test_empty_pane_rects_no_dividers() {
        let pane_rects: Vec<PaneRect> = vec![];
        let lines = calculate_divider_lines(&pane_rects);
        assert!(lines.is_empty());
    }

    // =========================================================================
    // calculate_focus_border Tests
    // =========================================================================

    #[test]
    fn test_focus_border_segments() {
        let pane_rect = PaneRect {
            x: 100.0,
            y: 50.0,
            width: 400.0,
            height: 300.0,
            pane_id: 1,
        };

        let segments = calculate_focus_border(&pane_rect);
        assert_eq!(segments.len(), 4);

        // Top border
        assert_eq!(segments[0].x, 100.0);
        assert_eq!(segments[0].y, 50.0);
        assert_eq!(segments[0].width, 400.0);
        assert_eq!(segments[0].height, FOCUS_BORDER_WIDTH);

        // Right border
        assert_eq!(segments[1].x, 100.0 + 400.0 - FOCUS_BORDER_WIDTH);
        assert_eq!(segments[1].y, 50.0);
        assert_eq!(segments[1].width, FOCUS_BORDER_WIDTH);
        assert_eq!(segments[1].height, 300.0);

        // Bottom border
        assert_eq!(segments[2].x, 100.0);
        assert_eq!(segments[2].y, 50.0 + 300.0 - FOCUS_BORDER_WIDTH);
        assert_eq!(segments[2].width, 400.0);
        assert_eq!(segments[2].height, FOCUS_BORDER_WIDTH);

        // Left border
        assert_eq!(segments[3].x, 100.0);
        assert_eq!(segments[3].y, 50.0);
        assert_eq!(segments[3].width, FOCUS_BORDER_WIDTH);
        assert_eq!(segments[3].height, 300.0);
    }

    // =========================================================================
    // PaneFrameBuffer Tests
    // =========================================================================

    #[test]
    fn test_pane_frame_buffer_new() {
        let buffer = PaneFrameBuffer::new();
        assert!(buffer.vertex_buffer.is_none());
        assert!(buffer.index_buffer.is_none());
        assert_eq!(buffer.index_count, 0);
        assert!(buffer.divider_range.is_empty());
        assert!(buffer.focus_border_range.is_empty());
    }
}
