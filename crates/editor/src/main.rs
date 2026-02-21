// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
//!
//! lite-edit: A lightweight, GPU-accelerated text editor for macOS
//!
//! This module bootstraps the macOS application with a Metal-backed window.
//! It implements the drain-all-then-render main loop pattern from the
//! editor_core_architecture investigation:
//!
//! 1. Drain all pending events, forwarding each to the active focus target
//! 2. Focus target mutates buffer and accumulates dirty regions
//! 3. Render once if dirty
//! 4. Sleep until next event or timer
//!
//! This ensures latency fairness — no event is penalized by intermediate
//! renders of events ahead of it in the batch.

mod buffer_target;
mod clipboard;
mod context;
mod dirty_region;
mod editor_state;
mod focus;
mod font;
mod glyph_atlas;
mod glyph_buffer;
mod input;
mod metal_view;
mod renderer;
mod shader;
mod viewport;

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSRunLoop, NSSize, NSTimer,
};

use lite_edit_buffer::TextBuffer;

use crate::editor_state::EditorState;
use crate::input::KeyEvent;
use crate::metal_view::MetalView;
use crate::renderer::Renderer;

/// Cursor blink interval in seconds
const CURSOR_BLINK_INTERVAL: f64 = 0.5;

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
    content.push_str("// \n");
    content.push_str("// TYPE HERE! This is now an interactive editor.\n");
    content.push_str("// Try: typing, backspace, arrow keys, Enter\n");
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
        content.push_str(&format!(
            "// Line {}: This is a demonstration line for viewport scrolling\n",
            70 + i
        ));
    }

    content.push_str("\n");
    content.push_str("// End of demo buffer\n");
    content.push_str("// Total lines: 120+\n");
    content.push_str("// Try scrolling programmatically to see different slices!\n");

    content
}

// =============================================================================
// Shared Editor Controller
// =============================================================================

/// Shared state that the event handlers can access.
///
/// This wraps the EditorState, Renderer, and MetalView in Rc<RefCell<>> so
/// that the key handler callback and timer callback can both access them.
struct EditorController {
    state: EditorState,
    renderer: Renderer,
    metal_view: Retained<MetalView>,
}

impl EditorController {
    fn new(state: EditorState, renderer: Renderer, metal_view: Retained<MetalView>) -> Self {
        Self {
            state,
            renderer,
            metal_view,
        }
    }

    /// Handles a key event by forwarding to the editor state.
    fn handle_key(&mut self, event: KeyEvent) {
        self.state.handle_key(event);
        self.render_if_dirty();
    }

    /// Toggles cursor blink and re-renders if needed.
    fn toggle_cursor_blink(&mut self) {
        let dirty = self.state.toggle_cursor_blink();
        if dirty.is_dirty() {
            self.state.dirty_region.merge(dirty);
            self.render_if_dirty();
        }
    }

    /// Renders if there's a dirty region.
    fn render_if_dirty(&mut self) {
        if self.state.is_dirty() {
            // Update renderer state from editor state
            self.renderer.set_cursor_visible(self.state.cursor_visible);

            // Update renderer's buffer from editor state
            // Note: We need to sync the buffer - the renderer has a copy
            // For now, we'll create a fresh buffer each time (not ideal, but correct)
            self.sync_renderer_buffer();

            // Take the dirty region and render
            let dirty = self.state.take_dirty_region();
            self.renderer.render_dirty(&self.metal_view, &dirty);
        }
    }

    /// Syncs the renderer's buffer with the editor state's buffer.
    fn sync_renderer_buffer(&mut self) {
        // Update viewport on renderer
        self.renderer.viewport_mut().scroll_offset = self.state.viewport.scroll_offset;

        // Sync buffer content
        // The renderer needs the buffer to render from, so we need to give it
        // an updated view. Since TextBuffer doesn't implement Clone, we'll
        // update in place.
        if let Some(render_buffer) = self.renderer.buffer_mut() {
            // We need to sync the cursor position and content
            // For now, reconstruct the buffer from content
            let content = self.state.buffer.content();
            let cursor_pos = self.state.buffer.cursor_position();

            // Clear and rebuild (not ideal but works for now)
            *render_buffer = TextBuffer::from_str(&content);
            render_buffer.set_cursor(cursor_pos);
        }
    }

    /// Handles window resize.
    fn handle_resize(&mut self) {
        self.metal_view.update_drawable_size();
        let frame = self.metal_view.frame();
        let scale = self.metal_view.scale_factor();
        let height = (frame.size.height * scale) as f32;

        self.state.update_viewport_size(height);
        self.renderer.update_viewport_size(height);

        // Mark full viewport dirty and render
        self.state.mark_full_dirty();
        self.render_if_dirty();
    }
}

// =============================================================================
// Application Delegate
// =============================================================================

/// Internal state for our application delegate
struct AppDelegateIvars {
    /// The main window (kept alive by the delegate)
    window: RefCell<Option<Retained<NSWindow>>>,
    /// The editor controller (shared between callbacks)
    controller: RefCell<Option<Rc<RefCell<EditorController>>>>,
    /// The cursor blink timer
    blink_timer: RefCell<Option<Retained<NSTimer>>>,
}

