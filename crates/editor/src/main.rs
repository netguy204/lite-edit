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
// Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue architecture
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
//!
//! ## Unified Event Queue Architecture
//!
//! The application uses a unified event queue to eliminate `Rc<RefCell<>>` borrow
//! conflicts that caused reentrant panics when PTY wakeup callbacks fired during
//! modal dialogs. All event sources (keyboard, mouse, scroll, PTY wakeup, blink
//! timer, resize) send events through an `mpsc` channel, and a single drain loop
//! processes them sequentially with exclusive ownership of the editor state.

mod buffer_target;
mod clipboard;
// Chunk: docs/chunks/renderer_styled_content - ColorPalette for styled text
mod color_palette;
// Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog widget
mod confirm_dialog;
mod context;
// Chunk: docs/chunks/workspace_dir_picker - Directory picker for new workspaces
mod dir_picker;
mod dirty_region;
// Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue
mod drain_loop;
// Chunk: docs/chunks/pty_wakeup_reentrant - Editor event types
mod editor_event;
mod editor_state;
// Chunk: docs/chunks/pty_wakeup_reentrant - Event channel (sender/receiver)
mod event_channel;
// Chunk: docs/chunks/fuzzy_file_matcher - File index for fuzzy file matching
pub mod file_index;
mod focus;
mod font;
mod glyph_atlas;
mod glyph_buffer;
// Chunk: docs/chunks/syntax_highlighting - Syntax-highlighted buffer view wrapper
mod highlighted_buffer;
mod input;
mod left_rail;
mod metal_view;
// Chunk: docs/chunks/mini_buffer_model - MiniBuffer single-line editing model
mod mini_buffer;
// Chunk: docs/chunks/tiling_workspace_integration - Pane layout data structures
mod pane_layout;
// Chunk: docs/chunks/tiling_multi_pane_render - Pane frame rendering
mod pane_frame_buffer;
mod renderer;
// Chunk: docs/chunks/row_scroller_extract - Reusable scroll arithmetic
mod row_scroller;
// Chunk: docs/chunks/pty_wakeup_reentrant - CFRunLoopSource wrapper
mod runloop_source;
mod tab_bar;
mod selector;
mod selector_overlay;
mod shader;
mod viewport;
// Chunk: docs/chunks/welcome_screen - Welcome screen for empty file tabs
mod welcome_screen;
mod workspace;
mod wrap_layout;
// Chunk: docs/chunks/workspace_session_persistence - Session persistence
mod session;

pub use file_index::FileIndex;
pub use row_scroller::RowScroller;

use std::cell::RefCell;
use std::ptr::NonNull;

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

// Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue components
use crate::drain_loop::EventDrainLoop;
use crate::event_channel::{create_event_channel, EventSender};
use crate::runloop_source::{create_waker, RunLoopSource};
// The PtyWakeup type is now created via EventSender, not imported directly
// (Chunk: docs/chunks/pty_wakeup_reentrant - removed direct import)

use crate::editor_state::EditorState;
use crate::metal_view::MetalView;
use crate::renderer::Renderer;

/// Cursor blink interval in seconds
const CURSOR_BLINK_INTERVAL: f64 = 0.5;

// Chunk: docs/chunks/pty_wakeup_reentrant - Global drain loop pointer for the CFRunLoopSource callback
// The drain loop is stored in a global because the CFRunLoopSource callback
// receives a raw void* context. We use Box::leak to get a 'static reference.
// This is safe because the drain loop lives for the entire application lifetime.
static mut DRAIN_LOOP: Option<*mut EventDrainLoop> = None;

// =============================================================================
// Application Delegate
// =============================================================================

// Chunk: docs/chunks/pty_wakeup_reentrant - Simplified ivars without Rc<RefCell<>> controller
/// Internal state for our application delegate
struct AppDelegateIvars {
    /// The main window (kept alive by the delegate)
    window: RefCell<Option<Retained<NSWindow>>>,
    /// Event sender for the window delegate to send resize events
    event_sender: RefCell<Option<EventSender>>,
    /// The cursor blink timer
    blink_timer: RefCell<Option<Retained<NSTimer>>>,
}

