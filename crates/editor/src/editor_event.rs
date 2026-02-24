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

    /// Files were dropped onto the view
    ///
    /// Contains the list of file paths (as UTF-8 strings) that were dropped.
    /// The paths are absolute and need shell escaping before insertion.
    // Chunk: docs/chunks/dragdrop_file_paste - File drop event for drag-and-drop
    FileDrop(Vec<String>),
}

impl EditorEvent {
    /// Returns true if this is a user input event (key, mouse, scroll, file drop).
    ///
    /// Used for resetting cursor blink state on user activity.
    pub fn is_user_input(&self) -> bool {
        matches!(
            self,
            EditorEvent::Key(_)
                | EditorEvent::Mouse(_)
                | EditorEvent::Scroll(_)
                | EditorEvent::FileDrop(_)
        )
    }

    // Chunk: docs/chunks/terminal_flood_starvation - Input-first event partitioning
    /// Returns true if this event should be processed before PTY wakeup events.
    ///
    /// Priority events include all user input events plus Resize (window resize
    /// should be responsive). CursorBlink is NOT included since it's cosmetic.
    /// This ensures input latency is bounded by the cost of processing priority
    /// events, not by accumulated terminal output.
    pub fn is_priority_event(&self) -> bool {
        matches!(
            self,
            EditorEvent::Key(_)
                | EditorEvent::Mouse(_)
                | EditorEvent::Scroll(_)
                | EditorEvent::FileDrop(_)
                | EditorEvent::Resize
        )
    }

    /// Returns true if this is a key event.
    pub fn is_key(&self) -> bool {
        matches!(self, EditorEvent::Key(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lite_edit_input::{KeyEvent, MouseEvent, MouseEventKind, Modifiers, ScrollDelta};

    // Chunk: docs/chunks/terminal_flood_starvation - Tests for is_priority_event

    #[test]
    fn test_key_is_priority() {
        let event = EditorEvent::Key(KeyEvent::char('a'));
        assert!(event.is_priority_event());
    }

    #[test]
    fn test_mouse_is_priority() {
        let event = EditorEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down,
            position: (0.0, 0.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        assert!(event.is_priority_event());
    }

    #[test]
    fn test_scroll_is_priority() {
        let event = EditorEvent::Scroll(ScrollDelta {
            dx: 0.0,
            dy: 10.0,
            mouse_position: None,
        });
        assert!(event.is_priority_event());
    }

    #[test]
    fn test_file_drop_is_priority() {
        let event = EditorEvent::FileDrop(vec!["/path/to/file.txt".to_string()]);
        assert!(event.is_priority_event());
    }

    #[test]
    fn test_resize_is_priority() {
        let event = EditorEvent::Resize;
        assert!(event.is_priority_event());
    }

    #[test]
    fn test_pty_wakeup_is_not_priority() {
        let event = EditorEvent::PtyWakeup;
        assert!(!event.is_priority_event());
    }

    #[test]
    fn test_cursor_blink_is_not_priority() {
        let event = EditorEvent::CursorBlink;
        assert!(!event.is_priority_event());
    }

    #[test]
    fn test_priority_events_superset_of_user_input() {
        // All user input events should also be priority events
        let user_input_events = vec![
            EditorEvent::Key(KeyEvent::char('x')),
            EditorEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                position: (0.0, 0.0),
                modifiers: Modifiers::default(),
                click_count: 0,
            }),
            EditorEvent::Scroll(ScrollDelta {
                dx: 0.0,
                dy: 0.0,
                mouse_position: None,
            }),
            EditorEvent::FileDrop(vec![]),
        ];

        for event in user_input_events {
            assert!(
                event.is_user_input(),
                "Event {:?} should be user input",
                event
            );
            assert!(
                event.is_priority_event(),
                "User input event {:?} should also be priority",
                event
            );
        }
    }
}
