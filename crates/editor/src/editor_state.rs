// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/viewport_scrolling - Scroll event handling
// Chunk: docs/chunks/quit_command - Cmd+Q quit flag and key interception
// Chunk: docs/chunks/file_picker - File picker (Cmd+P) integration
//!
//! Editor state container.
//!
//! This module consolidates all mutable editor state into a single struct
//! that the main loop can work with. It provides the EditorContext for
//! focus target event handling.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::buffer_target::BufferFocusTarget;
use crate::context::EditorContext;
use crate::dirty_region::DirtyRegion;
use crate::file_index::FileIndex;
use crate::focus::FocusTarget;
use crate::font::FontMetrics;
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};
use crate::selector::{SelectorOutcome, SelectorWidget};
use crate::selector_overlay::calculate_overlay_geometry;
use crate::viewport::Viewport;
use lite_edit_buffer::TextBuffer;

/// Duration in milliseconds for cursor blink interval
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// Which UI element currently owns keyboard/mouse focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorFocus {
    /// Normal buffer editing mode
    #[default]
    Buffer,
    /// Selector overlay is active (file picker, command palette, etc.)
    Selector,
}

/// Consolidated editor state.
///
/// This struct holds all mutable state that the main loop needs:
/// - The text buffer being edited
/// - The viewport (scroll state)
/// - The active focus target
/// - Cursor visibility state
/// - Dirty region tracking
/// - Font metrics for pixel-to-position conversion
/// - Application quit flag
/// - File picker state (focus, selector widget, file index)
pub struct EditorState {
    /// The text buffer being edited
    pub buffer: TextBuffer,
    /// The viewport for buffer-to-screen coordinate mapping
    pub viewport: Viewport,
    /// Accumulated dirty region for the current event batch
    pub dirty_region: DirtyRegion,
    /// The active focus target (currently always the buffer target)
    pub focus_target: BufferFocusTarget,
    /// Whether the cursor is currently visible (for blink animation)
    pub cursor_visible: bool,
    /// Time of the last keystroke (for cursor blink reset)
    pub last_keystroke: Instant,
    /// Font metrics for pixel-to-position conversion
    font_metrics: FontMetrics,
    /// View height in pixels (for y-coordinate flipping in mouse events)
    view_height: f32,
    /// View width in pixels (for selector overlay geometry)
    view_width: f32,
    /// Whether the app should quit (set by Cmd+Q)
    pub should_quit: bool,
    /// Which UI element currently owns focus
    pub focus: EditorFocus,
    /// The active selector widget (when focus == Selector)
    pub active_selector: Option<SelectorWidget>,
    /// The file index for fuzzy file matching
    file_index: Option<FileIndex>,
    /// The cache version at the last query (for streaming refresh)
    last_cache_version: u64,
    /// The resolved path from the last selector confirmation
    /// (consumed by file_save chunk for buffer association)
    pub resolved_path: Option<PathBuf>,
}

impl EditorState {
    /// Creates a new EditorState with the given buffer and font metrics.
    ///
    /// # Arguments
    /// * `buffer` - The text buffer being edited
    /// * `font_metrics` - Font metrics for pixel-to-position conversion
    pub fn new(buffer: TextBuffer, font_metrics: FontMetrics) -> Self {
        let line_height = font_metrics.line_height as f32;
        Self {
            buffer,
            viewport: Viewport::new(line_height),
            dirty_region: DirtyRegion::None,
            focus_target: BufferFocusTarget::new(),
            cursor_visible: true,
            last_keystroke: Instant::now(),
            font_metrics,
            view_height: 0.0,
            view_width: 0.0,
            should_quit: false,
            focus: EditorFocus::Buffer,
            active_selector: None,
            file_index: None,
            last_cache_version: 0,
            resolved_path: None,
        }
    }

    /// Creates an EditorState with an empty buffer.
    pub fn empty(font_metrics: FontMetrics) -> Self {
        Self::new(TextBuffer::new(), font_metrics)
    }

