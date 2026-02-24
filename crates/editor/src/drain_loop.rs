// Chunk: docs/chunks/pty_wakeup_reentrant - Event drain loop (single ownership of controller)
//! Event drain loop for processing the unified event queue.
//!
//! This module provides `EventDrainLoop`, which owns the `EditorController`
//! directly (no `Rc`, no `RefCell`). All events flow through the event channel
//! and are processed sequentially by the drain loop, eliminating reentrant
//! borrow panics.
//!
//! # Architecture
//!
//! ```text
//! NSView callbacks ─────────────────────┐
//! PTY reader thread ────────────────────┤──→ EventSender ──→ mpsc channel
//! Blink timer ──────────────────────────┤
//! Window delegate ──────────────────────┘
//!                                                               │
//!                                                               ▼
//!                                       CFRunLoopSource drain callback
//!                                                               │
//!                                                               ▼
//!                                       EventDrainLoop::process_pending_events()
//!                                                               │
//!                                                               ▼
//!                                       EditorController (owned directly)
//! ```

use objc2::rc::Retained;
use objc2_app_kit::NSApplication;
use objc2_foundation::{MainThreadMarker, NSString};

use crate::editor_event::EditorEvent;
use crate::editor_state::{EditorFocus, EditorState};
use crate::event_channel::{EventReceiver, EventSender};
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};
use crate::metal_view::{CursorRect, CursorRegions, MetalView};
use crate::renderer::Renderer;
use crate::selector_overlay::calculate_overlay_geometry;
use crate::left_rail::RAIL_WIDTH;
use crate::tab_bar::TAB_BAR_HEIGHT;

/// The event drain loop that owns the editor controller.
///
/// This is the single point of access to the `EditorController`. The drain loop:
/// 1. Receives events from the `EventReceiver`
/// 2. Processes each event sequentially
/// 3. Renders once after draining all events (if dirty)
///
/// Because the controller is owned directly (not wrapped in `Rc<RefCell<>>`),
/// there are no borrow conflicts. Events are processed one at a time.
pub struct EventDrainLoop {
    /// The editor state, renderer, and view - owned directly
    state: EditorState,
    renderer: Renderer,
    metal_view: Retained<MetalView>,
    /// Last window title that was set, to avoid redundant updates
    last_window_title: String,
    /// The event receiver (main thread only)
    receiver: EventReceiver,
    /// The event sender (for clearing wakeup pending flag)
    sender: EventSender,
}

impl EventDrainLoop {
    /// Creates a new event drain loop.
    ///
    /// # Arguments
    /// * `state` - The editor state (owned)
    /// * `renderer` - The renderer (owned)
    /// * `metal_view` - The Metal view
    /// * `receiver` - The event receiver
    /// * `sender` - The event sender (for clearing wakeup pending flag)
    pub fn new(
        state: EditorState,
        renderer: Renderer,
        metal_view: Retained<MetalView>,
        receiver: EventReceiver,
        sender: EventSender,
    ) -> Self {
        Self {
            state,
            renderer,
            metal_view,
            last_window_title: String::new(),
            receiver,
            sender,
        }
    }

    /// Provides mutable access to the state for initial setup.
    pub fn state_mut(&mut self) -> &mut EditorState {
        &mut self.state
    }

    /// Provides mutable access to the renderer for initial setup.
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// Returns a reference to the metal view.
    pub fn metal_view(&self) -> &MetalView {
        &self.metal_view
    }

    /// Processes all pending events from the channel.
    ///
    /// This is the main entry point, called by the CFRunLoopSource callback.
    /// It drains all events from the channel, processes each one, and then
    /// renders once if anything is dirty.
    ///
    /// # Drain-All-Then-Render Pattern
    ///
    /// Multiple events are batched: we process all available events before
    /// rendering. This ensures:
    /// - Latency fairness: no event is penalized by intermediate renders
    /// - Efficiency: rapid PTY output coalesces into one render
    pub fn process_pending_events(&mut self) {
        let mut had_pty_wakeup = false;

        // Drain all events from the channel into a Vec first to avoid borrow issues.
        // The drain() method borrows self.receiver, but we need to mutably borrow
        // self to process each event. Collecting into a Vec separates the lifetimes.
        let events: Vec<EditorEvent> = self.receiver.drain().collect();

        for event in events {
            match event {
                EditorEvent::Key(key_event) => {
                    self.handle_key(key_event);
                }
                EditorEvent::Mouse(mouse_event) => {
                    self.handle_mouse(mouse_event);
                }
                EditorEvent::Scroll(scroll_delta) => {
                    self.handle_scroll(scroll_delta);
                }
                EditorEvent::PtyWakeup => {
                    had_pty_wakeup = true;
                    self.handle_pty_wakeup();
                }
                EditorEvent::CursorBlink => {
                    self.handle_cursor_blink();
                }
                EditorEvent::Resize => {
                    self.handle_resize();
                }
            }
        }

        // Clear the wakeup pending flag if we processed a PTY wakeup
        if had_pty_wakeup {
            self.sender.clear_wakeup_pending();
        }

        // Render once after processing all events
        self.render_if_dirty();
    }

    /// Handles a key event by forwarding to the editor state.
    fn handle_key(&mut self, event: KeyEvent) {
        self.state.handle_key(event);

        // Check for quit request
        if self.state.should_quit {
            self.terminate_app();
            return;
        }

        // Poll immediately after input for responsive terminal echo
        self.poll_after_input();
    }

    /// Handles a mouse event by forwarding to the editor state.
    fn handle_mouse(&mut self, event: MouseEvent) {
        self.state.handle_mouse(event);
        self.poll_after_input();
    }

    /// Handles a scroll event by forwarding to the editor state.
    fn handle_scroll(&mut self, delta: ScrollDelta) {
        self.state.handle_scroll(delta);
        self.poll_after_input();
    }

