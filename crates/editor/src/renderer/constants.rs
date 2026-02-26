// Chunk: docs/chunks/renderer_decomposition - Color constants and uniforms extracted from renderer.rs

//! Color constants and uniform types for the renderer.
//!
//! This module contains the shared color definitions used across all rendering
//! phases and the Uniforms struct passed to shaders.

use objc2_metal::MTLClearColor;

// =============================================================================
// Background Color
// =============================================================================

/// The editor background color: #1e1e2e (Catppuccin Mocha base)
/// Converted to normalized RGB values
pub(super) const BACKGROUND_COLOR: MTLClearColor = MTLClearColor {
    red: 0.118,   // 0x1e / 255
    green: 0.118, // 0x1e / 255
    blue: 0.180,  // 0x2e / 255
    alpha: 1.0,
};

/// The text foreground color: #cdd6f4 (Catppuccin Mocha text)
/// Stored as [R, G, B, A] for passing to the shader
#[allow(dead_code)]
pub(super) const TEXT_COLOR: [f32; 4] = [
    0.804, // 0xcd / 255
    0.839, // 0xd6 / 255
    0.957, // 0xf4 / 255
    1.0,
];

// Chunk: docs/chunks/text_selection_rendering - Selection highlight color constant
/// The selection highlight color: #585b70 (Catppuccin Mocha surface2) at 40% alpha
/// This provides a visible background for selected text without overwhelming it.
#[allow(dead_code)]
pub(super) const SELECTION_COLOR: [f32; 4] = [
    0.345, // 0x58 / 255
    0.357, // 0x5b / 255
    0.439, // 0x70 / 255
    0.4,   // 40% opacity
];

// Chunk: docs/chunks/line_wrap_rendering - Continuation row border color
/// The border color for continuation rows: black (solid)
/// This provides a subtle visual indicator that a line has wrapped.
pub(super) const BORDER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

// Chunk: docs/chunks/tiling_multi_pane_render - Pane divider and focus border colors
/// Pane divider color: #313244 (Catppuccin Mocha surface0)
/// A subtle line between adjacent panes.
pub(super) const PANE_DIVIDER_COLOR: [f32; 4] = [
    0.192, // 0x31 / 255
    0.196, // 0x32 / 255
    0.267, // 0x44 / 255
    1.0,
];

/// Focused pane border color: #89b4fa at 60% (Catppuccin Mocha blue)
/// A colored border to indicate which pane is active.
pub(super) const FOCUSED_PANE_BORDER_COLOR: [f32; 4] = [
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
pub(super) struct Uniforms {
    /// Viewport size in pixels
    pub viewport_size: [f32; 2],
}
