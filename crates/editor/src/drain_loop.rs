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
// Chunk: docs/chunks/focus_stack - Use FocusLayer for render decisions
use crate::editor_state::EditorState;
use crate::focus::FocusLayer;
use crate::event_channel::{EventReceiver, EventSender};
use crate::input::{KeyEvent, MarkedTextEvent, MouseEvent, ScrollDelta, TextInputEvent};
use crate::metal_view::{CursorRect, CursorRegions, MetalView};
use crate::renderer::Renderer;
use crate::confirm_dialog::calculate_confirm_dialog_geometry;
// Chunk: docs/chunks/find_strip_multi_pane - Import FindStripState for pane-aware rendering
use crate::selector_overlay::{calculate_overlay_geometry, FindStripState};
use crate::left_rail::RAIL_WIDTH;
use crate::pane_layout::calculate_pane_rects;
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
    /// Performance statistics collector (perf-instrumentation feature only)
    #[cfg(feature = "perf-instrumentation")]
    perf_stats: crate::perf_stats::PerfStats,
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
            #[cfg(feature = "perf-instrumentation")]
            perf_stats: crate::perf_stats::PerfStats::new(),
        }
    }

    /// Provides mutable access to the state for initial setup.
    pub fn state_mut(&mut self) -> &mut EditorState {
        &mut self.state
    }

    // Chunk: docs/chunks/workspace_session_persistence - Editor access for session save
    /// Returns a reference to the editor for session persistence.
    ///
    /// This is called during application termination to save the session.
    pub fn editor(&self) -> &crate::workspace::Editor {
        &self.state.editor
    }

    /// Provides mutable access to the renderer for initial setup.
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// Returns a reference to the metal view.
    pub fn metal_view(&self) -> &MetalView {
        &self.metal_view
    }

    // Chunk: docs/chunks/terminal_flood_starvation - Input-first event partitioning
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
    ///
    /// # Input-First Partitioning
    ///
    /// Events are partitioned so priority events (user input, resize) are
    /// processed before PTY wakeup events. This ensures input latency is
    /// bounded by the cost of processing input events, not by accumulated
    /// terminal output.
    pub fn process_pending_events(&mut self) {
        #[cfg(feature = "perf-instrumentation")]
        self.perf_stats.mark_frame_start();

        let mut had_pty_wakeup = false;

        // Drain all events from the channel into a Vec first to avoid borrow issues.
        // The drain() method borrows self.receiver, but we need to mutably borrow
        // self to process each event. Collecting into a Vec separates the lifetimes.
        let events: Vec<EditorEvent> = self.receiver.drain().collect();

        // Partition: process priority events (user input, resize) first, then
        // PTY wakeup and cursor blink events. This ensures input latency is
        // never gated by accumulated terminal output.
        let (priority_events, other_events): (Vec<_>, Vec<_>) = events
            .into_iter()
            .partition(|e| e.is_priority_event());

        // Process priority events first (user input, resize)
        for event in priority_events {
            self.process_single_event(event, &mut had_pty_wakeup);
        }

        // Then process other events (PtyWakeup, CursorBlink)
        for event in other_events {
            self.process_single_event(event, &mut had_pty_wakeup);
        }

        // Clear the wakeup pending flag if we processed a PTY wakeup
        if had_pty_wakeup {
            self.sender.clear_wakeup_pending();
        }

        // Render once after processing all events
        self.render_if_dirty();
    }

    // Chunk: docs/chunks/terminal_flood_starvation - Single event processing
    /// Processes a single event, updating the had_pty_wakeup flag as needed.
    fn process_single_event(&mut self, event: EditorEvent, had_pty_wakeup: &mut bool) {
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
                *had_pty_wakeup = true;
                self.handle_pty_wakeup();
            }
            EditorEvent::CursorBlink => {
                self.handle_cursor_blink();
            }
            EditorEvent::Resize => {
                self.handle_resize();
            }
            // Chunk: docs/chunks/dragdrop_file_paste - File drop handling
            EditorEvent::FileDrop(paths) => {
                self.handle_file_drop(paths);
            }
            // Chunk: docs/chunks/file_change_events - External file modification handling
            EditorEvent::FileChanged(path) => {
                self.handle_file_changed(path);
            }
            // Chunk: docs/chunks/deletion_rename_handling - External file deletion handling
            EditorEvent::FileDeleted(path) => {
                self.handle_file_deleted(path);
            }
            // Chunk: docs/chunks/deletion_rename_handling - External file rename handling
            EditorEvent::FileRenamed { from, to } => {
                self.handle_file_renamed(from, to);
            }
            // Chunk: docs/chunks/unicode_ime_input - Text input event handling
            EditorEvent::InsertText(event) => {
                self.handle_insert_text(event);
            }
            EditorEvent::SetMarkedText(event) => {
                self.handle_set_marked_text(event);
            }
            EditorEvent::UnmarkText => {
                self.handle_unmark_text();
            }
        }
    }

    // Chunk: docs/chunks/file_change_events - File change event handler
    // Chunk: docs/chunks/base_snapshot_reload - Reload clean buffers on external modification
    // Chunk: docs/chunks/three_way_merge - Merge dirty buffers on external modification
    // Chunk: docs/chunks/conflict_mode_lifecycle - Suppress FileChanged when in conflict mode
    /// Handles external file modification events.
    ///
    /// This method is called when the filesystem watcher detects that a file
    /// within the workspace was modified by an external process.
    ///
    /// For clean tabs (dirty == false), reloads the buffer from disk.
    /// For dirty tabs (dirty == true), performs a three-way merge to combine
    /// the user's local edits with the external changes.
    ///
    /// The self-write suppression check prevents reacting to our own saves.
    /// Tabs in conflict mode are skipped - they suppress auto-merge until
    /// the user saves to signal conflict resolution completion.
    fn handle_file_changed(&mut self, path: std::path::PathBuf) {
        // Check if this is a self-triggered event (our own save)
        if self.state.is_file_change_suppressed(&path) {
            // Ignore - this was our own write
            return;
        }

        // Chunk: docs/chunks/conflict_mode_lifecycle - Ignore events for tabs in conflict mode
        // When a tab is in conflict mode (has unresolved merge conflicts), we suppress
        // further auto-merge. The user must save (Cmd+S) to signal they've resolved
        // the conflicts, which clears conflict mode and allows auto-merge to resume.
        if self.state.is_tab_in_conflict_mode(&path) {
            // Ignore - tab is resolving conflicts, don't auto-merge
            return;
        }

        // Chunk: docs/chunks/base_snapshot_reload - File change event handler
        // Attempt to reload the file tab. The reload_file_tab method:
        // - Finds the tab across all workspaces
        // - Checks if the tab is clean (dirty == false)
        // - Reloads the buffer content from disk if clean
        // - Updates base_content and re-applies syntax highlighting
        // - Returns false if no matching tab or if tab is dirty
        if self.state.reload_file_tab(&path) {
            return;
        }

        // Chunk: docs/chunks/three_way_merge - Merge for dirty buffers
        // If reload returned false and a matching dirty tab exists, try merge.
        // The merge_file_tab method:
        // - Finds the tab across all workspaces
        // - Checks if the tab is dirty (dirty == true)
        // - Performs three-way merge: base_content → buffer, base_content → disk
        // - Updates buffer with merged content (including any conflict markers)
        // - Updates base_content to new disk content
        // - Re-applies syntax highlighting
        // - Sets conflict_mode = true if merge produced conflicts
        // - Returns Some(MergeResult) if merge was performed
        let _merge_result = self.state.merge_file_tab(&path);
    }

    // Chunk: docs/chunks/deletion_rename_handling - File deleted event handler
    /// Handles external file deletion events.
    ///
    /// This method is called when the filesystem watcher detects that a file
    /// with an open buffer was deleted by an external process.
    ///
    /// If a tab is open for this file, we show a confirm dialog asking the user
    /// whether to "Save" (recreate the file) or "Abandon" (close the tab).
    fn handle_file_deleted(&mut self, path: std::path::PathBuf) {
        self.state.handle_file_deleted(path);
    }

    // Chunk: docs/chunks/deletion_rename_handling - File renamed event handler
    /// Handles external file rename events.
    ///
    /// This method is called when the filesystem watcher detects that a file
    /// with an open buffer was renamed by an external process.
    ///
    /// Updates the `associated_file` of any matching tab to the new path and
    /// updates the tab label to reflect the new filename.
    fn handle_file_renamed(&mut self, from: std::path::PathBuf, to: std::path::PathBuf) {
        self.state.handle_file_renamed(from, to);
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
    // Chunk: docs/chunks/terminal_flood_starvation - Follow-up wakeup scheduling
    /// Handles PTY wakeup by polling agents/terminals.
    ///
    /// When any terminal hits its byte budget, schedules a follow-up wakeup
    /// to ensure remaining data is processed on the next cycle. This bounds
    /// the wall-clock cost of a single drain cycle while ensuring all data
    /// is eventually processed.
    fn handle_pty_wakeup(&mut self) {
        let (terminal_dirty, needs_rewakeup) = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }

        // If any terminal hit its byte budget, schedule a follow-up wakeup
        // so remaining data gets processed on the next cycle.
        // This uses send_pty_wakeup_followup which bypasses debouncing.
        if needs_rewakeup {
            let _ = self.sender.send_pty_wakeup_followup();
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
        let (terminal_dirty, needs_rewakeup) = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }
        // Schedule follow-up if needed (same logic as handle_pty_wakeup)
        if needs_rewakeup {
            let _ = self.sender.send_pty_wakeup_followup();
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

    // Chunk: docs/chunks/dragdrop_file_paste - File drop handling
    /// Handles file drop events by forwarding to the editor state.
    fn handle_file_drop(&mut self, paths: Vec<String>) {
        self.state.handle_file_drop(paths);
        self.poll_after_input();
    }

    // Chunk: docs/chunks/unicode_ime_input - Text input event handlers

    /// Handles text insertion events from IME, keyboard, paste, or dictation.
    ///
    /// This is the final text to insert - IME composition is complete.
    fn handle_insert_text(&mut self, event: TextInputEvent) {
        self.state.handle_insert_text(event);
        self.poll_after_input();
    }

    /// Handles IME composition (marked text) updates.
    ///
    /// The marked text should be displayed with an underline while composition
    /// is in progress. This may be called multiple times as the user types.
    fn handle_set_marked_text(&mut self, event: MarkedTextEvent) {
        self.state.handle_set_marked_text(event);
        self.poll_after_input();
    }

    /// Handles IME composition cancellation.
    ///
    /// This clears any marked text without inserting it.
    fn handle_unmark_text(&mut self) {
        self.state.handle_unmark_text();
        self.poll_after_input();
    }

    /// Polls PTY and picker after user input for responsive feedback.
    fn poll_after_input(&mut self) {
        let (terminal_dirty, needs_rewakeup) = self.state.poll_agents();
        if terminal_dirty.is_dirty() {
            self.state.dirty_region.merge(terminal_dirty);
        }
        // Schedule follow-up if needed (same logic as handle_pty_wakeup)
        if needs_rewakeup {
            let _ = self.sender.send_pty_wakeup_followup();
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

            // Chunk: docs/chunks/styled_line_cache - Handle styled line cache invalidation
            // Check if the cache should be fully cleared (e.g., on tab switch)
            if self.state.take_clear_styled_line_cache() {
                self.renderer.clear_styled_line_cache();
            } else {
                // Take the dirty lines and invalidate the styled line cache so that modified
                // lines are recomputed during the next render while unchanged lines are served
                // from cache.
                let dirty_lines = self.state.take_dirty_lines();
                self.renderer.invalidate_styled_lines(&dirty_lines);
            }

            #[cfg(feature = "perf-instrumentation")]
            self.perf_stats.record_dirty_region(&_dirty);

            // Chunk: docs/chunks/focus_stack - Render based on focus layer
            // Render based on current focus layer (derived from FocusStack)
            match self.state.focus_layer() {
                FocusLayer::Selector => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        self.state.active_selector.as_ref(),
                        self.state.overlay_cursor_visible,
                        None, // No find strip when selector is active
                    );
                }
                // Chunk: docs/chunks/find_strip_multi_pane - Use render_with_editor for find strip
                FocusLayer::FindInFile => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    // Build the find strip state from the mini buffer
                    // Note: We need to extract content into a local variable because
                    // FindStripState borrows it, and content() returns an owned String.
                    let find_strip = self.state.find_mini_buffer.as_ref().map(|mb| {
                        let content = mb.content();
                        (content, mb.cursor_col())
                    });
                    if let Some((ref query, cursor_col)) = find_strip {
                        self.renderer.render_with_editor(
                            &self.metal_view,
                            &self.state.editor,
                            None, // No selector when find is active
                            self.state.cursor_visible,
                            Some(FindStripState {
                                query,
                                cursor_col,
                                cursor_visible: self.state.overlay_cursor_visible,
                            }),
                        );
                    }
                }
                FocusLayer::Buffer | FocusLayer::GlobalShortcuts => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    self.renderer.render_with_editor(
                        &self.metal_view,
                        &self.state.editor,
                        None,
                        self.state.cursor_visible,
                        None, // No find strip
                    );
                }
                // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog rendering
                FocusLayer::ConfirmDialog => {
                    self.renderer.set_cursor_visible(self.state.cursor_visible);
                    self.renderer.render_with_confirm_dialog(
                        &self.metal_view,
                        &self.state.editor,
                        self.state.confirm_dialog.as_ref(),
                    );
                }
            }

            // Update cursor regions after rendering
            self.update_cursor_regions();

            // Record styled_line timing from the renderer
            #[cfg(feature = "perf-instrumentation")]
            if let Some((duration, line_count)) = self.renderer.take_styled_line_timing() {
                self.perf_stats.record_styled_line_batch(duration, line_count);
            }

            // Mark the frame complete for latency measurement
            #[cfg(feature = "perf-instrumentation")]
            self.perf_stats.mark_frame_end();
        }

        // Auto-report and on-demand dump (outside the is_dirty block so
        // frame_end is always recorded, but reporting can happen even on
        // skipped frames).
        #[cfg(feature = "perf-instrumentation")]
        {
            if self.perf_stats.should_auto_report() {
                eprint!("{}", self.perf_stats.report());
            }
            if self.state.dump_perf_stats {
                self.state.dump_perf_stats = false;
                eprint!("{}", self.perf_stats.report());
            }
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

        // Chunk: docs/chunks/pane_tabs_interaction - Tab bar pointer cursor for all panes
        // In multi-pane layouts, each pane has its own tab bar at its top edge.
        // We need to add a pointer cursor rect for each pane's tab bar.
        if let Some(workspace) = self.state.editor.active_workspace() {
            if workspace.tab_count() > 0 {
                // Calculate pane rects in pixel space (starting at RAIL_WIDTH, 0)
                let bounds = (
                    RAIL_WIDTH,
                    0.0,
                    view_width_px - RAIL_WIDTH,
                    view_height_px,
                );
                let pane_rects = calculate_pane_rects(bounds, &workspace.pane_root);

                for pane_rect in &pane_rects {
                    // Each pane's tab bar is at the top of its bounds
                    // In NSView coords (origin at bottom-left), we need to convert
                    // from screen-space y (origin at top) to NSView y
                    let tab_bar_x_pt = pane_rect.x as f64 / scale;
                    let tab_bar_width_pt = pane_rect.width as f64 / scale;
                    let tab_bar_height_pt = TAB_BAR_HEIGHT as f64 / scale;
                    // In screen space, pane_rect.y is the top of the pane (y=0 at top)
                    // Convert to NSView coords (y=0 at bottom): nsview_y = view_height - screen_y - height
                    let tab_bar_y_pt = view_height_pt - (pane_rect.y as f64 / scale) - tab_bar_height_pt;

                    regions.add_pointer(CursorRect::new(
                        tab_bar_x_pt,
                        tab_bar_y_pt,
                        tab_bar_width_pt,
                        tab_bar_height_pt,
                    ));
                }
            }
        }

        // Chunk: docs/chunks/focus_stack - Use focus_layer() for cursor region decisions
        // Selector Overlay (Pointer Cursor for items, I-beam for query)
        if self.state.focus_layer() == FocusLayer::Selector {
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

        // Chunk: docs/chunks/dialog_pointer_cursor - Pointer cursor for confirm dialog buttons
        // Confirm Dialog (Pointer Cursor for Cancel and Confirm buttons)
        if self.state.focus_layer() == FocusLayer::ConfirmDialog {
            if let Some(ref dialog) = self.state.confirm_dialog {
                let font_metrics = self.state.font_metrics();
                let line_height = font_metrics.line_height as f32;
                let glyph_width = font_metrics.advance_width as f32;

                let geometry = calculate_confirm_dialog_geometry(
                    view_width_px,
                    view_height_px,
                    line_height,
                    glyph_width,
                    dialog,
                );

                // Cancel button region
                let cancel_x_pt = geometry.cancel_button_x as f64 / scale;
                let cancel_y_pt = px_to_pt(geometry.buttons_y, geometry.button_height);
                let button_width_pt = geometry.button_width as f64 / scale;
                let button_height_pt = geometry.button_height as f64 / scale;

                regions.add_pointer(CursorRect::new(
                    cancel_x_pt,
                    cancel_y_pt,
                    button_width_pt,
                    button_height_pt,
                ));

                // Confirm/Abandon button region
                let confirm_x_pt = geometry.abandon_button_x as f64 / scale;

                regions.add_pointer(CursorRect::new(
                    confirm_x_pt,
                    cancel_y_pt, // Same Y as cancel button
                    button_width_pt,
                    button_height_pt,
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
