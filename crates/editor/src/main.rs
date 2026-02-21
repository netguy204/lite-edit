// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
//!
//! lite-edit: A lightweight, GPU-accelerated text editor for macOS
//!
//! This module bootstraps the macOS application with a Metal-backed window.
//! It proves the Rust → Cocoa → Metal pipeline works end-to-end and displays
//! rendered text using a glyph atlas.
//!
//! This version uses a TextBuffer with viewport-based rendering to display
//! a large (100+ line) demo buffer with cursor support.

mod dirty_region;
mod font;
mod glyph_atlas;
mod glyph_buffer;
mod metal_view;
mod renderer;
mod shader;
mod viewport;

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSSize,
};

use lite_edit_buffer::TextBuffer;

use crate::metal_view::MetalView;
use crate::renderer::Renderer;

// =============================================================================
// Demo Text Generation
// =============================================================================

/// Generates a demo buffer with 100+ lines of content to demonstrate
/// viewport-based rendering.
fn generate_demo_content() -> String {
    let mut content = String::new();

    // Header section
    content.push_str("// lite-edit: A lightweight, GPU-accelerated text editor\n");
    content.push_str("// Powered by Rust + Metal on macOS\n");
    content.push_str("// This buffer demonstrates viewport-based rendering with 100+ lines\n");
    content.push_str("\n");

    // Main function
    content.push_str("fn main() {\n");
    content.push_str("    println!(\"Hello, lite-edit!\");\n");
    content.push_str("    \n");
    content.push_str("    // This text is rendered using:\n");
    content.push_str("    // - Core Text for glyph rasterization\n");
    content.push_str("    // - A texture atlas for glyph caching\n");
    content.push_str("    // - Metal shaders for textured quad rendering\n");
    content.push_str("    // - Monospace layout (x = col * width, y = row * height)\n");
    content.push_str("    // - Viewport-based rendering (only visible lines are drawn)\n");
    content.push_str("    \n");
    content.push_str("    let editor = LiteEdit::new();\n");
    content.push_str("    editor.open(\"src/main.rs\");\n");
    content.push_str("    editor.run();\n");
    content.push_str("}\n");
    content.push_str("\n");

    // Character test section
    content.push_str("// The quick brown fox jumps over the lazy dog.\n");
    content.push_str("// ABCDEFGHIJKLMNOPQRSTUVWXYZ\n");
    content.push_str("// abcdefghijklmnopqrstuvwxyz\n");
    content.push_str("// 0123456789 !@#$%^&*()_+-=[]{}|;':\",./<>?\n");
    content.push_str("\n");

    // LiteEdit struct
    content.push_str("struct LiteEdit {\n");
    content.push_str("    buffer: TextBuffer,\n");
    content.push_str("    viewport: Viewport,\n");
    content.push_str("    renderer: Renderer,\n");
    content.push_str("}\n");
    content.push_str("\n");

    // Implementation block
    content.push_str("impl LiteEdit {\n");
    content.push_str("    pub fn new() -> Self {\n");
    content.push_str("        Self {\n");
    content.push_str("            buffer: TextBuffer::new(),\n");
    content.push_str("            viewport: Viewport::new(16.0),\n");
    content.push_str("            renderer: Renderer::new(),\n");
    content.push_str("        }\n");
    content.push_str("    }\n");
    content.push_str("\n");
    content.push_str("    pub fn open(&mut self, path: &str) {\n");
    content.push_str("        // Load file contents into buffer\n");
    content.push_str("        let contents = std::fs::read_to_string(path).unwrap();\n");
    content.push_str("        self.buffer = TextBuffer::from_str(&contents);\n");
    content.push_str("    }\n");
    content.push_str("\n");
    content.push_str("    pub fn run(&mut self) {\n");
    content.push_str("        // Main event loop\n");
    content.push_str("        loop {\n");
    content.push_str("            self.handle_events();\n");
    content.push_str("            self.render();\n");
    content.push_str("        }\n");
    content.push_str("    }\n");
    content.push_str("\n");
    content.push_str("    fn handle_events(&mut self) {\n");
    content.push_str("        // Process keyboard and mouse events\n");
    content.push_str("    }\n");
    content.push_str("\n");
    content.push_str("    fn render(&mut self) {\n");
    content.push_str("        // Render visible portion of buffer\n");
    content.push_str("        self.renderer.render(&self.buffer, &self.viewport);\n");
    content.push_str("    }\n");
    content.push_str("}\n");
    content.push_str("\n");

    // Add numbered lines to reach 100+ total
    for i in 1..=50 {
        content.push_str(&format!("// Line {}: This is a demonstration line for viewport scrolling\n", 70 + i));
    }

    content.push_str("\n");
    content.push_str("// End of demo buffer\n");
    content.push_str("// Total lines: 120+\n");
    content.push_str("// Try scrolling programmatically to see different slices!\n");

    content
}

