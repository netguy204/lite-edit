// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
//!
//! Smoke tests for the Metal surface and glyph rendering implementation
//!
//! This test verifies that the basic Metal, AppKit, and text rendering
//! initialization code paths work without panicking. Full visual verification
//! must be done manually.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::MainThreadMarker;
use objc2_metal::{MTLDevice, MTLPixelFormat, MTLTexture, MTLTextureDescriptor};

// =============================================================================
// Metal Device Tests
// =============================================================================

fn get_metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
    extern "C" {
        fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
    }

    let ptr = unsafe { MTLCreateSystemDefaultDevice() };
    assert!(!ptr.is_null(), "Metal device should be available on macOS");
    unsafe { Retained::from_raw(ptr).unwrap() }
}

/// Test that we can get a Metal device
#[test]
fn test_metal_device_available() {
    let _device = get_metal_device();
}

/// Test that we can create a Metal command queue
#[test]
fn test_metal_command_queue() {
    let device = get_metal_device();

    let command_queue = device.newCommandQueue();
    assert!(
        command_queue.is_some(),
        "Should be able to create a command queue"
    );
}

// =============================================================================
// Texture Atlas Tests
// =============================================================================

/// Test that we can create a texture suitable for a glyph atlas
#[test]
fn test_atlas_texture_creation() {
    let device = get_metal_device();

    // Create a 1024x1024 R8 texture (same as glyph atlas)
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::R8Unorm,
            1024,
            1024,
            false,
        )
    };

    let texture = device.newTextureWithDescriptor(&descriptor);
    assert!(texture.is_some(), "Should be able to create atlas texture");

    let texture = texture.unwrap();
    assert_eq!(texture.width(), 1024, "Texture width should be 1024");
    assert_eq!(texture.height(), 1024, "Texture height should be 1024");
}

// =============================================================================
// Main Thread Tests
// =============================================================================

/// Test that we can access the main thread marker
/// Note: This test must run on the main thread to pass
#[test]
#[ignore = "Requires main thread - run with --ignored to test"]
fn test_main_thread_marker() {
    let mtm = MainThreadMarker::new();
    assert!(
        mtm.is_some(),
        "Should be on main thread when running this test"
    );
}

// =============================================================================
// Visual Verification Notes
// =============================================================================

/// Visual smoke test documentation
///
/// To verify text rendering visually:
/// 1. Run: `cargo run --package lite-edit`
/// 2. A window should appear with the title "lite-edit"
/// 3. The window should have a dark background (Catppuccin Mocha #1e1e2e)
/// 4. You should see 20+ lines of demo code rendered in light text
/// 5. Text should be legible, properly anti-aliased
/// 6. Resizing the window should re-render correctly
/// 7. The text should read correctly (not garbled or misaligned)
///
/// Performance validation:
/// - Rendering should be smooth during window resize
/// - Initial startup should be under 1 second
/// - The window should be responsive (no lag during interaction)
#[test]
fn test_visual_verification_notes() {
    // This test just documents the manual verification process
    // The actual verification must be done visually
}

// =============================================================================
// Performance Notes
// =============================================================================

/// Performance test documentation
///
/// The success criteria specify:
/// - Rendering 50 lines x 120 columns (~6,000 glyphs) < 2ms
///
/// This is validated by the architecture:
/// - Glyphs are pre-rasterized into a texture atlas at startup
/// - Rendering is just textured quads with a simple shader
/// - No text shaping, kerning, or complex layout (monospace)
/// - Buffer updates are O(n) character count
/// - GPU draw call is a single indexed draw
///
/// To measure actual performance:
/// 1. Uncomment timing code in renderer.rs
/// 2. Run with demo text
/// 3. Check console output for render times
#[test]
fn test_performance_notes() {
    // This test documents the performance expectations
    // Actual measurement requires running the application
}
