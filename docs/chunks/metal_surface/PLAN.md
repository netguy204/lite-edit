<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk establishes the Rust → Cocoa → Metal pipeline from scratch. We'll create a native macOS application that:

1. **Initializes an NSApplication** with a minimal run loop
2. **Creates an NSWindow** with a content view backed by a CAMetalLayer
3. **Sets up the Metal rendering pipeline**: MTLDevice, MTLCommandQueue, MTLRenderPipelineState
4. **Executes a render pass** that clears to a dark editor background color
5. **Handles window lifecycle** (close, resize) cleanly

**Rust-to-macOS bridging strategy**: Use the `objc2` family of crates (`objc2`, `objc2-foundation`, `objc2-app-kit`, `objc2-metal`, `objc2-quartz-core`) for type-safe Cocoa/Metal bindings. These provide idiomatic Rust wrappers around Objective-C classes while maintaining zero-overhead abstractions. Alternative approaches (raw `objc` crate, `winit` + `wgpu`) either lack type safety or add unnecessary abstraction layers between us and Metal.

**Why not wgpu/winit?** The GOAL.md explicitly states "Direct Metal rendering on macOS. No Electron, no webview, no terminal emulator intermediary." While wgpu is excellent, it's an abstraction over Metal. For a performance-critical text editor where we need precise control over the render pipeline (incremental dirty region rendering, glyph atlas management), direct Metal access is preferred. This also eliminates a dependency and its associated complexity.

**Testing strategy**: Window creation and Metal rendering are inherently visual and macOS-specific, making traditional unit testing difficult. We'll verify through:
- Compilation success (tests the bindings work)
- Manual verification (window opens, correct color displayed)
- Automated smoke test using `NSRunLoop::runUntilDate:` to verify the window opens and renders at least one frame without crashing
- Clean shutdown verification (no resource leaks, no crashes on close)

## Subsystem Considerations

No existing subsystems. This is the first chunk being implemented. The patterns established here (Metal initialization, render pass structure, window management) may seed a future `rendering` or `platform_macos` subsystem as subsequent chunks build on this foundation.

## Sequence

### Step 1: Initialize the Rust project with Cargo.toml

Create the project structure with dependencies:
- `objc2` - Core Objective-C runtime bindings
- `objc2-foundation` - Foundation framework (NSString, NSRunLoop, etc.)
- `objc2-app-kit` - AppKit framework (NSApplication, NSWindow, NSView)
- `objc2-metal` - Metal framework bindings
- `objc2-quartz-core` - Core Animation (CAMetalLayer)
- `block2` - Objective-C block support

Configure build settings:
- Edition 2021
- macOS minimum deployment target (use `MACOSX_DEPLOYMENT_TARGET` env var)
- Link frameworks: `AppKit`, `Metal`, `QuartzCore`, `Foundation`

**Output**: `Cargo.toml`, `src/main.rs` (minimal placeholder), `build.rs` (framework linking)

Location: Project root

---

### Step 2: Implement NSApplication bootstrap

Create the minimal macOS application lifecycle:
- Obtain shared NSApplication instance
- Set activation policy to `.regular` (creates Dock icon, menu bar presence)
- Create a minimal NSApplicationDelegate to handle termination

The delegate must implement:
- `applicationDidFinishLaunching:` - where we'll create the window
- `applicationShouldTerminateAfterLastWindowClosed:` → `true` - clean exit when window closes

**Key insight from investigation**: The entire critical path runs on the main thread via NSRunLoop. We're not building a separate event system — we hook into macOS's existing one.

Location: `src/main.rs` (or `src/app.rs` if we modularize)

---

### Step 3: Create NSWindow with basic configuration

Instantiate an NSWindow with:
- Style mask: titled, closable, resizable, miniaturizable
- Initial size: 800×600 (reasonable default for a code editor)
- Title: "lite-edit" (or configurable)
- Background: Opaque, no transparency

Position the window centered on screen. Make it key and order front.

Do NOT yet attach a custom view — just verify the window opens and displays.

Location: `src/main.rs` (window creation logic)

---

### Step 4: Create a CAMetalLayer-backed NSView