    // Chunk: docs/chunks/terminal_pty_wakeup - Handler that polls agents when PTY data arrives
    /// Handles PTY wakeup by polling agents/terminals.
    fn handle_pty_wakeup(&mut self) {
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }
    }

    /// Handles cursor blink timer by toggling visibility.
    /// Chunk: docs/chunks/file_picker - Integration of tick_picker into timer-driven refresh loop
    fn handle_cursor_blink(&mut self) {
        let cursor_dirty = self.state.toggle_cursor_blink();
        if cursor_dirty.is_dirty() {
            self.state.dirty_region.merge(cursor_dirty);
        }

        // Also poll PTY events on timer tick (backup for any missed wakeups)
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        // Check for picker streaming updates
        let picker_dirty = self.state.tick_picker();
        if picker_dirty.is_dirty() {
            self.state.dirty_region.merge(picker_dirty);
        }
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

        // Mark full viewport dirty
        self.state.mark_full_dirty();
    }

    /// Polls PTY and picker after user input for responsive feedback.
    fn poll_after_input(&mut self) {
        let terminal_dirty = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        let picker_dirty = self.state.tick_picker();
        if picker_dirty.is_dirty() {
            self.state.dirty_region.merge(picker_dirty);
        }
    }

    /// Terminates the macOS application.
    fn terminate_app(&self) {
        let mtm = MainThreadMarker::new().expect("must be on main thread");
        let app = NSApplication::sharedApplication(mtm);
        app.terminate(None);
    }

    /// Renders if there's a dirty region.
    /// Chunk: docs/chunks/file_picker - Conditional render_with_selector when focus is Selector
    fn render_if_dirty(&mut self) {
        // Update window title if needed
        self.update_window_title_if_needed();

        if self.state.is_dirty() {
            // Chunk: docs/chunks/pane_scroll_isolation - Viewport sync removed
            // Viewport sync used to happen here, but now render_with_editor and render_pane
            // configure the viewport from the active tab's viewport before rendering.
            // This ensures each pane uses its own scroll state in multi-pane mode.

            // Take the dirty region
            let _dirty = self.state.take_dirty_region();

            // Render based on current focus mode
            match self.state.focus {
                EditorFocus::Selector => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        self.state.active_selector.as_ref(),
                        self.state.overlay_cursor_visible,
                    );
                }
                EditorFocus::FindInFile => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
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
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        None,
                        self.state.cursor_visible,
                    );
                }
            }

            // Update cursor regions after rendering
            self.update_cursor_regions();
        }
    }

    /// Updates the window title if it has changed.
    fn update_window_title_if_needed(&mut self) {
        let current_title = self.state.window_title();
        if current_title != self.last_window_title {
            if let Some(window) = self.metal_view.window() {
                window.setTitle(&NSString::from_str(&current_title));
            }
            self.last_window_title = current_title;
        }
    }

    /// Calculates and sets cursor regions for the current UI state.
    fn update_cursor_regions(&self) {
        let frame = self.metal_view.frame();
        let scale = self.metal_view.scale_factor();

        let view_width_px = (frame.size.width * scale) as f32;
        let view_height_px = (frame.size.height * scale) as f32;
        let view_width_pt = frame.size.width;
        let view_height_pt = frame.size.height;

        let mut regions = CursorRegions::new();

        let px_to_pt = |y_px: f32, height_px: f32| -> f64 {
            view_height_pt - ((y_px + height_px) as f64 / scale)
        };

        // Left Rail (Pointer Cursor)
        {
            let rail_width_pt = RAIL_WIDTH as f64 / scale;
            regions.add_pointer(CursorRect::new(0.0, 0.0, rail_width_pt, view_height_pt));
        }

        // Tab Bar (Pointer Cursor)
        if let Some(workspace) = self.state.editor.active_workspace() {
            if workspace.tab_count() > 0 {
                let tab_bar_x_pt = RAIL_WIDTH as f64 / scale;
                let tab_bar_width_pt = view_width_pt - tab_bar_x_pt;
                let tab_bar_height_pt = TAB_BAR_HEIGHT as f64 / scale;
                let tab_bar_y_pt = view_height_pt - tab_bar_height_pt;

                regions.add_pointer(CursorRect::new(
                    tab_bar_x_pt,
                    tab_bar_y_pt,
                    tab_bar_width_pt,
                    tab_bar_height_pt,
                ));
            }
        }

        // Selector Overlay (Pointer Cursor for items, I-beam for query)
        if let EditorFocus::Selector = self.state.focus {
            if let Some(ref selector) = self.state.active_selector {
                let line_height = self.state.font_metrics().line_height as f32;
                let geometry = calculate_overlay_geometry(
                    view_width_px,
                    view_height_px,
                    line_height,
                    selector.items().len(),
                );

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

        // Buffer Content Area (I-beam Cursor)
        {
            let content_x_pt = RAIL_WIDTH as f64 / scale;
            let content_width_pt = view_width_pt - content_x_pt;

            let tab_bar_height_pt = if self.state.editor.active_workspace().map_or(false, |ws| ws.tab_count() > 0) {
                TAB_BAR_HEIGHT as f64 / scale
            } else {
                0.0
            };

            let content_height_pt = view_height_pt - tab_bar_height_pt;
            let content_y_pt = 0.0;

            regions.add_ibeam(CursorRect::new(
                content_x_pt,
                content_y_pt,
                content_width_pt,
                content_height_pt,
            ));
        }

        self.metal_view.set_cursor_regions(regions);
    }

    /// Performs initial render.
    pub fn initial_render(&mut self) {
        self.state.mark_full_dirty();
        self.render_if_dirty();
    }
}
