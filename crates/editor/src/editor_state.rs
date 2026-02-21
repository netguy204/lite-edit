// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
//!
//! Editor state container.
//!
//! This module consolidates all mutable editor state into a single struct
//! that the main loop can work with. It provides the EditorContext for
//! focus target event handling.

use std::time::Instant;

use crate::buffer_target::BufferFocusTarget;
use crate::context::EditorContext;
use crate::dirty_region::DirtyRegion;
use crate::focus::FocusTarget;
use crate::font::FontMetrics;
use crate::input::{KeyEvent, MouseEvent};
use crate::viewport::Viewport;
use lite_edit_buffer::TextBuffer;

/// Duration in milliseconds for cursor blink interval
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

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
    /// Whether the app should quit (set by Cmd+Q)
    pub should_quit: bool,
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
            should_quit: false,
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

    /// Updates the viewport size based on window height in pixels.
    ///
    /// This also updates the stored view_height for mouse event coordinate flipping.
    pub fn update_viewport_size(&mut self, window_height: f32) {
        self.viewport.update_size(window_height);
        self.view_height = window_height;
    }

    /// Handles a key event by forwarding to the active focus target.
    ///
    /// This records the keystroke time (for cursor blink reset) and
    /// ensures the cursor is visible after any keystroke.
    ///
    /// App-level shortcuts (like Cmd+Q for quit) are intercepted here
    /// before being forwarded to the focus target.
    pub fn handle_key(&mut self, event: KeyEvent) {
        use crate::input::Key;

        // Check for app-level shortcuts before delegating to focus target
        // Cmd+Q (without Ctrl) triggers quit
        if event.modifiers.command && !event.modifiers.control {
            if let Key::Char('q') = event.key {
                self.should_quit = true;
                return;
            }
        }

        // Record keystroke time for cursor blink reset
        self.last_keystroke = Instant::now();

        // Ensure cursor is visible when typing
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
        self.focus_target.handle_key(event, &mut ctx);
    }

    /// Handles a mouse event by forwarding to the active focus target.
    ///
    /// This records the event time (for cursor blink reset) and
    /// ensures the cursor is visible after any mouse interaction.
    pub fn handle_mouse(&mut self, event: MouseEvent) {
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

    /// Returns true if any screen region needs re-rendering.
    pub fn is_dirty(&self) -> bool {
        self.dirty_region.is_dirty()
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
    use crate::input::{Key, Modifiers};
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
}