    /// Returns the font metrics.
    pub fn font_metrics(&self) -> &FontMetrics {
        &self.font_metrics
    }

    /// Updates the viewport size based on window dimensions in pixels.
    ///
    /// This also updates the stored view_height and view_width for
    /// mouse event coordinate flipping and selector overlay geometry.
    pub fn update_viewport_size(&mut self, window_height: f32) {
        self.viewport.update_size(window_height);
        self.view_height = window_height;
    }

    /// Updates the viewport size with both width and height.
    ///
    /// This is the preferred method when both dimensions are available.
    pub fn update_viewport_dimensions(&mut self, window_width: f32, window_height: f32) {
        self.viewport.update_size(window_height);
        self.view_height = window_height;
        self.view_width = window_width;
    }

    /// Handles a key event by forwarding to the active focus target.
    ///
    /// This records the keystroke time (for cursor blink reset) and
    /// ensures the cursor is visible after any keystroke.
    ///
    /// App-level shortcuts (like Cmd+Q for quit, Cmd+P for file picker) are
    /// intercepted here before being forwarded to the focus target.
    ///
    /// If the cursor has been scrolled off-screen, we snap the viewport back
    /// to make the cursor visible BEFORE processing the keystroke.
    pub fn handle_key(&mut self, event: KeyEvent) {
        use crate::input::Key;

        // Check for app-level shortcuts before delegating to focus target
        // Cmd+Q (without Ctrl) triggers quit
        if event.modifiers.command && !event.modifiers.control {
            if let Key::Char('q') = event.key {
                self.should_quit = true;
                return;
            }

            // Cmd+P (without Ctrl) toggles file picker
            if let Key::Char('p') = event.key {
                self.handle_cmd_p();
                return;
            }
        }

        // Route based on current focus
        match self.focus {
            EditorFocus::Selector => {
                self.handle_key_selector(event);
            }
            EditorFocus::Buffer => {
                self.handle_key_buffer(event);
            }
        }
    }

    /// Handles Cmd+P to toggle the file picker.
    fn handle_cmd_p(&mut self) {
        match self.focus {
            EditorFocus::Buffer => {
                // Open the file picker
                self.open_file_picker();
            }
            EditorFocus::Selector => {
                // Close the file picker (toggle behavior)
                self.close_selector();
            }
        }
    }

