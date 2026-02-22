#![allow(dead_code)]
// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/glyph_rendering - Monospace glyph atlas + text rendering
// Chunk: docs/chunks/viewport_rendering - Viewport + Buffer-to-Screen Rendering
// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/viewport_scrolling - Scroll event handling
// Chunk: docs/chunks/quit_command - Cmd+Q app termination handling
// Chunk: docs/chunks/file_picker - File picker (Cmd+P) integration
// Chunk: docs/chunks/file_save - File-buffer association and Cmd+S save
// Chunk: docs/chunks/workspace_model - Workspace model and left rail rendering
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
// Chunk: docs/chunks/renderer_styled_content - ColorPalette for styled text
mod color_palette;
mod context;
mod dirty_region;
mod editor_state;
pub mod file_index;
mod focus;
mod font;
mod glyph_atlas;
mod glyph_buffer;
mod input;
mod left_rail;
mod metal_view;
// Chunk: docs/chunks/mini_buffer_model - MiniBuffer single-line editing model
mod mini_buffer;
mod renderer;
// Chunk: docs/chunks/row_scroller_extract - Reusable scroll arithmetic
mod row_scroller;
mod tab_bar;
mod selector;
mod selector_overlay;
mod shader;
mod viewport;
mod workspace;
mod wrap_layout;

pub use file_index::FileIndex;
pub use row_scroller::RowScroller;

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
    NSRunLoop, NSSize, NSString, NSTimer,
};

use lite_edit_buffer::TextBuffer;

use crate::editor_state::{EditorFocus, EditorState};
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};
// Chunk: docs/chunks/cursor_pointer_ui_hints - Cursor region types for dynamic cursor display
use crate::left_rail::RAIL_WIDTH;
use crate::metal_view::{CursorRect, CursorRegions, MetalView};
use crate::renderer::Renderer;
use crate::selector_overlay::calculate_overlay_geometry;
use crate::tab_bar::TAB_BAR_HEIGHT;

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
    /// Last window title that was set, to avoid redundant updates
    last_window_title: String,
}

impl EditorController {
    fn new(state: EditorState, renderer: Renderer, metal_view: Retained<MetalView>) -> Self {
        Self {
            state,
            renderer,
            metal_view,
            last_window_title: String::new(),
        }
    }

    /// Handles a key event by forwarding to the editor state.
    ///
    /// After processing the event, checks if the app should quit (Cmd+Q)
    /// and terminates the application if so.
    // Chunk: docs/chunks/quit_command - Checks quit flag and triggers app termination
    fn handle_key(&mut self, event: KeyEvent) {
        self.state.handle_key(event);

        // Check for quit request
        if self.state.should_quit {
            self.terminate_app();
            return;
        }

        // Chunk: docs/chunks/terminal_input_render_bug - Poll immediately after input
        // For terminal tabs, poll PTY output immediately after sending input
        // to ensure echoed characters appear without waiting for the next timer tick.
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        // Poll for file index updates so picker results stream in on every keystroke
        // Chunk: docs/chunks/picker_eager_index
        let picker_dirty = self.state.tick_picker();
        if picker_dirty.is_dirty() {
            self.state.dirty_region.merge(picker_dirty);
        }

        self.render_if_dirty();
    }

    /// Handles a mouse event by forwarding to the editor state.
    fn handle_mouse(&mut self, event: MouseEvent) {
        self.state.handle_mouse(event);

        // Chunk: docs/chunks/terminal_input_render_bug - Poll immediately after input
        // For terminal tabs, poll PTY output immediately after mouse input.
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        // Poll for file index updates so picker results stream in on mouse interaction
        // Chunk: docs/chunks/picker_eager_index
        let picker_dirty = self.state.tick_picker();
        if picker_dirty.is_dirty() {
            self.state.dirty_region.merge(picker_dirty);
        }

        self.render_if_dirty();
    }

