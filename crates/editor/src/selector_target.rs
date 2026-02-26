// Chunk: docs/chunks/focus_stack - Selector focus target
//!
//! Selector focus target.
//!
//! This module provides [`SelectorFocusTarget`], a focus target that wraps
//! a [`SelectorWidget`] and handles key events for selector overlays like
//! the file picker and command palette.
//!
//! The selector target handles its own key events (arrows, typing, enter, escape)
//! and returns `Handled::Yes` for all of them. This means keys pressed while
//! the selector is active won't propagate to lower layers (like the buffer).
//!
//! After handling, check `pending_outcome` to see what action the user took.

use crate::context::EditorContext;
use crate::focus::{FocusLayer, FocusTarget, Handled};
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};
use crate::selector::{SelectorOutcome, SelectorWidget};

/// Focus target for selector overlays (file picker, command palette, etc.).
///
/// This wraps a [`SelectorWidget`] and provides the FocusTarget interface.
/// All key events are handled by this target when it's at the top of the
/// focus stack, preventing propagation to lower layers.
///
/// After dispatch, check `pending_outcome` to see if the user confirmed
/// or cancelled the selection.
pub struct SelectorFocusTarget {
    /// The wrapped selector widget.
    widget: SelectorWidget,
    /// The outcome from the last key event, if any.
    pending_outcome: Option<SelectorOutcome>,
}

impl SelectorFocusTarget {
    /// Creates a new selector focus target wrapping the given widget.
    pub fn new(widget: SelectorWidget) -> Self {
        Self {
            widget,
            pending_outcome: None,
        }
    }

    /// Creates a new selector focus target with an empty widget.
    pub fn new_empty() -> Self {
        Self::new(SelectorWidget::new())
    }

    /// Returns the pending outcome, if any.
    pub fn pending_outcome(&self) -> Option<SelectorOutcome> {
        self.pending_outcome
    }

    /// Takes and returns the pending outcome, clearing it.
    pub fn take_outcome(&mut self) -> Option<SelectorOutcome> {
        self.pending_outcome.take()
    }

    /// Returns a reference to the underlying selector widget.
    pub fn widget(&self) -> &SelectorWidget {
        &self.widget
    }

    /// Returns a mutable reference to the underlying selector widget.
    pub fn widget_mut(&mut self) -> &mut SelectorWidget {
        &mut self.widget
    }
}

impl FocusTarget for SelectorFocusTarget {
    fn layer(&self) -> FocusLayer {
        FocusLayer::Selector
    }

    fn handle_key(&mut self, event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
        let outcome = self.widget.handle_key(&event);

        // Store non-Pending outcomes for EditorState to process
        match outcome {
            SelectorOutcome::Pending => {
                // No action needed, selector is still open
            }
            SelectorOutcome::Confirmed(_) | SelectorOutcome::Cancelled => {
                self.pending_outcome = Some(outcome);
            }
        }

        // Selector always handles key events when active
        // This prevents keys from propagating to lower layers
        Handled::Yes
    }

    fn handle_scroll(&mut self, delta: ScrollDelta, _ctx: &mut EditorContext) {
        // Forward scroll to the selector widget
        // The delta.dy is the vertical scroll amount in pixels
        self.widget.handle_scroll(delta.dy);
    }

    fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {
        // Mouse events require geometry information (item height, list origin)
        // that is computed by EditorState. The mouse handling for selectors
        // is done in EditorState::handle_mouse which has access to this geometry.
        //
        // This method is a no-op - mouse events for selectors are handled
        // directly by EditorState rather than through the focus stack.
    }
}

// =============================================================================
// Tests
// Chunk: docs/chunks/focus_stack - SelectorFocusTarget unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty_region::DirtyRegion;
    use crate::font::FontMetrics;
    use crate::input::{Key, Modifiers};
    use crate::viewport::Viewport;
    use lite_edit_buffer::TextBuffer;

    fn make_test_context<'a>(
        buffer: &'a mut TextBuffer,
        viewport: &'a mut Viewport,
        dirty_region: &'a mut DirtyRegion,
    ) -> EditorContext<'a> {
        let metrics = FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        };
        EditorContext::new(buffer, viewport, dirty_region, metrics, 400.0, 600.0)
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

    fn arrow_up() -> KeyEvent {
        KeyEvent::new(Key::Up, Modifiers::default())
    }

    #[test]
    fn selector_target_handles_escape() {
        let mut target = SelectorFocusTarget::new_empty();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(escape_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(SelectorOutcome::Cancelled));
    }

    #[test]
    fn selector_target_handles_return() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["foo.rs".into(), "bar.rs".into()]);

        let mut target = SelectorFocusTarget::new(widget);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(return_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(SelectorOutcome::Confirmed(0)));
    }

    #[test]
    fn selector_target_handles_arrows() {
        let mut widget = SelectorWidget::new();
        widget.set_items(vec!["foo.rs".into(), "bar.rs".into()]);

        let mut target = SelectorFocusTarget::new(widget);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // Arrow keys should be handled without producing an outcome
        let result = target.handle_key(arrow_up(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), None); // Still pending
    }

    #[test]
    fn selector_target_handles_typing() {
        let mut target = SelectorFocusTarget::new_empty();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // Typing should be handled and update the query
        let result = target.handle_key(char_key('a'), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), None); // Still pending
        assert_eq!(target.widget().query(), "a");
    }

    #[test]
    fn layer_is_selector() {
        let target = SelectorFocusTarget::new_empty();
        assert_eq!(target.layer(), FocusLayer::Selector);
    }
}