    /// Opens the file picker selector.
    fn open_file_picker(&mut self) {
        // Get the current working directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Initialize file_index if needed
        if self.file_index.is_none() {
            self.file_index = Some(FileIndex::start(cwd.clone()));
        }

        // Query with empty string to get initial results
        let results = self.file_index.as_ref().unwrap().query("");

        // Create a new selector widget
        let mut selector = SelectorWidget::new();

        // Map results to display strings
        let items: Vec<String> = results
            .iter()
            .map(|r| r.path.display().to_string())
            .collect();
        selector.set_items(items);

        // Store the selector and update focus
        self.active_selector = Some(selector);
        self.focus = EditorFocus::Selector;
        self.last_cache_version = self.file_index.as_ref().unwrap().cache_version();

        // Mark full viewport dirty for overlay rendering
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Closes the active selector.
    fn close_selector(&mut self) {
        self.active_selector = None;
        self.focus = EditorFocus::Buffer;
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Handles a key event when the selector is focused.
    fn handle_key_selector(&mut self, event: KeyEvent) {
        let selector = match self.active_selector.as_mut() {
            Some(s) => s,
            None => return,
        };

        // Capture the previous query for change detection
        let prev_query = selector.query().to_string();

        // Forward to the selector widget
        let outcome = selector.handle_key(&event);

        match outcome {
            SelectorOutcome::Pending => {
                // Check if query changed
                let current_query = selector.query();
                if current_query != prev_query {
                    // Re-query the file index with the new query
                    if let Some(ref file_index) = self.file_index {
                        let results = file_index.query(current_query);
                        let items: Vec<String> = results
                            .iter()
                            .map(|r| r.path.display().to_string())
                            .collect();
                        // Need to reborrow selector mutably
                        if let Some(ref mut sel) = self.active_selector {
                            sel.set_items(items);
                        }
                        self.last_cache_version = file_index.cache_version();
                    }
                }
                // Mark dirty for any visual update (selection, query, etc.)
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            SelectorOutcome::Confirmed(idx) => {
                // Resolve the path and handle confirmation
                self.handle_selector_confirm(idx);
            }
            SelectorOutcome::Cancelled => {
                self.close_selector();
            }
        }
    }

    /// Handles selector confirmation (Enter pressed).
    fn handle_selector_confirm(&mut self, idx: usize) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Get items and query from selector
        let (items, query) = if let Some(ref selector) = self.active_selector {
            (selector.items().to_vec(), selector.query().to_string())
        } else {
            return;
        };

        // Resolve the path
        let resolved = self.resolve_picker_path(idx, &items, &query, &cwd);

        // Record the selection for recency
        if let Some(ref file_index) = self.file_index {
            file_index.record_selection(&resolved);
        }

        // Store the resolved path for file_save chunk to consume
        self.resolved_path = Some(resolved);

        // Close the selector
        self.close_selector();
    }

    /// Resolves the path from a selector confirmation.
    ///
    /// If `idx < items.len()`: returns `cwd / items[idx]`
    /// If `idx == usize::MAX` or query doesn't match: returns `cwd / query` (new file)
    /// If the resolved file doesn't exist, creates it as an empty file.
    fn resolve_picker_path(
        &self,
        idx: usize,
        items: &[String],
        query: &str,
        cwd: &Path,
    ) -> PathBuf {
        let resolved = if idx < items.len() {
            cwd.join(&items[idx])
        } else {
            // idx == usize::MAX (empty items) or out of range
            // Use the query as the new filename
            cwd.join(query)
        };

        // Create the file if it doesn't exist
        if !resolved.exists() && !query.is_empty() {
            // Attempt to create the file (ignore errors for now)
            let _ = std::fs::File::create(&resolved);
        }

        resolved
    }

    /// Handles a key event when the buffer is focused.
    fn handle_key_buffer(&mut self, event: KeyEvent) {
        // Record keystroke time for cursor blink reset
        self.last_keystroke = Instant::now();

        // Ensure cursor blink visibility is on when typing
        if !self.cursor_visible {
            self.cursor_visible = true;
            // Mark cursor line dirty to show it
            let cursor_line = self.buffer.cursor_position().line;
            let dirty = self.viewport.dirty_lines_to_region(
                &lite_edit_buffer::DirtyLines::Single(cursor_line),
                self.buffer.line_count(),
            );
            self.dirty_region.merge(dirty);
        }

        // If the cursor is off-screen (scrolled away), snap the viewport back
        // to make the cursor visible BEFORE processing the keystroke.
        // This ensures typing after scrolling doesn't edit at a position
        // the user can't see.
        let cursor_line = self.buffer.cursor_position().line;
        if self
            .viewport
            .buffer_line_to_screen_line(cursor_line)
            .is_none()
        {
            // Cursor is off-screen - scroll to make it visible
            let line_count = self.buffer.line_count();
            if self.viewport.ensure_visible(cursor_line, line_count) {
                // Viewport scrolled - mark full viewport dirty
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
        }

        // Create context and forward to focus target
        let mut ctx = EditorContext::new(
            &mut self.buffer,
            &mut self.viewport,
            &mut self.dirty_region,
            self.font_metrics,
            self.view_height,
        );
        self.focus_target.handle_key(event, &mut ctx);
    }

    /// Handles a mouse event by forwarding to the active focus target.
    ///
    /// This records the event time (for cursor blink reset) and
    /// ensures the cursor is visible after any mouse interaction.
    ///
    /// When the selector is focused, mouse events are forwarded to the selector
    /// widget using the overlay geometry.
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        // Route based on current focus
        match self.focus {
            EditorFocus::Selector => {
                self.handle_mouse_selector(event);
            }
            EditorFocus::Buffer => {
                self.handle_mouse_buffer(event);
            }
        }
    }

    /// Handles a mouse event when the selector is focused.
    fn handle_mouse_selector(&mut self, event: MouseEvent) {
        let selector = match self.active_selector.as_mut() {
            Some(s) => s,
            None => return,
        };

        // Calculate overlay geometry to map mouse coordinates
        let line_height = self.font_metrics.line_height as f32;
        let geometry = calculate_overlay_geometry(
            self.view_width,
            self.view_height,
            line_height,
            selector.items().len(),
        );

        // Convert mouse position to the format expected by selector
        // Mouse events arrive in view coordinates (y=0 at top)
        let position = event.position;

        // Forward to selector widget
        let outcome = selector.handle_mouse(
            position,
            event.kind,
            geometry.item_height as f64,
            geometry.list_origin_y as f64,
        );

        match outcome {
            SelectorOutcome::Pending => {
                // Mark dirty for visual update
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            SelectorOutcome::Confirmed(idx) => {
                self.handle_selector_confirm(idx);
            }
            SelectorOutcome::Cancelled => {
                self.close_selector();
            }
        }
    }

    /// Handles a mouse event when the buffer is focused.
    fn handle_mouse_buffer(&mut self, event: MouseEvent) {
        // Record event time for cursor blink reset (same as keystroke)
        self.last_keystroke = Instant::now();

        // Ensure cursor is visible when clicking
        if !self.cursor_visible {
            self.cursor_visible = true;
            // Mark cursor line dirty to show it
            let cursor_line = self.buffer.cursor_position().line;
            let dirty = self.viewport.dirty_lines_to_region(
                &lite_edit_buffer::DirtyLines::Single(cursor_line),
                self.buffer.line_count(),
            );
            self.dirty_region.merge(dirty);
        }

        // Create context and forward to focus target
        let mut ctx = EditorContext::new(
            &mut self.buffer,
            &mut self.viewport,
            &mut self.dirty_region,
            self.font_metrics,
            self.view_height,
        );
        self.focus_target.handle_mouse(event, &mut ctx);
    }

    /// Handles a scroll event by forwarding to the active focus target.
    ///
    /// Scroll events only affect the viewport, not the cursor position or buffer.
    /// The cursor may end up off-screen after scrolling, which is intentional.
    ///
    /// When the selector is open, scroll events are ignored.
    pub fn handle_scroll(&mut self, delta: ScrollDelta) {
        // Ignore scroll events when selector is open
        if self.focus == EditorFocus::Selector {
            return;
        }

        // Create context and forward to focus target
        let mut ctx = EditorContext::new(
            &mut self.buffer,
            &mut self.viewport,
            &mut self.dirty_region,
            self.font_metrics,
            self.view_height,
        );
        self.focus_target.handle_scroll(delta, &mut ctx);
    }

    /// Returns true if any screen region needs re-rendering.
    pub fn is_dirty(&self) -> bool {
        self.dirty_region.is_dirty()
    }

    /// Called periodically to check for streaming file index updates.
    ///
    /// When the selector is open and the file index has discovered new paths,
    /// this re-queries the index with the current query and updates the selector's
    /// item list. This is the mechanism by which results stream in during the
    /// initial directory walk.
    ///
    /// Returns `DirtyRegion::FullViewport` if items were updated, `None` otherwise.
    pub fn tick_picker(&mut self) -> DirtyRegion {
        // Only relevant when selector is active
        if self.focus != EditorFocus::Selector {
            return DirtyRegion::None;
        }

        let file_index = match &self.file_index {
            Some(idx) => idx,
            None => return DirtyRegion::None,
        };

        // Check if cache version has changed
        let current_version = file_index.cache_version();
        if current_version <= self.last_cache_version {
            return DirtyRegion::None;
        }

        // Re-query with current query
        let query = self
            .active_selector
            .as_ref()
            .map(|s| s.query().to_string())
            .unwrap_or_default();

        let results = file_index.query(&query);
        let items: Vec<String> = results
            .iter()
            .map(|r| r.path.display().to_string())
            .collect();

        // Update the selector items
        if let Some(ref mut widget) = self.active_selector {
            widget.set_items(items);
        }

        self.last_cache_version = current_version;

        DirtyRegion::FullViewport
    }

    /// Takes the dirty region, leaving `DirtyRegion::None` in its place.
    ///
    /// Call this after rendering to reset the dirty state.
    pub fn take_dirty_region(&mut self) -> DirtyRegion {
        std::mem::take(&mut self.dirty_region)
    }

    /// Toggles cursor visibility for blink animation.
    ///
    /// Returns the dirty region for the cursor line if visibility changed.
    /// If the user recently typed, this keeps the cursor solid instead of toggling.
    pub fn toggle_cursor_blink(&mut self) -> DirtyRegion {
        let now = Instant::now();
        let since_keystroke = now.duration_since(self.last_keystroke);

        // If user typed recently, keep cursor solid
        if since_keystroke.as_millis() < CURSOR_BLINK_INTERVAL_MS as u128 {
            if !self.cursor_visible {
                self.cursor_visible = true;
                return self.cursor_dirty_region();
            }
            return DirtyRegion::None;
        }

        // Toggle visibility
        self.cursor_visible = !self.cursor_visible;
        self.cursor_dirty_region()
    }

    /// Returns the dirty region for just the cursor line.
    fn cursor_dirty_region(&self) -> DirtyRegion {
        let cursor_line = self.buffer.cursor_position().line;
        self.viewport.dirty_lines_to_region(
            &lite_edit_buffer::DirtyLines::Single(cursor_line),
            self.buffer.line_count(),
        )
    }

    /// Marks the full viewport as dirty (e.g., after buffer replacement).
    pub fn mark_full_dirty(&mut self) {
        self.dirty_region = DirtyRegion::FullViewport;
    }
}

impl Default for EditorState {
    fn default() -> Self {
        // Sensible default font metrics
        let font_metrics = FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        };
        Self::empty(font_metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, Modifiers, ScrollDelta};
    use std::time::Duration;

    /// Creates test font metrics with known values
    fn test_font_metrics() -> FontMetrics {
        FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        }
    }

    #[test]
    fn test_new_state() {
        let state = EditorState::empty(test_font_metrics());
        assert!(state.buffer.is_empty());
        assert!(!state.is_dirty());
        assert!(state.cursor_visible);
        assert!(!state.should_quit);
    }

    #[test]
    fn test_handle_key_marks_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));

