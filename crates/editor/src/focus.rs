// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
//!
//! Focus target trait definition.
//!
//! Focus targets interpret their own input. The buffer's focus target owns chord
//! resolution and produces editor commands. Plugins can provide their own focus
//! targets (for minibuffers, completion menus, etc.) with entirely different
//! input models.
//!
//! This design (from the editor_core_architecture investigation) keeps the core
//! simple: it just delivers events to the active focus target. No global keymap,
//! no event × focus matrix, no god dispatch function.

use crate::context::EditorContext;
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};

/// Result of handling an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// The event was handled by this focus target
    Yes,
    /// The event was not handled (should propagate or be ignored)
    No,
}

/// A focus target that interprets input events.
///
/// Each focus target interprets its own input. The buffer's focus target
/// handles standard editing commands. Plugin-provided focus targets
/// (minibuffer, completion menu, file picker) handle input with their own logic.
///
/// The core defines this trait; implementations are either built-in (buffer editing)
/// or provided by plugins.
pub trait FocusTarget {
    /// Handle a keyboard event.
    ///
    /// The focus target should:
    /// 1. Interpret the key event (resolve any chords if applicable)
    /// 2. Execute the resulting command by mutating state through `ctx`
    /// 3. Return `Handled::Yes` if the event was consumed
    ///
    /// Mutations through `ctx` automatically accumulate dirty regions.
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled;

    // Chunk: docs/chunks/viewport_scrolling - Scroll event handling
    /// Handle a scroll event (trackpad or mouse wheel).
    ///
    /// The focus target should adjust the viewport based on the scroll delta.
    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext);

    /// Handle a mouse event (click, drag).
    ///
    /// The focus target should handle cursor placement, selection, etc.
    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext);
}
