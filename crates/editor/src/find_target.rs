// Chunk: docs/chunks/focus_stack - Find focus target
//!
//! Find-in-file focus target.
//!
//! This module provides [`FindFocusTarget`], a focus target that wraps
//! a [`MiniBuffer`] and handles key events for the find-in-file strip.
//!
//! The find target handles its own key events (typing, escape, return)
//! and returns `Handled::Yes` for all of them. This means keys pressed while
//! the find strip is active won't propagate to lower layers.
//!
//! After handling, check `pending_outcome` to see what action the user took.

use crate::context::EditorContext;
use crate::focus::{FocusLayer, FocusTarget, Handled};
use crate::input::{Key, KeyEvent, MouseEvent, ScrollDelta};
use crate::mini_buffer::MiniBuffer;

/// Outcome of handling a key event in the find strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindOutcome {
    /// The find strip is still open; query may have changed.
    Pending,
    /// User pressed Escape - close the find strip.
    Closed,
    /// User pressed Return - advance to next match.
    FindNext,
}

/// Focus target for the find-in-file strip.
///
/// This wraps a [`MiniBuffer`] and provides the FocusTarget interface.
/// All key events are handled by this target when it's at the top of the
/// focus stack, preventing propagation to lower layers.
///
/// After dispatch, check `pending_outcome` to see if the user closed
/// the strip or wants to find the next match.
pub struct FindFocusTarget {
    /// The wrapped mini buffer for query editing.
    mini_buffer: MiniBuffer,
    /// The outcome from the last key event, if any.
    pending_outcome: Option<FindOutcome>,
    /// Whether the query changed in the last key event.
    query_changed: bool,
}

impl FindFocusTarget {
    /// Creates a new find focus target wrapping the given mini buffer.
    pub fn new(mini_buffer: MiniBuffer) -> Self {
        Self {
            mini_buffer,
            pending_outcome: None,
            query_changed: false,
        }
    }

    // Chunk: docs/chunks/focus_stack - Empty constructor for focus_layer() reporting
    /// Creates a new find focus target with a default empty mini buffer.
    ///
    /// This is used during the transition period where EditorState maintains
    /// both its own state fields and the focus_stack. The focus_stack entry
    /// only needs to provide the correct `layer()` result for rendering decisions.
    pub fn new_empty(font_metrics: crate::font::FontMetrics) -> Self {
        Self {
            mini_buffer: MiniBuffer::new(font_metrics),
            pending_outcome: None,
            query_changed: false,
        }
    }

    /// Returns the pending outcome, if any.
    pub fn pending_outcome(&self) -> Option<FindOutcome> {
        self.pending_outcome
    }

    /// Takes and returns the pending outcome, clearing it.
    pub fn take_outcome(&mut self) -> Option<FindOutcome> {
        self.pending_outcome.take()
    }

    /// Returns true if the query changed in the last key event.
    ///
    /// This is used to determine whether to re-run the live search.
    pub fn query_changed(&self) -> bool {
        self.query_changed
    }

    /// Clears the query_changed flag.
    pub fn clear_query_changed(&mut self) {
        self.query_changed = false;
    }

    /// Returns the current query content.
    pub fn query(&self) -> String {
        self.mini_buffer.content()
    }

    /// Returns a reference to the underlying mini buffer.
    pub fn mini_buffer(&self) -> &MiniBuffer {
        &self.mini_buffer
    }

    /// Returns a mutable reference to the underlying mini buffer.
    pub fn mini_buffer_mut(&mut self) -> &mut MiniBuffer {
        &mut self.mini_buffer
    }

    // Chunk: docs/chunks/minibuffer_input - Text input support for find strip
    /// Handles text input events (from IME, keyboard, paste).
    ///
    /// Inserts text into the query field. Sets `query_changed` to true
    /// if the content changed, allowing live search to trigger.
    pub fn handle_text_input(&mut self, text: &str) {
        let prev_content = self.mini_buffer.content();
        self.mini_buffer.handle_text_input(text);
        let new_content = self.mini_buffer.content();
        self.query_changed = prev_content != new_content;
    }
}

impl FocusTarget for FindFocusTarget {
    fn layer(&self) -> FocusLayer {
        FocusLayer::FindInFile
    }