        assert!(state.is_dirty());
        assert_eq!(state.buffer.content(), "a");
    }

    #[test]
    fn test_take_dirty_region_resets() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));
        assert!(state.is_dirty());

        let dirty = state.take_dirty_region();
        assert!(dirty.is_dirty());
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_keystroke_shows_cursor() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);
        state.cursor_visible = false;

        state.handle_key(KeyEvent::char('a'));

        assert!(state.cursor_visible);
    }

    #[test]
    fn test_toggle_cursor_blink() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Set last_keystroke to the past so blink toggle works
        state.last_keystroke = Instant::now() - Duration::from_secs(1);

        assert!(state.cursor_visible);
        state.toggle_cursor_blink();
        assert!(!state.cursor_visible);
        state.toggle_cursor_blink();
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_recent_keystroke_keeps_cursor_solid() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Keystroke just happened
        state.last_keystroke = Instant::now();

        // Toggle should keep cursor visible
        state.toggle_cursor_blink();
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_viewport_size_update() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        assert_eq!(state.viewport.visible_lines(), 20); // 320 / 16 = 20
        assert_eq!(state.view_height, 320.0);
    }

    // =========================================================================
    // Quit flag tests (Cmd+Q behavior)
    // =========================================================================

    #[test]
    fn test_cmd_q_sets_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Cmd+Q should set should_quit
        let cmd_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_q);

        assert!(state.should_quit);
    }

    #[test]
    fn test_cmd_q_does_not_modify_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content first
        state.handle_key(KeyEvent::char('a'));
        assert_eq!(state.buffer.content(), "a");

        // Cmd+Q should NOT add 'q' to the buffer
        let cmd_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_q);

        // Buffer should be unchanged
        assert_eq!(state.buffer.content(), "a");
        assert!(state.should_quit);
    }

    #[test]
    fn test_ctrl_q_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Ctrl+Q should NOT set should_quit (different binding)
        let ctrl_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        state.handle_key(ctrl_q);

        assert!(!state.should_quit);
    }

    #[test]
    fn test_cmd_ctrl_q_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Cmd+Ctrl+Q should NOT set should_quit (we explicitly check !control)
        let cmd_ctrl_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                command: true,
                control: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_ctrl_q);

        assert!(!state.should_quit);
    }

    #[test]
    fn test_cmd_z_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Other Cmd+ combinations should NOT set should_quit
        let cmd_z = KeyEvent::new(
            Key::Char('z'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_z);

        assert!(!state.should_quit);
    }

    #[test]
    fn test_plain_q_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Plain 'q' should type 'q', not quit
        state.handle_key(KeyEvent::char('q'));

        assert!(!state.should_quit);
        assert_eq!(state.buffer.content(), "q");
    }

    // =========================================================================
    // Scroll handling tests
    // =========================================================================

    #[test]
    fn test_handle_scroll_moves_viewport() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0); // 10 visible lines

        // Initial scroll offset should be 0
        assert_eq!(state.viewport.first_visible_line(), 0);

        // Scroll down by 5 lines (positive dy = scroll down)
        // line_height is 16.0, so 5 lines = 80 pixels
        state.handle_scroll(ScrollDelta::new(0.0, 80.0));

        // Viewport should have scrolled
        assert_eq!(state.viewport.first_visible_line(), 5);
        assert!(state.is_dirty()); // Should be dirty after scroll
    }

    #[test]
    fn test_handle_scroll_does_not_move_cursor() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0);

        // Set cursor to line 3
        state.buffer.set_cursor(lite_edit_buffer::Position::new(3, 5));

        // Scroll down by 10 lines
        state.handle_scroll(ScrollDelta::new(0.0, 160.0));

        // Cursor position should be unchanged
        assert_eq!(
            state.buffer.cursor_position(),
            lite_edit_buffer::Position::new(3, 5)
        );
    }

    #[test]
    fn test_keystroke_snaps_back_when_cursor_off_screen() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0); // 10 visible lines

        // Cursor starts at line 0
        assert_eq!(state.buffer.cursor_position().line, 0);

        // Scroll down so cursor is off-screen (scroll to show lines 15-24)
        state.handle_scroll(ScrollDelta::new(0.0, 15.0 * 16.0)); // 15 lines * 16 pixels
        assert_eq!(state.viewport.first_visible_line(), 15);

        // Clear dirty flag
        let _ = state.take_dirty_region();

        // Now type a character - viewport should snap back to show cursor
        state.handle_key(KeyEvent::char('X'));

        // Cursor should still be at line 0, and viewport should have scrolled
        // back to make line 0 visible
        assert_eq!(state.buffer.cursor_position().line, 0);
        assert_eq!(state.viewport.first_visible_line(), 0);
        assert!(state.is_dirty()); // Should be dirty after snap-back
    }

    #[test]
    fn test_no_snapback_when_cursor_visible() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0); // 10 visible lines

        // Move cursor to line 15
        state.buffer.set_cursor(lite_edit_buffer::Position::new(15, 0));

        // Scroll to make line 15 visible (show lines 10-19)
        state.viewport.scroll_to(10, 50);
        assert_eq!(state.viewport.first_visible_line(), 10);

        // Clear dirty flag
        let _ = state.take_dirty_region();

        // Type a character - viewport should NOT snap back since cursor is visible
        state.handle_key(KeyEvent::char('X'));

        // Scroll offset should remain at 10
        assert_eq!(state.viewport.first_visible_line(), 10);
    }

    // =========================================================================
    // File Picker Tests (Cmd+P behavior)
    // =========================================================================

    #[test]
    fn test_initial_focus_is_buffer() {
        let state = EditorState::empty(test_font_metrics());
        assert_eq!(state.focus, EditorFocus::Buffer);
    }

    #[test]
    fn test_initial_active_selector_is_none() {
        let state = EditorState::empty(test_font_metrics());
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_cmd_p_transitions_to_selector_focus() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        assert_eq!(state.focus, EditorFocus::Selector);
    }

    #[test]
    fn test_cmd_p_opens_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        assert!(state.active_selector.is_some());
    }

    #[test]
    fn test_cmd_p_does_not_insert_p() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Buffer should remain empty - 'p' should not be inserted
        assert!(state.buffer.is_empty());
    }

    #[test]
    fn test_cmd_p_when_selector_open_closes_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );

        // Open the selector
        state.handle_key(cmd_p.clone());
        assert_eq!(state.focus, EditorFocus::Selector);

        // Press Cmd+P again - should close
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_escape_closes_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Press Escape
        let escape = KeyEvent::new(Key::Escape, Modifiers::default());
        state.handle_key(escape);

        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_typing_in_selector_appends_to_query() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Type some characters
        state.handle_key(KeyEvent::char('t'));
        state.handle_key(KeyEvent::char('e'));
        state.handle_key(KeyEvent::char('s'));
        state.handle_key(KeyEvent::char('t'));

        // Check query
        let query = state.active_selector.as_ref().unwrap().query();
        assert_eq!(query, "test");
    }

    #[test]
    fn test_down_arrow_moves_selection_in_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Set some items manually for testing
        if let Some(ref mut selector) = state.active_selector {
            selector.set_items(vec!["file1.rs".into(), "file2.rs".into(), "file3.rs".into()]);
            assert_eq!(selector.selected_index(), 0);
        }

        // Press Down
        state.handle_key(KeyEvent::new(Key::Down, Modifiers::default()));

        let selected = state.active_selector.as_ref().unwrap().selected_index();
        assert_eq!(selected, 1);
    }

    #[test]
    fn test_scroll_ignored_when_selector_open() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_dimensions(800.0, 160.0); // 10 visible lines

        // Initial scroll offset should be 0
        assert_eq!(state.viewport.scroll_offset(), 0);

        // Open the selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Try to scroll
        state.handle_scroll(ScrollDelta::new(0.0, 80.0));

        // Viewport should NOT have scrolled (scroll ignored when selector open)
        assert_eq!(state.viewport.scroll_offset(), 0);
    }

    #[test]
    fn test_tick_picker_returns_none_when_buffer_focused() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Focus is Buffer, tick_picker should return None
        let dirty = state.tick_picker();
        assert!(!dirty.is_dirty());
    }

    #[test]
    fn test_tick_picker_returns_none_when_no_version_change() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Clear dirty region from opening
        let _ = state.take_dirty_region();

        // First tick - might update if cache changed
        let _first = state.tick_picker();

        // Second tick immediately - should return None (no change)
        let dirty = state.tick_picker();
        assert!(!dirty.is_dirty());
    }
}