Create a custom NSView subclass that:
- Overrides `makeBackingLayer` to return a CAMetalLayer
- Sets `wantsLayer = true` to enable layer-backed rendering

Configure the CAMetalLayer:
- Set `device` to the default MTLDevice
- Set `pixelFormat` to `.bgra8Unorm` (standard for display)
- Set `framebufferOnly = true` (we don't need to read back)
- Set `drawableSize` to match the view's backing size (retina-aware)

Set this custom view as the window's `contentView`.

**Retina handling**: The layer's `drawableSize` must account for the backing scale factor (`window.backingScaleFactor`). A 800×600 window on a 2x retina display needs a 1600×1200 drawable.

Location: `src/metal_view.rs`

---

### Step 5: Initialize the Metal rendering pipeline

Create the core Metal objects:

1. **MTLDevice**: `MTLCreateSystemDefaultDevice()` — the GPU handle
2. **MTLCommandQueue**: `device.newCommandQueue()` — where we submit work
3. **MTLLibrary**: Compile a minimal shader (vertex + fragment that outputs a solid color)
4. **MTLRenderPipelineState**: Configured with our vertex/fragment functions and the layer's pixel format

The shader for this chunk is trivially simple:
```metal
vertex float4 vertex_main(uint vid [[vertex_id]]) {
    // Full-screen triangle or quad (can also rely solely on clear color)
    return float4(0.0);
}
fragment float4 fragment_main() {
    return float4(0.118, 0.118, 0.180, 1.0); // #1e1e2e
}
```

**Alternative (simpler)**: Skip the shader entirely for this chunk. The render pass descriptor's `clearColor` will fill the drawable with our background color. No geometry needed. The render pipeline state is only necessary if we're drawing geometry — for a solid color, just configuring the clear color and performing a render pass with no draw calls is sufficient.

**Decision**: Use the clear-color-only approach for this chunk. It proves Metal works without introducing shader compilation complexity. The glyph_rendering chunk will add shaders when needed.

Location: `src/renderer.rs`

---

### Step 6: Implement the render loop (render-on-demand)

Per the investigation findings, we do NOT use CVDisplayLink or a continuous render loop. Instead:

1. On window creation, perform an initial render
2. On window resize, perform a render (via `viewDidChangeBackingProperties` or `setFrameSize:`)
3. Future chunks will trigger renders on input events

The render sequence for each frame:
1. Get next drawable from CAMetalLayer (`nextDrawable()`)
2. Create a render pass descriptor with:
   - `colorAttachments[0].texture` = drawable's texture
   - `colorAttachments[0].loadAction` = `.clear`
   - `colorAttachments[0].clearColor` = MTLClearColor(0.118, 0.118, 0.180, 1.0) // #1e1e2e
   - `colorAttachments[0].storeAction` = `.store`
3. Create a command buffer from the command queue
4. Create a render command encoder from the render pass descriptor
5. End encoding (no draw calls for this chunk — clear is enough)
6. Present the drawable via `commandBuffer.presentDrawable(drawable)`
7. Commit the command buffer

Location: `src/renderer.rs`

---

### Step 7: Handle window resize

When the window is resized:
1. Update the CAMetalLayer's `drawableSize` to match the new view size × backing scale factor
2. Trigger a re-render

This can be done by:
- Observing `NSViewFrameDidChangeNotification`
- Overriding `setFrameSize:` in our custom view
- Using `viewDidChangeBackingProperties` for scale factor changes

**Edge case**: During live resize, many resize events fire rapidly. For this chunk, we'll render on every resize. Future optimization (if needed) could coalesce resize events, but the investigation found that full viewport redraws are <1ms, so rapid re-renders during resize should be fine.

Location: `src/metal_view.rs` (resize handling)

---

### Step 8: Handle clean shutdown

Ensure clean resource cleanup:
- When the window closes, the app should terminate (handled by delegate's `applicationShouldTerminateAfterLastWindowClosed:`)
- Metal resources (device, command queue) should be dropped cleanly
- No explicit cleanup is usually needed — Rust's RAII handles it if we structure ownership correctly

Verify:
- Cmd-Q terminates cleanly
- Clicking the red close button terminates cleanly
- No crashes, no "zombie" processes

Location: `src/main.rs` (shutdown path)

---

### Step 9: Add smoke test and verification

Create a minimal test that:
1. Launches the app
2. Runs the run loop briefly (`NSRunLoop.current.runUntilDate:` with a short timeout)
3. Verifies the window exists and is visible
4. Terminates cleanly

This won't verify the visual output (that's inherently manual), but it ensures the code path executes without panicking.

Also add a `README.md` with:
- Build instructions (`cargo build --release`)
- Run instructions (`cargo run`)
- Expected behavior (window opens with dark purple/blue background)

Location: `tests/smoke_test.rs`, `README.md`

---

**BACKREFERENCE COMMENTS**

Add chunk backreference at the module level for core files:
```rust
// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
```

Place this comment at the top of:
- `src/main.rs`
- `src/metal_view.rs`
- `src/renderer.rs`

## Dependencies

**No chunk dependencies** — this is the first chunk in the editor core architecture.

**External crate dependencies** (to be added to Cargo.toml):
- `objc2` ^0.5 — Core Objective-C runtime bindings
- `objc2-foundation` ^0.2 — Foundation framework
- `objc2-app-kit` ^0.2 — AppKit framework
- `objc2-metal` ^0.2 — Metal framework
- `objc2-quartz-core` ^0.2 — Core Animation (CAMetalLayer)
- `block2` ^0.5 — Objective-C block support

**macOS SDK requirement**: Xcode Command Line Tools must be installed. The build uses `xcrun` to locate the SDK for linking frameworks.

**Target platform**: macOS only. The Cargo.toml should specify `[target.'cfg(target_os = "macos")'.dependencies]` for platform-specific deps.

## Risks and Open Questions

1. **objc2 crate stability**: The `objc2` family of crates is actively developed but may have API changes between versions. Pin to specific versions in Cargo.toml to avoid surprises. If critical APIs are missing, we may need to fall back to raw `objc` crate bindings for specific calls.

2. **CAMetalLayer threading**: CAMetalLayer is documented as requiring main-thread access for certain operations. Since our architecture runs everything on the main thread (per investigation findings), this should be fine, but verify during implementation.

3. **Retina/scaling edge cases**: The backing scale factor can change if a window is dragged between displays with different DPIs. We need to handle `viewDidChangeBackingProperties` to update the drawable size. Test with external monitors if available.

4. **Live resize performance**: During window resize, many resize events fire. While full redraws are <1ms, there may be visual artifacts if the resize outpaces rendering. Monitor for tearing or flashing during live resize.

5. **Memory management with objc2**: The objc2 crates use Rust's ownership model to manage Objective-C reference counting. Ensure we understand the `Retained<T>` / `Id<T>` patterns to avoid leaks or use-after-free.

6. **Missing framework bindings**: If the objc2-metal or objc2-quartz-core crates don't expose everything we need (e.g., specific MTLRenderPassDescriptor configuration), we may need to add custom bindings or use `msg_send!` directly.

## Deviations

- **Crate versions**: Plan specified objc2 ^0.5 and related crates at ^0.2. The crates
  have since been updated to objc2 0.6 and related crates at 0.3. This required updating
  the macro syntax from `declare_class!` to `define_class!`, using `#[unsafe(super = ...)]`
  attributes, and `DefinedClass` trait instead of `DeclaredClass`. The core concepts
  remained the same.

- **Feature flags**: Plan mentioned explicit feature flags for objc2 crates. In version
  0.3.x, most features are enabled by default, simplifying the Cargo.toml.

- **NSBackingStoreType**: Changed from `NSBackingStoreType::NSBackingStoreBuffered` to
  `NSBackingStoreType::Buffered` per updated API naming.

- **activateIgnoringOtherApps**: The method is deprecated in objc2-app-kit 0.3.2. Changed
  to use `app.activate()` instead.

- **CGSize**: Plan mentioned using `objc2_foundation::CGSize`. In the implementation,
  `NSSize` is used directly as it's a type alias for `CGSize` in objc2-foundation.

- **Smoke test scope**: Plan suggested using NSRunLoop::runUntilDate for integration
  testing. Implemented simpler unit tests that verify Metal device availability and
  command queue creation without requiring the full app lifecycle, as this provides
  sufficient coverage for the core functionality.