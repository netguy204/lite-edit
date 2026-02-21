---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - Cargo.toml
  - crates/editor/Cargo.toml
  - crates/editor/build.rs
  - crates/editor/src/main.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/renderer.rs
  - crates/editor/tests/smoke_test.rs
  - README.md
code_references:
  - ref: crates/editor/src/main.rs#AppDelegate
    implements: "Application delegate handling lifecycle and window setup"
  - ref: crates/editor/src/main.rs#AppDelegate::setup_window
    implements: "Window creation with Metal view and renderer initialization"
  - ref: crates/editor/src/main.rs#main
    implements: "Application bootstrap - NSApplication initialization and run loop"
  - ref: crates/editor/src/metal_view.rs#MetalView
    implements: "CAMetalLayer-backed NSView for Metal rendering"
  - ref: crates/editor/src/metal_view.rs#MetalView::update_drawable_size_internal
    implements: "Retina-aware drawable sizing"
  - ref: crates/editor/src/metal_view.rs#get_default_metal_device
    implements: "MTLDevice acquisition"
  - ref: crates/editor/src/renderer.rs#Renderer
    implements: "Metal rendering pipeline with command queue"
  - ref: crates/editor/src/renderer.rs#Renderer::render
    implements: "Frame rendering with clear-to-background-color render pass"
  - ref: crates/editor/src/renderer.rs#BACKGROUND_COLOR
    implements: "Editor background color constant (#1e1e2e)"
  - ref: crates/editor/build.rs#main
    implements: "Framework linking for AppKit, Metal, QuartzCore, Foundation"
narrative: null
investigation: editor_core_architecture
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after: []
---

# macOS Window + Metal Surface

## Minor Goal

Establish the foundational rendering infrastructure for lite-edit: a native macOS window with a GPU-accelerated Metal surface. This is the first step on the critical path described in GOAL.md — "Direct Metal rendering on macOS. No Electron, no webview, no terminal emulator intermediary."

This chunk proves the Rust → Cocoa → Metal pipeline works end-to-end: creating an NSWindow, attaching a CAMetalLayer-backed NSView, obtaining a Metal device and command queue, executing a render pass that clears to a background color, and presenting the result. Every subsequent visual chunk builds on this foundation.

No event handling beyond window close. No text rendering. No input processing. Just a colored rectangle on screen, rendered via Metal from Rust.

## Success Criteria

- A Rust binary (`cargo run`) opens a native macOS window with a title bar.
- The window's content area is rendered via Metal (CAMetalLayer), not CPU compositing.
- The window displays a solid background color (dark editor-style color, e.g., `#1e1e2e`).
- The Metal pipeline is fully initialized: MTLDevice, MTLCommandQueue, MTLRenderPipelineState, and a render pass descriptor that clears to the background color.
- Window close (clicking the red button or Cmd-Q) terminates the process cleanly without crashes or resource leaks.
- The window is resizable, and resizing causes the Metal layer to resize and re-render correctly (no artifacts, no stale content).
- The project builds with `cargo build --release` on macOS with no non-Rust build steps required beyond what Cargo provides (build.rs is fine, external Makefiles are not).