    // Chunk: docs/chunks/viewport_scrolling - Controller scroll event forwarding
    /// Handles a scroll event by forwarding to the editor state.
    ///
    /// Scroll events only affect the viewport position, not the cursor.
    fn handle_scroll(&mut self, delta: ScrollDelta) {
        self.state.handle_scroll(delta);

        // Chunk: docs/chunks/terminal_input_render_bug - Poll immediately after input
        // For terminal tabs, poll PTY output immediately after scroll input.
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        // Poll for file index updates so picker results stream in on scroll
        // Chunk: docs/chunks/picker_eager_index
        let picker_dirty = self.state.tick_picker();
        if picker_dirty.is_dirty() {
            self.state.dirty_region.merge(picker_dirty);
        }

        self.render_if_dirty();
    }

    /// Terminates the macOS application.
    ///
    /// This is called when the user presses Cmd+Q. It obtains a MainThreadMarker
    /// (which is safe since EditorController only runs on the main thread) and
    /// calls NSApplication::terminate to perform a clean shutdown.
    // Chunk: docs/chunks/quit_command - Calls NSApplication::terminate for clean macOS shutdown
    fn terminate_app(&self) {
        // SAFETY: EditorController is only accessed from the main thread
        // (via callbacks from the NSRunLoop), so MainThreadMarker::new() will succeed.
        let mtm = MainThreadMarker::new().expect("EditorController must run on main thread");
        let app = NSApplication::sharedApplication(mtm);
        // Passing None as sender is equivalent to the user quitting from the menu
        app.terminate(None);
    }

    /// Toggles cursor blink, polls PTY events, checks for picker updates, and re-renders if needed.
    /// Chunk: docs/chunks/file_picker - Integration of tick_picker into timer-driven refresh loop
    fn toggle_cursor_blink(&mut self) {
        // Toggle cursor blink
        let cursor_dirty = self.state.toggle_cursor_blink();
        if cursor_dirty.is_dirty() {
            self.state.dirty_region.merge(cursor_dirty);
        }

        // Chunk: docs/chunks/terminal_input_render_bug - Poll PTY events
        // Poll all agent and standalone terminal PTY events.
        // This processes shell output and updates TerminalBuffer content.
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        // Check for picker streaming updates
        let picker_dirty = self.state.tick_picker();
        if picker_dirty.is_dirty() {
            self.state.dirty_region.merge(picker_dirty);
        }

        // Render if anything is dirty
        self.render_if_dirty();
    }

    /// Renders if there's a dirty region.
    /// Chunk: docs/chunks/file_picker - Conditional render_with_selector when focus is Selector
    /// Chunk: docs/chunks/cursor_blink_focus - Focus-aware cursor visibility for buffer vs overlay
    fn render_if_dirty(&mut self) {
        // Update window title if needed (even if not rendering)
        self.update_window_title_if_needed();

        if self.state.is_dirty() {
            // Chunk: docs/chunks/renderer_polymorphic_buffer - Sync viewport scroll offset
            // The renderer's viewport needs the scroll offset from the editor state.
            // We sync this here rather than in render_with_editor because the EditorState
            // owns the authoritative scroll position.
            if let Some(buffer_view) = self.state.editor.active_buffer_view() {
                let line_count = buffer_view.line_count();
                let state_scroll_px = self.state.viewport().scroll_offset_px();
                self.renderer.viewport_mut().set_scroll_offset_px(state_scroll_px, line_count);
            }

            // Take the dirty region
            let _dirty = self.state.take_dirty_region();

            // Chunk: docs/chunks/workspace_model - Render with left rail
            // Chunk: docs/chunks/find_in_file - Render with find strip when active
            // Chunk: docs/chunks/cursor_blink_focus - Pass appropriate cursor visibility per focus
            match self.state.focus {
                EditorFocus::Selector => {
                    // When selector is focused, main buffer cursor stays static (visible),
                    // overlay cursor blinks via overlay_cursor_visible
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    // Render with selector overlay, passing overlay cursor visibility
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        self.state.active_selector.as_ref(),
                        self.state.overlay_cursor_visible,
                    );
                }
                EditorFocus::FindInFile => {
                    // When find strip is focused, main buffer cursor stays static (visible),
                    // find strip cursor blinks via overlay_cursor_visible
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    // Render with find strip at bottom, passing overlay cursor visibility
                    if let Some(ref mini_buffer) = self.state.find_mini_buffer {
                        self.renderer.render_with_find_strip(
                            &self.metal_view,
                            &self.state.editor,
                            &mini_buffer.content(),
                            mini_buffer.cursor_col(),
                            self.state.overlay_cursor_visible,
                        );
                    }
                }
                EditorFocus::Buffer => {
                    // Normal buffer focus - main cursor blinks
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    // Normal rendering with left rail (no overlay)
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        None,
                        self.state.cursor_visible,
                    );
                }
            }

