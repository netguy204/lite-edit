// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
//!
//! Metal shader compilation and pipeline setup
//!
//! This module compiles the glyph rendering shaders at runtime and creates
//! the render pipeline state for drawing textured glyph quads.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlendOperation, MTLDevice, MTLFunction, MTLLibrary, MTLPixelFormat,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLVertexDescriptor, MTLVertexFormat,
    MTLVertexStepFunction,
};

// =============================================================================
// Shader Source
// =============================================================================

/// The Metal shader source code for glyph rendering
const GLYPH_SHADER_SOURCE: &str = include_str!("../shaders/glyph.metal");

// =============================================================================
// Vertex Layout
// =============================================================================

/// Vertex structure layout:
/// - position: float2 (8 bytes) at offset 0
/// - uv: float2 (8 bytes) at offset 8
/// Total: 16 bytes per vertex
pub const VERTEX_SIZE: usize = 16;

/// Creates the vertex descriptor for glyph quad vertices
fn create_vertex_descriptor() -> Retained<MTLVertexDescriptor> {
    let descriptor = MTLVertexDescriptor::new();

    // Get the attributes array
    let attributes = descriptor.attributes();

    // Attribute 0: position (float2) at offset 0
    let attr0 = unsafe { attributes.objectAtIndexedSubscript(0) };
    unsafe {
        attr0.setFormat(MTLVertexFormat::Float2);
        attr0.setOffset(0);
        attr0.setBufferIndex(0);
    }

    // Attribute 1: UV (float2) at offset 8
    let attr1 = unsafe { attributes.objectAtIndexedSubscript(1) };
    unsafe {
        attr1.setFormat(MTLVertexFormat::Float2);
        attr1.setOffset(8);
        attr1.setBufferIndex(0);
    }

    // Configure the buffer layout
    let layouts = descriptor.layouts();
    let layout0 = unsafe { layouts.objectAtIndexedSubscript(0) };
    unsafe {
        layout0.setStride(VERTEX_SIZE);
        layout0.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout0.setStepRate(1);
    }

    descriptor
}

// =============================================================================
// Shader Pipeline
// =============================================================================

/// A compiled shader pipeline for rendering glyphs
pub struct GlyphPipeline {
    /// The compiled render pipeline state
    pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

impl GlyphPipeline {
    /// Creates a new glyph rendering pipeline
    ///
    /// # Arguments
    /// * `device` - The Metal device to create the pipeline on
    ///
    /// # Panics
    /// Panics if shader compilation or pipeline creation fails.
    pub fn new(device: &ProtocolObject<dyn MTLDevice>) -> Self {
        // Compile the shader source
        let library = Self::compile_shader(device);

        // Get the shader functions
        let vertex_function = Self::get_function(&library, "glyph_vertex");
        let fragment_function = Self::get_function(&library, "glyph_fragment");

        // Create the pipeline descriptor
        let descriptor = MTLRenderPipelineDescriptor::new();

        // Set the shader functions
        descriptor.setVertexFunction(Some(&vertex_function));
        descriptor.setFragmentFunction(Some(&fragment_function));

        // Set the vertex descriptor
        let vertex_descriptor = create_vertex_descriptor();
        descriptor.setVertexDescriptor(Some(&vertex_descriptor));

        // Configure the color attachment (matches the drawable's pixel format)
        let color_attachments = descriptor.colorAttachments();
        let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(0) };

        // Set pixel format to match CAMetalLayer (BGRA8Unorm)
        color_attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);

        // Enable alpha blending for anti-aliased glyphs
        // source * source_alpha + dest * (1 - source_alpha)
        color_attachment.setBlendingEnabled(true);
        color_attachment.setSourceRGBBlendFactor(MTLBlendFactor::SourceAlpha);
        color_attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachment.setRgbBlendOperation(MTLBlendOperation::Add);
        color_attachment.setSourceAlphaBlendFactor(MTLBlendFactor::SourceAlpha);
        color_attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
        color_attachment.setAlphaBlendOperation(MTLBlendOperation::Add);

        // Create the pipeline state
        let pipeline_state = device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .expect("Failed to create render pipeline state");

        Self { pipeline_state }
    }

    /// Returns the compiled pipeline state
    pub fn pipeline_state(&self) -> &ProtocolObject<dyn MTLRenderPipelineState> {
        &self.pipeline_state
    }

    /// Compiles the shader source into a Metal library
    fn compile_shader(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Retained<ProtocolObject<dyn MTLLibrary>> {
        let source = NSString::from_str(GLYPH_SHADER_SOURCE);

        device
            .newLibraryWithSource_options_error(&source, None)
            .expect("Failed to compile Metal shader")
    }

    /// Gets a function from the library by name
    fn get_function(
        library: &ProtocolObject<dyn MTLLibrary>,
        name: &str,
    ) -> Retained<ProtocolObject<dyn MTLFunction>> {
        let name_str = NSString::from_str(name);

        library
            .newFunctionWithName(&name_str)
            .unwrap_or_else(|| panic!("Failed to find function: {}", name))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        extern "C" {
            fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
        }

        let ptr = unsafe { MTLCreateSystemDefaultDevice() };
        assert!(!ptr.is_null(), "Metal device should be available");
        unsafe { Retained::from_raw(ptr).unwrap() }
    }

    #[test]
    fn test_shader_compilation() {
        let device = get_test_device();
        // This will panic if compilation fails
        let _pipeline = GlyphPipeline::new(&device);
    }

    #[test]
    fn test_vertex_descriptor() {
        // Just verify it creates without panicking
        let _descriptor = create_vertex_descriptor();
    }
}
