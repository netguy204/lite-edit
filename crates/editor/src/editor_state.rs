// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
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
use crate::input::KeyEvent;
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
}

impl EditorState {
    /// Creates a new EditorState with the given buffer.
    pub fn new(buffer: TextBuffer, line_height: f32) -> Self {
        Self {
            buffer,
            viewport: Viewport::new(line_height),
            dirty_region: DirtyRegion::None,
            focus_target: BufferFocusTarget::new(),
            cursor_visible: true,
            last_keystroke: Instant::now(),
        }
    }

    /// Creates an EditorState with an empty buffer.
    pub fn empty(line_height: f32) -> Self {
        Self::new(TextBuffer::new(), line_height)
    }

    /// Updates the viewport size based on window height in pixels.
    pub fn update_viewport_size(&mut self, window_height: f32) {
        self.viewport.update_size(window_height);
    }

    /// Handles a key event by forwarding to the active focus target.
    ///
    /// This records the keystroke time (for cursor blink reset) and
    /// ensures the cursor is visible after any keystroke.
    pub fn handle_key(&mut self, event: KeyEvent) {
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
        );
        self.focus_target.handle_key(event, &mut ctx);
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
        Self::empty(16.0) // Sensible default line height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_new_state() {
        let state = EditorState::empty(16.0);
        assert!(state.buffer.is_empty());
        assert!(!state.is_dirty());
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_handle_key_marks_dirty() {
        let mut state = EditorState::empty(16.0);
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));

        assert!(state.is_dirty());
        assert_eq!(state.buffer.content(), "a");
    }

    #[test]
    fn test_take_dirty_region_resets() {
        let mut state = EditorState::empty(16.0);
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));
        assert!(state.is_dirty());

        let dirty = state.take_dirty_region();
        assert!(dirty.is_dirty());
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_keystroke_shows_cursor() {
        let mut state = EditorState::empty(16.0);
        state.update_viewport_size(160.0);
        state.cursor_visible = false;

        state.handle_key(KeyEvent::char('a'));

        assert!(state.cursor_visible);
    }

    #[test]
    fn test_toggle_cursor_blink() {
        let mut state = EditorState::empty(16.0);
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
        let mut state = EditorState::empty(16.0);
        state.update_viewport_size(160.0);

        // Keystroke just happened
        state.last_keystroke = Instant::now();

        // Toggle should keep cursor visible
        state.toggle_cursor_blink();
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_viewport_size_update() {
        let mut state = EditorState::empty(16.0);
        state.update_viewport_size(320.0);

        assert_eq!(state.viewport.visible_lines(), 20); // 320 / 16 = 20
    }
}
