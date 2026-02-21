// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
//!
//! lite-edit: A lightweight, GPU-accelerated text editor for macOS
//!
//! This module bootstraps the macOS application with a Metal-backed window.
//! It proves the Rust → Cocoa → Metal pipeline works end-to-end and displays
//! rendered text using a glyph atlas.

mod font;
mod glyph_atlas;
mod glyph_buffer;
mod metal_view;
mod renderer;
mod shader;

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

use crate::metal_view::MetalView;
use crate::renderer::Renderer;

// =============================================================================
// Demo Text
// =============================================================================

/// Demo text to display - showing the text rendering pipeline works
const DEMO_TEXT: &[&str] = &[
    "// lite-edit: A lightweight, GPU-accelerated text editor",
    "// Powered by Rust + Metal on macOS",
    "",
    "fn main() {",
    "    println!(\"Hello, lite-edit!\");",
    "    ",
    "    // This text is rendered using:",
    "    // - Core Text for glyph rasterization",
    "    // - A texture atlas for glyph caching",
    "    // - Metal shaders for textured quad rendering",
    "    // - Monospace layout (x = col * width, y = row * height)",
    "    ",
    "    let editor = LiteEdit::new();",
    "    editor.open(\"src/main.rs\");",
    "    editor.run();",
    "}",
    "",
    "// The quick brown fox jumps over the lazy dog.",
    "// ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "// abcdefghijklmnopqrstuvwxyz",
    "// 0123456789 !@#$%^&*()_+-=[]{}|;':\",./<>?",
    "",
    "struct LiteEdit {",
    "    buffer: TextBuffer,",
    "    viewport: Viewport,",
    "    renderer: Renderer,",
    "}",
];

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
            let renderer_ref = self.ivars().renderer.borrow();
            let metal_view_ref = self.ivars().metal_view.borrow();

            if let (Some(renderer), Some(metal_view)) = (renderer_ref.as_ref(), metal_view_ref.as_ref()) {
                metal_view.update_drawable_size();
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

        // Set the demo text content
        renderer.set_content(DEMO_TEXT);

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
        let app = NSApplication::sharedApplication(mtm);
        app.activate();
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