impl Default for AppDelegateIvars {
    fn default() -> Self {
        Self {
            window: RefCell::new(None),
            event_sender: RefCell::new(None),
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

        // Chunk: docs/chunks/workspace_session_persistence - Save session on clean exit
        #[unsafe(method(applicationWillTerminate:))]
        fn application_will_terminate(&self, _notification: &NSNotification) {
            // SAFETY: DRAIN_LOOP is set once in setup_window and never cleared.
            // We're on the main thread and the app is terminating, so no race.
            unsafe {
                if let Some(drain_loop_ptr) = DRAIN_LOOP {
                    let drain_loop = &*drain_loop_ptr;
                    if let Err(e) = session::save_session(drain_loop.editor()) {
                        eprintln!("Failed to save session: {}", e);
                    }
                }
            }
        }
    }

    // SAFETY: NSWindowDelegate protocol methods are implemented correctly
    // Chunk: docs/chunks/pty_wakeup_reentrant - Send resize events through channel
    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            // Send resize event through the channel
            let sender = self.ivars().event_sender.borrow();
            if let Some(sender) = sender.as_ref() {
                let _ = sender.send_resize();
            }
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, _notification: &NSNotification) {
            // Fires when the window moves between displays with different
            // scale factors (e.g., Retina ↔ non-Retina). Send resize event.
            let sender = self.ivars().event_sender.borrow();
            if let Some(sender) = sender.as_ref() {
                let _ = sender.send_resize();
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

    // Chunk: docs/chunks/startup_workspace_dialog - Resolve startup directory from CLI or dialog
    /// Resolves the startup directory for the initial workspace.
    ///
    /// This function implements the startup directory resolution logic:
    /// 1. If a directory argument is provided on the command line, use it
    /// 2. Otherwise, show the NSOpenPanel directory picker
    /// 3. Return None if the user cancels the picker (and no CLI arg was provided)
    ///
    /// For CLI argument validation: if a path is provided but doesn't exist or
    /// isn't a directory, falls back to showing the picker (graceful degradation).
    fn resolve_startup_directory(&self) -> Option<std::path::PathBuf> {
        // Check for command-line argument (first arg after program name)
        if let Some(arg) = std::env::args().nth(1) {
            let path = std::path::PathBuf::from(&arg);
            // Validate: must exist and be a directory
            if path.is_dir() {
                return Some(path);
            }
            // Invalid path, fall through to show picker
            // (graceful degradation rather than error)
        }

        // No valid CLI argument, show directory picker
        dir_picker::pick_directory()
    }

    // Chunk: docs/chunks/workspace_session_persistence - Check for CLI override
    /// Checks if a command-line argument was provided.
    ///
    /// When a CLI argument is provided, session restoration is skipped
    /// to allow users to explicitly open a different directory.
    fn has_cli_argument(&self) -> bool {
        if let Some(arg) = std::env::args().nth(1) {
            let path = std::path::PathBuf::from(&arg);
            path.is_dir()
        } else {
            false
        }
    }

    /// Sets up the main window with Metal rendering
    // Chunk: docs/chunks/startup_workspace_dialog - Directory selection before window creation
    // Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue setup
    fn setup_window(&self, mtm: MainThreadMarker) {
        // Activate the application and create the window BEFORE showing the
        // directory picker. This ensures the app has a visible presence on the
        // current macOS space/desktop so the NSOpenPanel modal dialog appears
        // in front rather than on a hidden desktop (which looks like a hang,
        // especially when launching from a full-screen terminal).
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);

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

        // Create the Metal-backed view and attach it to the window.
        let metal_view = MetalView::new(mtm, content_rect);
        window.setContentView(Some(&metal_view));
        metal_view.sync_backing_properties();

        // Make the window visible now so the app owns a space on the current
        // desktop. The directory picker modal will then appear on top of it.
        window.makeKeyAndOrderFront(None);

        // The renderer needs the correct scale factor to rasterize the font
        // and glyph atlas at native resolution (e.g., 2x on Retina).
        // viewDidChangeBackingProperties may not fire synchronously during
        // setContentView. sync_backing_properties above already handled this.

        // Create the renderer
        let mut renderer = Renderer::new(&metal_view);

        // Get font metrics from the renderer
        let font_metrics = renderer.font_metrics();

        // Chunk: docs/chunks/workspace_session_persistence - Session restoration or directory picker
        // Try to restore from session first (unless CLI argument was provided).
        // If no session exists or restoration fails, fall back to directory picker.
        let state = if !self.has_cli_argument() {
            // Try loading existing session
            if let Some(session_data) = session::load_session() {
                match session_data.restore_into_editor(font_metrics.line_height as f32) {
                    Ok(editor) => {
                        // Session restored successfully
                        let mut state = EditorState::new_deferred(font_metrics);
                        state.editor = editor;
                        Some(state)
                    }
                    Err(e) => {
                        eprintln!("Failed to restore session: {:?}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // If session restoration didn't work, use directory picker
        let mut state = match state {
            Some(s) => s,
            None => {
                // Chunk: docs/chunks/startup_workspace_dialog - Resolve directory before initializing editor
                // Resolve the startup directory after the window is visible so that the
                // NSOpenPanel appears on the same space as the app window.
                let startup_dir = match self.resolve_startup_directory() {
                    Some(dir) => dir,
                    None => {
                        // User cancelled the directory picker, terminate gracefully
                        app.terminate(None);
                        return;
                    }
                };

                // Chunk: docs/chunks/startup_workspace_dialog - Deferred editor state creation
                // Create the editor state with deferred initialization (no workspace yet),
                // then add the startup workspace with the user-selected directory.
                let mut state = EditorState::new_deferred(font_metrics);
                state.add_startup_workspace(startup_dir);
                state
            }
        };

        // Update viewport size based on window dimensions
        let frame = metal_view.frame();
        let scale = metal_view.scale_factor();
        let width = (frame.size.width * scale) as f32;
        let height = (frame.size.height * scale) as f32;
        state.update_viewport_dimensions(width, height);
        renderer.update_viewport_size(width, height);

        // ==========================================================================
        // Chunk: docs/chunks/pty_wakeup_reentrant - Set up unified event queue
        // ==========================================================================

        // Create the CFRunLoopSource that will wake the run loop when events arrive.
        // The source's callback will drain and process all pending events.
        // We create a placeholder callback first, then update it after creating
        // the drain loop (since the callback needs to reference the drain loop).
        let runloop_source = RunLoopSource::new(|| {
            // This callback is invoked when the CFRunLoopSource is signaled.
            // It processes all pending events from the channel.
            // SAFETY: DRAIN_LOOP is set once in setup_window and never cleared.
            unsafe {
                if let Some(drain_loop_ptr) = DRAIN_LOOP {
                    (*drain_loop_ptr).process_pending_events();
                }
            }
        });

        // Create the run loop waker that signals the CFRunLoopSource
        let waker = create_waker(&runloop_source);

        // Create the event channel
        let (sender, receiver) = create_event_channel(waker);

        // Chunk: docs/chunks/pty_wakeup_reentrant - Store EventSender for PTY wakeup
        // Set the event sender on EditorState so terminals can create PtyWakeup handles
        // that signal through the event channel.
        state.set_event_sender(sender.clone());

        // Create the drain loop (owns the state, renderer, and view)
        let mut drain_loop = EventDrainLoop::new(
            state,
            renderer,
            metal_view.clone(),
            receiver,
            sender.clone(),
        );

        // Set up the event sender on the MetalView
        metal_view.set_event_sender(sender.clone());

        // Make the view first responder to receive key events
        window.makeFirstResponder(Some(&metal_view));

        // Set up window delegate (self handles window events)
        let delegate: &ProtocolObject<dyn NSWindowDelegate> = ProtocolObject::from_ref(self);
        window.setDelegate(Some(delegate));

        // Perform initial render
        drain_loop.initial_render();

        // Set up cursor blink timer
        let blink_timer = self.setup_cursor_blink_timer(mtm, sender.clone());

        // Store the drain loop in the global pointer for the CFRunLoopSource callback
        // Box::leak gives us a 'static reference; we never deallocate it
        let drain_loop_box = Box::new(drain_loop);
        let drain_loop_ptr = Box::leak(drain_loop_box) as *mut EventDrainLoop;
        // SAFETY: We're on the main thread and this is only called once
        unsafe {
            DRAIN_LOOP = Some(drain_loop_ptr);
        }

        // Store state in ivars
        *self.ivars().window.borrow_mut() = Some(window.clone());
        *self.ivars().event_sender.borrow_mut() = Some(sender);
        *self.ivars().blink_timer.borrow_mut() = Some(blink_timer);

        // The RunLoopSource is kept alive by being added to the run loop.
        // We don't need to store it explicitly (it's never removed).
        std::mem::forget(runloop_source);
    }

    // Chunk: docs/chunks/pty_wakeup_reentrant - Timer sends events through channel
    /// Sets up the cursor blink timer
    fn setup_cursor_blink_timer(
        &self,
        _mtm: MainThreadMarker,
        sender: EventSender,
    ) -> Retained<NSTimer> {
        // Create a block for the timer callback
        let block = RcBlock::new(move |_timer: NonNull<NSTimer>| {
            // Send cursor blink event through the channel
            let _ = sender.send_cursor_blink();
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