impl Default for AppDelegateIvars {
    fn default() -> Self {
        Self {
            window: RefCell::new(None),
            controller: RefCell::new(None),
            blink_timer: RefCell::new(None),
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
            let controller_ref = self.ivars().controller.borrow();
            if let Some(controller) = controller_ref.as_ref() {
                controller.borrow_mut().handle_resize();
            }
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, _notification: &NSNotification) {
            // Fires when the window moves between displays with different
            // scale factors (e.g., Retina ↔ non-Retina). The MetalView's
            // viewDidChangeBackingProperties updates the drawable size;
            // we need to re-calculate the viewport and re-render.
            let controller_ref = self.ivars().controller.borrow();
            if let Some(controller) = controller_ref.as_ref() {
                controller.borrow_mut().handle_resize();
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

        // Create the Metal-backed view
        let metal_view = MetalView::new(mtm, content_rect);

        // Attach the view to the window BEFORE creating the renderer.
        // The renderer needs the correct scale factor to rasterize the font
        // and glyph atlas at native resolution (e.g., 2x on Retina).
        window.setContentView(Some(&metal_view));

        // viewDidChangeBackingProperties may not fire synchronously during
        // setContentView. Explicitly sync the scale factor, contentsScale,
        // and drawable size from the window so the renderer sees the correct
        // values when it creates the font and atlas.
        metal_view.sync_backing_properties();

        // Create the renderer
        let mut renderer = Renderer::new(&metal_view);

        // Get font metrics for line height
        let line_height = renderer.viewport().line_height();

        // Create a TextBuffer with demo content
        let demo_content = generate_demo_content();
        let buffer = TextBuffer::from_str(&demo_content);

        // Create the editor state
        let mut state = EditorState::new(buffer, line_height);

        // Update viewport size based on window dimensions
        let frame = metal_view.frame();
        let scale = metal_view.scale_factor();
        let height = (frame.size.height * scale) as f32;
        state.update_viewport_size(height);
        renderer.update_viewport_size(height);

        // Set the initial buffer in the renderer
        let initial_buffer = TextBuffer::from_str(&demo_content);
        renderer.set_buffer(initial_buffer);

        // Create the shared controller
        let controller = Rc::new(RefCell::new(EditorController::new(
            state,
            renderer,
            metal_view.clone(),
        )));

        // Set up key handler
        let key_controller = controller.clone();
        metal_view.set_key_handler(move |event| {
            key_controller.borrow_mut().handle_key(event);
        });

        // Make the view first responder to receive key events
        window.makeFirstResponder(Some(&metal_view));

        // Set up window delegate (self handles window events)
        let delegate: &ProtocolObject<dyn NSWindowDelegate> = ProtocolObject::from_ref(self);
        window.setDelegate(Some(delegate));

        // Perform initial render
        {
            let mut ctrl = controller.borrow_mut();
            ctrl.state.mark_full_dirty();
            ctrl.render_if_dirty();
        }

        // Set up cursor blink timer
        let timer_controller = controller.clone();
        let blink_timer = self.setup_cursor_blink_timer(mtm, timer_controller);

        // Store state in ivars
        *self.ivars().window.borrow_mut() = Some(window.clone());
        *self.ivars().controller.borrow_mut() = Some(controller);
        *self.ivars().blink_timer.borrow_mut() = Some(blink_timer);

        // Make window visible and key
        window.makeKeyAndOrderFront(None);

        // Activate the application (bring to front)
        // activateIgnoringOtherApps is deprecated but required when launching
        // unbundled (i.e., from cargo run / terminal without an app bundle).
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }

    /// Sets up the cursor blink timer
    fn setup_cursor_blink_timer(
        &self,
        _mtm: MainThreadMarker,
        controller: Rc<RefCell<EditorController>>,
    ) -> Retained<NSTimer> {
        // Create a block for the timer callback
        let block = RcBlock::new(move |_timer: NonNull<NSTimer>| {
            controller.borrow_mut().toggle_cursor_blink();
        });

        // Create and schedule the timer
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(
                CURSOR_BLINK_INTERVAL,
                true,
                &block,
            )
        };

        // Add to common run loop modes so it fires during tracking (resize/drag)
        let run_loop = NSRunLoop::currentRunLoop();
        unsafe {
            run_loop.addTimer_forMode(&timer, objc2_foundation::NSRunLoopCommonModes);
        }

        timer
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
    let delegate_obj: &ProtocolObject<dyn NSApplicationDelegate> =
        ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(delegate_obj));

    // Run the application event loop
    // This blocks until the application terminates
    //
    // Note: macOS's NSRunLoop handles the drain-all-then-render pattern naturally:
    // - Events arrive and are dispatched to handlers
    // - Our key handler forwards to EditorState and triggers render
    // - The run loop sleeps until the next event or timer
    //
    // For explicit batching (if needed in the future), we could install a
    // CFRunLoopObserver for kCFRunLoopBeforeWaiting to render after all
    // events in a batch are processed.
    app.run();
}