// =============================================================================
// Application Delegate
// =============================================================================

/// Internal state for our application delegate
struct AppDelegateIvars {
    /// The main window (kept alive by the delegate)
    window: RefCell<Option<Retained<NSWindow>>>,
    /// The Metal renderer
    renderer: RefCell<Option<Renderer>>,
    /// The Metal view
    metal_view: RefCell<Option<Retained<MetalView>>>,
}

impl Default for AppDelegateIvars {
    fn default() -> Self {
        Self {
            window: RefCell::new(None),
            renderer: RefCell::new(None),
            metal_view: RefCell::new(None),
        }
    }
}

define_class!(
    // SAFETY: AppDelegate follows the correct Objective-C memory management rules
    // and is only accessed from the main thread
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    #[name = "LiteEditAppDelegate"]
    struct AppDelegate;

    // SAFETY: NSObjectProtocol is correctly implemented - we inherit from NSObject
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: NSApplicationDelegate protocol methods are implemented correctly
    // with proper signatures matching the Objective-C protocol
    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn application_did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);
            self.setup_window(mtm);
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn application_should_terminate_after_last_window_closed(
            &self,
            _sender: &NSApplication,
        ) -> bool {
            // When the window closes, terminate the app
            true
        }
    }

    // SAFETY: NSWindowDelegate protocol methods are implemented correctly
    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            // Render on resize
            let mut renderer_ref = self.ivars().renderer.borrow_mut();
            let metal_view_ref = self.ivars().metal_view.borrow();

            if let (Some(renderer), Some(metal_view)) = (renderer_ref.as_mut(), metal_view_ref.as_ref()) {
                metal_view.update_drawable_size();
                // Update viewport size based on new window dimensions
                let frame = metal_view.frame();
                let scale = metal_view.scale_factor();
                renderer.update_viewport_size((frame.size.height * scale) as f32);
                renderer.render(metal_view);
            }
        }
    }
);

impl AppDelegate {
    /// Creates a new application delegate
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(AppDelegateIvars::default());
        unsafe { msg_send![super(this), init] }
    }

    /// Sets up the main window with Metal rendering
    fn setup_window(&self, mtm: MainThreadMarker) {
        // Create window with standard editor dimensions
        let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1000.0, 700.0));

        let style_mask = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::Miniaturizable;

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSWindow>(),
                content_rect,
                style_mask,
                NSBackingStoreType::Buffered,
                false,
            )
        };

        // Configure window
        window.setTitle(ns_string!("lite-edit"));
        window.center();

        // Create the Metal-backed view and renderer
        let metal_view = MetalView::new(mtm, content_rect);
        let mut renderer = Renderer::new(&metal_view);

        // Create a TextBuffer with 100+ lines of demo content
        let demo_content = generate_demo_content();
        let buffer = TextBuffer::from_str(&demo_content);
        renderer.set_buffer(buffer);

        // Update viewport size based on window dimensions
        let frame = metal_view.frame();
        let scale = metal_view.scale_factor();
        renderer.update_viewport_size((frame.size.height * scale) as f32);

        // Set the Metal view as the window's content view
        window.setContentView(Some(&metal_view));

        // Set up window delegate (self handles window events)
        let delegate: &ProtocolObject<dyn NSWindowDelegate> = ProtocolObject::from_ref(self);
        window.setDelegate(Some(delegate));

        // Perform initial render
        renderer.render(&metal_view);

        // Store window, renderer, and metal_view in ivars
        *self.ivars().window.borrow_mut() = Some(window.clone());
        *self.ivars().renderer.borrow_mut() = Some(renderer);
        *self.ivars().metal_view.borrow_mut() = Some(metal_view);

        // Make window visible and key
        window.makeKeyAndOrderFront(None);

        // Activate the application (bring to front)
        // activateIgnoringOtherApps is deprecated but required when launching
        // unbundled (i.e., from cargo run / terminal without an app bundle).
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

fn main() {
    // Get main thread marker - panics if not on main thread
    let mtm = MainThreadMarker::new().expect("must be on main thread");

    // Get the shared application instance
    let app = NSApplication::sharedApplication(mtm);

    // Set activation policy to regular (creates Dock icon, menu bar presence)
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    // Create and set our application delegate
    let delegate = AppDelegate::new(mtm);
    let delegate_obj: &ProtocolObject<dyn NSApplicationDelegate> = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(delegate_obj));

    // Run the application event loop
    // This blocks until the application terminates
    app.run();
}
