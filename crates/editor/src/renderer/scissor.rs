// Chunk: docs/chunks/renderer_decomposition - Scissor rect helpers extracted from renderer.rs

//! Scissor rect helpers for clipping rendering to specific screen regions.
//!
//! These functions create `MTLScissorRect` values for various rendering contexts:
//! - Full viewport
//! - Buffer content area (below tab bar)
//! - Pane regions (for multi-pane layouts)
//! - Selector list clipping

use objc2_metal::MTLScissorRect;

use crate::pane_layout::PaneRect;
use crate::selector_overlay::OverlayGeometry;

// Chunk: docs/chunks/selector_list_clipping - Clip item list to panel bounds
/// Creates a scissor rect for clipping the selector item list.
///
/// The rect spans from `list_origin_y` to `panel_y + panel_height`,
/// clipped to the viewport bounds.
pub(super) fn selector_list_scissor_rect(
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
pub(super) fn full_viewport_scissor_rect(view_width: f32, view_height: f32) -> MTLScissorRect {
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
pub(super) fn buffer_content_scissor_rect(
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
pub(super) fn pane_scissor_rect(
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

// Chunk: docs/chunks/tiling_multi_pane_render - Pane content clipping (below tab bar)
/// Creates a scissor rect for a pane's content area (below its tab bar).
pub(super) fn pane_content_scissor_rect(
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