            // Chunk: docs/chunks/cursor_pointer_ui_hints - Update cursor regions after rendering
            // Update cursor regions to reflect current UI layout. This ensures the
            // correct cursor (pointer vs I-beam) appears when hovering over different
            // UI elements like the left rail, tab bar, or buffer content area.
            self.update_cursor_regions();
        }
    }

    /// Updates the window title if it has changed.
    ///
    /// Compares the current `state.window_title()` with `last_window_title`.
    /// If different, updates the NSWindow title and caches the new value.
    fn update_window_title_if_needed(&mut self) {
        let current_title = self.state.window_title();
        if current_title != self.last_window_title {
            // Get window from metal_view and update title
            if let Some(window) = self.metal_view.window() {
                window.setTitle(&NSString::from_str(&current_title));
            }
            self.last_window_title = current_title;
        }
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Removed sync_renderer_buffer
    // The renderer no longer owns a buffer copy, so buffer content sync is eliminated.
    // Viewport scroll sync is now done inline in render_if_dirty.

    // Chunk: docs/chunks/cursor_pointer_ui_hints - Calculate cursor regions for UI
    /// Calculates and sets the cursor regions for the current UI state.
    ///
    /// This method determines which areas of the view should display pointer
    /// (arrow) cursor vs I-beam (text) cursor based on the current layout:
    ///
    /// - Left rail (workspace tiles): Pointer cursor
    /// - Tab bar (content tabs): Pointer cursor
    /// - Buffer content area: I-beam cursor
    /// - Selector overlay (when active): Pointer for items, I-beam for query input
    ///
    /// Coordinates are converted from pixel space (top-left origin) to point
    /// space (bottom-left origin) as required by NSView's addCursorRect.
    fn update_cursor_regions(&self) {
        let frame = self.metal_view.frame();
        let scale = self.metal_view.scale_factor();

        // View dimensions in pixels
        let view_width_px = (frame.size.width * scale) as f32;
        let view_height_px = (frame.size.height * scale) as f32;

        // View dimensions in points (for NSView coordinate system)
        let view_width_pt = frame.size.width;
        let view_height_pt = frame.size.height;

        let mut regions = CursorRegions::new();

        // Helper to convert from pixel coordinates (top-left origin, y-down)
        // to point coordinates (bottom-left origin, y-up) for NSView
        let px_to_pt = |y_px: f32, height_px: f32| -> f64 {
            // Convert y from top-down to bottom-up
            // top-down: y_px is distance from top
            // bottom-up: y = view_height - (y_px + height_px)
            view_height_pt - ((y_px + height_px) as f64 / scale)
        };

        // ==== Left Rail (Pointer Cursor) ====
        // The left rail is always visible on the left edge
        // Geometry: x=0, y=0 (top-down), width=RAIL_WIDTH, height=view_height
        {
            let rail_width_pt = RAIL_WIDTH as f64 / scale;
            // In NSView coords: bottom-left origin, so y=0 is at bottom
            regions.add_pointer(CursorRect::new(
                0.0,
                0.0,
                rail_width_pt,
                view_height_pt,
            ));
        }

        // ==== Tab Bar (Pointer Cursor) ====
        // Tab bar is at the top of the content area (right of left rail)
        // Geometry: x=RAIL_WIDTH, y=0 (top-down), width=view_width-RAIL_WIDTH, height=TAB_BAR_HEIGHT
        if let Some(workspace) = self.state.editor.active_workspace() {
            if !workspace.tabs.is_empty() {
                let tab_bar_x_pt = RAIL_WIDTH as f64 / scale;
                let tab_bar_width_pt = view_width_pt - tab_bar_x_pt;
                let tab_bar_height_pt = TAB_BAR_HEIGHT as f64 / scale;
                // Tab bar is at y=0 in top-down, so in NSView coords it's at view_height - tab_bar_height
                let tab_bar_y_pt = view_height_pt - tab_bar_height_pt;

                regions.add_pointer(CursorRect::new(
                    tab_bar_x_pt,
                    tab_bar_y_pt,
                    tab_bar_width_pt,
                    tab_bar_height_pt,
                ));
            }
        }

        // ==== Selector Overlay (Pointer Cursor for items, I-beam for query) ====
        // When selector is active, it overlays the content area
        if let EditorFocus::Selector = self.state.focus {
            if let Some(ref selector) = self.state.active_selector {
                let line_height = self.state.font_metrics().line_height as f32;
                let geometry = calculate_overlay_geometry(
                    view_width_px,
                    view_height_px,
                    line_height,
                    selector.items().len(),
                );

                // The selector panel takes pointer cursor for the entire panel
                // (clicking anywhere dismisses or selects)
                let panel_x_pt = geometry.panel_x as f64 / scale;
                let panel_y_pt = px_to_pt(geometry.panel_y, geometry.panel_height);
                let panel_width_pt = geometry.panel_width as f64 / scale;
                let panel_height_pt = geometry.panel_height as f64 / scale;

                regions.add_pointer(CursorRect::new(
                    panel_x_pt,
                    panel_y_pt,
                    panel_width_pt,
                    panel_height_pt,
                ));

                // The query input area within the panel gets I-beam cursor
                // Query is at the top of the panel (after padding)
                let query_x_pt = panel_x_pt;
                let query_y_pt = px_to_pt(geometry.query_row_y, line_height);
                let query_width_pt = panel_width_pt;
                let query_height_pt = line_height as f64 / scale;

                regions.add_ibeam(CursorRect::new(
                    query_x_pt,
                    query_y_pt,
                    query_width_pt,
                    query_height_pt,
                ));
            }
        }

        // ==== Buffer Content Area (I-beam Cursor) ====
        // The buffer content area is to the right of the left rail and below the tab bar
        {
            let content_x_pt = RAIL_WIDTH as f64 / scale;
            let content_width_pt = view_width_pt - content_x_pt;

            // Y starts below the tab bar (if present)
            let tab_bar_height_pt = if self.state.editor.active_workspace().map_or(false, |ws| !ws.tabs.is_empty()) {
                TAB_BAR_HEIGHT as f64 / scale
            } else {
                0.0
            };

            let content_height_pt = view_height_pt - tab_bar_height_pt;
            // In NSView coords, content starts at y=0 (bottom)
            let content_y_pt = 0.0;

            regions.add_ibeam(CursorRect::new(
                content_x_pt,
                content_y_pt,
                content_width_pt,
                content_height_pt,
            ));
        }

        // Set the cursor regions on the view
        self.metal_view.set_cursor_regions(regions);
    }

    /// Handles window resize.
    fn handle_resize(&mut self) {
        self.metal_view.update_drawable_size();
        let frame = self.metal_view.frame();
        let scale = self.metal_view.scale_factor();
        let width = (frame.size.width * scale) as f32;
        let height = (frame.size.height * scale) as f32;

        self.state.update_viewport_dimensions(width, height);
        self.renderer.update_viewport_size(width, height);

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

        // Get font metrics from the renderer
        let font_metrics = renderer.font_metrics();

        // Create a TextBuffer with demo content
        let demo_content = generate_demo_content();
        let buffer = TextBuffer::from_str(&demo_content);

        // Create the editor state with font metrics
        let mut state = EditorState::new(buffer, font_metrics);

        // Update viewport size based on window dimensions
        let frame = metal_view.frame();
        let scale = metal_view.scale_factor();
        let width = (frame.size.width * scale) as f32;
        let height = (frame.size.height * scale) as f32;
        state.update_viewport_dimensions(width, height);
        renderer.update_viewport_size(width, height);

        // Chunk: docs/chunks/renderer_polymorphic_buffer - No longer setting buffer on renderer
        // The renderer reads from Editor.active_buffer_view() at render time instead of
        // owning a buffer copy. The demo content is already in EditorState.

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

        // Set up mouse handler
        let mouse_controller = controller.clone();
        metal_view.set_mouse_handler(move |event| {
            mouse_controller.borrow_mut().handle_mouse(event);
        });

        // Set up scroll handler
        let scroll_controller = controller.clone();
        metal_view.set_scroll_handler(move |delta| {
            scroll_controller.borrow_mut().handle_scroll(delta);
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
