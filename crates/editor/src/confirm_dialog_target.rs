// Chunk: docs/chunks/focus_stack - Confirm dialog focus target
//!
//! Confirm dialog focus target.
//!
//! This module provides [`ConfirmDialogFocusTarget`], a focus target that wraps
//! a [`ConfirmDialog`] and handles key events for confirmation dialogs.
//!
//! The confirm dialog target handles its own key events (tab, arrows, enter, escape)
//! and returns `Handled::Yes` for all of them. This means keys pressed while
//! the dialog is active won't propagate to lower layers.
//!
//! After handling, check `pending_outcome` to see what action the user took.

use crate::confirm_dialog::{ConfirmDialog, ConfirmOutcome};
use crate::context::EditorContext;
use crate::focus::{FocusLayer, FocusTarget, Handled};
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};

/// Focus target for confirmation dialogs.
///
/// This wraps a [`ConfirmDialog`] and provides the FocusTarget interface.
/// All key events are handled by this target when it's at the top of the
/// focus stack, preventing propagation to lower layers.
///
/// After dispatch, check `pending_outcome` to see if the user confirmed
/// or cancelled.
pub struct ConfirmDialogFocusTarget {
    /// The wrapped confirm dialog.
    dialog: ConfirmDialog,
    /// The outcome from the last key event, if any.
    pending_outcome: Option<ConfirmOutcome>,
}

impl ConfirmDialogFocusTarget {
    /// Creates a new confirm dialog focus target wrapping the given dialog.
    pub fn new(dialog: ConfirmDialog) -> Self {
        Self {
            dialog,
            pending_outcome: None,
        }
    }

    /// Returns the pending outcome, if any.
    pub fn pending_outcome(&self) -> Option<ConfirmOutcome> {
        self.pending_outcome
    }

    /// Takes and returns the pending outcome, clearing it.
    pub fn take_outcome(&mut self) -> Option<ConfirmOutcome> {
        self.pending_outcome.take()
    }

    /// Returns a reference to the underlying confirm dialog.
    pub fn dialog(&self) -> &ConfirmDialog {
        &self.dialog
    }

    /// Returns a mutable reference to the underlying confirm dialog.
    pub fn dialog_mut(&mut self) -> &mut ConfirmDialog {
        &mut self.dialog
    }
}

impl FocusTarget for ConfirmDialogFocusTarget {
    fn layer(&self) -> FocusLayer {
        FocusLayer::ConfirmDialog
    }

    fn handle_key(&mut self, event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
        let outcome = self.dialog.handle_key(&event);

        // Store non-Pending outcomes for EditorState to process
        match outcome {
            ConfirmOutcome::Pending => {
                // No action needed, dialog is still open
            }
            ConfirmOutcome::Confirmed | ConfirmOutcome::Cancelled => {
                self.pending_outcome = Some(outcome);
            }
        }

        // Confirm dialog always handles key events when active
        // This prevents keys from propagating to lower layers
        Handled::Yes
    }

    fn handle_scroll(&mut self, _delta: ScrollDelta, _ctx: &mut EditorContext) {
        // Confirm dialog doesn't handle scroll events
    }

    fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {
        // Mouse events in the confirm dialog are handled by EditorState
        // which has access to the geometry needed for button hit-testing
    }
}

// =============================================================================
// Tests
// Chunk: docs/chunks/focus_stack - ConfirmDialogFocusTarget unit tests
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

    fn tab_key() -> KeyEvent {
        KeyEvent::new(Key::Tab, Modifiers::default())
    }

    fn escape_key() -> KeyEvent {
        KeyEvent::new(Key::Escape, Modifiers::default())
    }

    fn return_key() -> KeyEvent {
        KeyEvent::new(Key::Return, Modifiers::default())
    }

    #[test]
    fn confirm_target_handles_tab() {
        let dialog = ConfirmDialog::new("Test prompt?");
        let mut target = ConfirmDialogFocusTarget::new(dialog);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // Tab should be handled without producing an outcome (just toggles button)
        let result = target.handle_key(tab_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), None); // Still pending
    }

    #[test]
    fn confirm_target_handles_escape() {
        let dialog = ConfirmDialog::new("Test prompt?");
        let mut target = ConfirmDialogFocusTarget::new(dialog);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(escape_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(ConfirmOutcome::Cancelled));
    }

    #[test]
    fn confirm_target_handles_return_with_cancel_selected() {
        // Default selection is Cancel
        let dialog = ConfirmDialog::new("Test prompt?");
        let mut target = ConfirmDialogFocusTarget::new(dialog);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // Return with Cancel selected should produce Cancelled
        let result = target.handle_key(return_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(ConfirmOutcome::Cancelled));
    }

    #[test]
    fn confirm_target_handles_return_with_abandon_selected() {
        let dialog = ConfirmDialog::new("Test prompt?");
        let mut target = ConfirmDialogFocusTarget::new(dialog);
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // First tab to select Abandon
        target.handle_key(tab_key(), &mut ctx);
        target.take_outcome(); // Clear any pending (should be None)

        // Now Return should confirm
        let result = target.handle_key(return_key(), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_outcome(), Some(ConfirmOutcome::Confirmed));
    }

    #[test]
    fn layer_is_confirm_dialog() {
        let dialog = ConfirmDialog::new("Test prompt?");
        let target = ConfirmDialogFocusTarget::new(dialog);
        assert_eq!(target.layer(), FocusLayer::ConfirmDialog);
    }
}