    fn handle_key(&mut self, event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
        // Handle special keys first
        match &event.key {
            Key::Escape => {
                self.pending_outcome = Some(FindOutcome::Closed);
                return Handled::Yes;
            }
            Key::Return => {
                self.pending_outcome = Some(FindOutcome::FindNext);
                return Handled::Yes;
            }
            _ => {}
        }

        // For other keys, delegate to mini buffer
        let prev_content = self.mini_buffer.content();
        self.mini_buffer.handle_key(event);
        let new_content = self.mini_buffer.content();

        // Track if query changed
        self.query_changed = prev_content != new_content;

        // All events are handled by the find strip
        Handled::Yes
    }

    fn handle_scroll(&mut self, _delta: ScrollDelta, _ctx: &mut EditorContext) {
        // Find strip doesn't handle scroll events
        // Let them pass through to the buffer below
    }

    fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {
        // Mouse events in the find strip area are handled by EditorState
        // which has access to the geometry needed for proper hit-testing
    }
}

// =============================================================================
// Tests
// Chunk: docs/chunks/focus_stack - FindFocusTarget unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty_region::DirtyRegion;
    use lite_edit_buffer::DirtyLines;
    use crate::font::FontMetrics;
    use crate::input::Modifiers;
    use crate::viewport::Viewport;
    use lite_edit_buffer::TextBuffer;

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

    fn make_test_context<'a>(
        buffer: &'a mut TextBuffer,
        viewport: &'a mut Viewport,
        dirty_region: &'a mut DirtyRegion,
        dirty_lines: &'a mut DirtyLines,
    ) -> EditorContext<'a> {
        let metrics = test_font_metrics();
        EditorContext::new(buffer, viewport, dirty_region, dirty_lines, metrics, 400.0, 600.0)
    }

    fn char_key(ch: char) -> KeyEvent {
        KeyEvent::new(Key::Char(ch), Modifiers::default())
    }

    fn escape_key() -> KeyEvent {
        KeyEvent::new(Key::Escape, Modifiers::default())
    }

    fn return_key() -> KeyEvent {
        KeyEvent::new(Key::Return, Modifiers::default())
    }

    #[test]
    fn find_target_handles_escape() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut dirty_lines = DirtyLines::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty, &mut dirty_lines);

        let result = target.handle_key(escape_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(FindOutcome::Closed));
    }

    #[test]
    fn find_target_handles_return() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut dirty_lines = DirtyLines::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty, &mut dirty_lines);

        let result = target.handle_key(return_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(FindOutcome::FindNext));
    }

    #[test]
    fn find_target_handles_typing() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut dirty_lines = DirtyLines::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty, &mut dirty_lines);

        // Typing should be handled and update the query
        let result = target.handle_key(char_key('a'), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert!(target.query_changed());
        assert_eq!(target.query(), "a");
    }

    #[test]
    fn find_target_no_query_change_on_escape() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut dirty_lines = DirtyLines::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty, &mut dirty_lines);

        // Escape should not set query_changed
        target.handle_key(escape_key(), &mut ctx);

        assert!(!target.query_changed());
    }

    #[test]
    fn layer_is_find_in_file() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let target = FindFocusTarget::new(mini_buffer);
        assert_eq!(target.layer(), FocusLayer::FindInFile);
    }

    // =========================================================================
    // Chunk: docs/chunks/minibuffer_input - handle_text_input tests
    // =========================================================================

    #[test]
    fn test_handle_text_input_updates_query() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);

        target.handle_text_input("search");
        assert_eq!(target.query(), "search");
    }

    #[test]
    fn test_handle_text_input_sets_changed_flag() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);

        assert!(!target.query_changed());
        target.handle_text_input("a");
        assert!(target.query_changed());
    }

    #[test]
    fn test_handle_text_input_empty_string_no_change() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);

        target.handle_text_input("");
        // Empty input doesn't change content, so query_changed should be false
        assert!(!target.query_changed());
    }

    #[test]
    fn test_handle_text_input_unicode() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);

        target.handle_text_input("日本語");
        assert_eq!(target.query(), "日本語");
        assert!(target.query_changed());
    }

    #[test]
    fn test_handle_text_input_multiple_calls() {
        let mini_buffer = MiniBuffer::new(test_font_metrics());
        let mut target = FindFocusTarget::new(mini_buffer);

        target.handle_text_input("hel");
        target.clear_query_changed();
        target.handle_text_input("lo");
        assert_eq!(target.query(), "hello");
        assert!(target.query_changed());
    }
}
