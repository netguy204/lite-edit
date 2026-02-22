// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/renderer_styled_content - Per-vertex color for styled text
//
// Metal shaders for text rendering with a glyph atlas.
//
// The vertex shader positions textured quads in screen space using an
// orthographic projection. The fragment shader samples the glyph atlas
// and applies the per-vertex color with alpha blending.

#include <metal_stdlib>
using namespace metal;

// =============================================================================
// Vertex Data Structures
// =============================================================================

// Per-vertex data for glyph quads
struct GlyphVertex {
    // Screen position (x, y) - already in pixel coordinates
    float2 position [[attribute(0)]];
    // Texture UV coordinates (u, v)
    float2 uv [[attribute(1)]];
    // Per-vertex RGBA color
    float4 color [[attribute(2)]];
};

// Uniforms passed to the vertex shader
struct Uniforms {
    // Viewport dimensions in pixels
    float2 viewport_size;
};

// Data passed from vertex to fragment shader
struct FragmentInput {
    float4 position [[position]];
    float2 uv;
    float4 color;
};

// =============================================================================
// Vertex Shader
// =============================================================================

// Transforms glyph quad vertices from screen space to Metal's NDC.
// Metal's NDC has origin at center, Y up, range [-1, 1].
// Screen coordinates have origin at top-left, Y down.
vertex FragmentInput glyph_vertex(
    GlyphVertex in [[stage_in]],
    constant Uniforms& uniforms [[buffer(1)]]
) {
    FragmentInput out;

    // Convert from screen coordinates to NDC
    // Screen: (0,0) at top-left, Y down
    // NDC: (0,0) at center, Y up, range [-1, 1]
    float2 ndc;
    ndc.x = (in.position.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    // Flip Y: screen Y increases downward, NDC Y increases upward
    ndc.y = 1.0 - (in.position.y / uniforms.viewport_size.y) * 2.0;

    out.position = float4(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

// =============================================================================
// Fragment Shader
// =============================================================================

// Samples the glyph atlas and outputs the glyph with the per-vertex color.
// The atlas stores glyph coverage in the red channel.
// We multiply by the per-vertex color and use alpha blending.
fragment float4 glyph_fragment(
    FragmentInput in [[stage_in]],
    texture2d<float> atlas [[texture(0)]]
) {
    constexpr sampler atlas_sampler(
        filter::nearest,
        address::clamp_to_edge
    );

    // Sample the glyph alpha from the red channel of the atlas
    float alpha = atlas.sample(atlas_sampler, in.uv).r;

    // Apply per-vertex color with glyph alpha
    return float4(in.color.rgb, in.color.a * alpha);
}
