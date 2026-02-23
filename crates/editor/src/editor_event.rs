// Chunk: docs/chunks/pty_wakeup_reentrant - Unified event queue architecture
//! Editor event types for the unified event queue.
//!
//! All event sources (keyboard, mouse, scroll, PTY wakeup, cursor blink, window resize)
//! send their events through a single channel rather than each holding a clone of
//! `Rc<RefCell<EditorController>>`. This eliminates the reentrant borrow panics that
//! occur when multiple event sources try to borrow the controller simultaneously.
//!
//! The event queue is drained by a single callback that owns the controller directly
//! (no `Rc`, no `RefCell`), ensuring exclusive access during event processing.

use crate::input::{KeyEvent, MouseEvent, ScrollDelta};

/// Unified event type for all editor events.
///
/// All event sources send one of these variants to the event channel. The drain
/// loop processes events one at a time, ensuring the controller is never borrowed
/// by multiple callbacks simultaneously.
#[derive(Debug)]
pub enum EditorEvent {
    /// A keyboard event (key down)
    Key(KeyEvent),

    /// A mouse event (click, drag, release)
    Mouse(MouseEvent),

    /// A scroll event (trackpad or mouse wheel)
    Scroll(ScrollDelta),

    /// PTY data is available - poll all agents/terminals
    ///
    /// This replaces the `dispatch_async` + `PtyWakeup::signal` pattern.
    /// The PTY reader thread sends this when data arrives.
    PtyWakeup,

    /// Cursor blink timer fired - toggle cursor visibility
    CursorBlink,

    /// Window was resized or moved between displays
    ///
    /// This covers both `windowDidResize:` and `windowDidChangeBackingProperties:`.
    Resize,
}

impl EditorEvent {
    /// Returns true if this is a user input event (key, mouse, scroll).
    ///
    /// Used for resetting cursor blink state on user activity.
    pub fn is_user_input(&self) -> bool {
        matches!(self, EditorEvent::Key(_) | EditorEvent::Mouse(_) | EditorEvent::Scroll(_))
    }

    /// Returns true if this is a key event.
    pub fn is_key(&self) -> bool {
        matches!(self, EditorEvent::Key(_))
    }
}
