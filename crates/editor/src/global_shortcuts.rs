// Chunk: docs/chunks/focus_stack - Global shortcut focus target
//!
//! Global shortcut focus target.
//!
//! This module provides [`GlobalShortcutTarget`], a focus target that handles
//! application-level keyboard shortcuts like Cmd+Q (quit), Cmd+S (save),
//! Cmd+P (file picker), etc.
//!
//! The global shortcut target sits at the bottom of the focus stack. Events
//! that aren't handled by higher layers (overlays, buffer) fall through to
//! this target. This allows overlays to handle their own keys while still
//! allowing global shortcuts to work.
//!
//! # Design
//!
//! The target doesn't own any state - it only interprets key events and
//! records what action should be taken. After `dispatch_key` returns,
//! EditorState checks the target's `pending_action` field and executes
//! the appropriate operation.

use crate::context::EditorContext;
use crate::focus::{FocusLayer, FocusTarget, Handled};
use crate::input::{Key, KeyEvent, MouseEvent, ScrollDelta};
use crate::pane_layout::Direction;

/// Actions that can be triggered by global keyboard shortcuts.
///
/// After dispatch, EditorState checks if there's a pending action
/// and executes it. This decouples the key recognition from the
/// state mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalAction {
    /// Quit the application (Cmd+Q)
    Quit,
    /// Toggle file picker (Cmd+P)
    ToggleFilePicker,
    /// Save the current file (Cmd+S)
    Save,
    /// Open find-in-file strip (Cmd+F)
    Find,
    /// Create a new workspace (Cmd+N)
    NewWorkspace,
    /// Open system file picker (Cmd+O)
    OpenFilePicker,
    /// Close the active tab (Cmd+W)
    CloseTab,
    /// Close the active workspace (Cmd+Shift+W)
    CloseWorkspace,
    /// Switch to next tab (Cmd+Shift+])
    NextTab,
    /// Switch to previous tab (Cmd+Shift+[)
    PrevTab,
    /// Switch to next workspace (Cmd+])
    NextWorkspace,
    /// Switch to previous workspace (Cmd+[)
    PrevWorkspace,
    /// Create a new tab (Cmd+T)
    NewTab,
    /// Create a new terminal tab (Cmd+Shift+T)
    NewTerminalTab,
    /// Switch to workspace by number (Cmd+1..9)
    SwitchWorkspace(usize),
    /// Move active tab in direction (Cmd+Shift+Arrow)
    MoveTab(Direction),
    /// Switch focus to pane in direction (Cmd+Option+Arrow)
    SwitchFocus(Direction),
}

/// Global keyboard shortcut focus target.
///
/// This target handles application-level shortcuts that should work
/// regardless of which overlay is active. It sits at the bottom of
/// the focus stack.
///
/// After dispatch, check `pending_action` to see if an action was
/// triggered, then call `take_action` to clear it.
pub struct GlobalShortcutTarget {
    /// The action triggered by the last key event, if any.
    ///
    /// This is set during `handle_key` and should be consumed by
    /// EditorState after dispatch completes.
    pending_action: Option<GlobalAction>,
}

impl Default for GlobalShortcutTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalShortcutTarget {
    /// Creates a new global shortcut target.
    pub fn new() -> Self {
        Self {
            pending_action: None,
        }
    }

    /// Returns the pending action, if any.
    pub fn pending_action(&self) -> Option<GlobalAction> {
        self.pending_action
    }

    /// Takes and returns the pending action, clearing it.
    ///
    /// Call this after dispatch to consume the action.
    pub fn take_action(&mut self) -> Option<GlobalAction> {
        self.pending_action.take()
    }

    /// Resolves a key event to a global action.
    ///
    /// Returns `Some(action)` if the key event is a global shortcut,
    /// `None` otherwise.
    fn resolve_action(&self, event: &KeyEvent) -> Option<GlobalAction> {
        // Only handle Cmd+key shortcuts (without Ctrl)
        if !event.modifiers.command || event.modifiers.control {
            return None;
        }

        match &event.key {
            Key::Char('q') => Some(GlobalAction::Quit),
            Key::Char('p') => Some(GlobalAction::ToggleFilePicker),
            Key::Char('s') => Some(GlobalAction::Save),
            Key::Char('f') => Some(GlobalAction::Find),
            Key::Char('n') if !event.modifiers.shift => Some(GlobalAction::NewWorkspace),
            Key::Char('o') => Some(GlobalAction::OpenFilePicker),
            Key::Char('w') if event.modifiers.shift => Some(GlobalAction::CloseWorkspace),
            Key::Char('w') => Some(GlobalAction::CloseTab),
            Key::Char(']') if event.modifiers.shift => Some(GlobalAction::NextTab),
            Key::Char('[') if event.modifiers.shift => Some(GlobalAction::PrevTab),
            Key::Char(']') => Some(GlobalAction::NextWorkspace),
            Key::Char('[') => Some(GlobalAction::PrevWorkspace),
            Key::Char('t') if event.modifiers.shift => Some(GlobalAction::NewTerminalTab),
            Key::Char('t') => Some(GlobalAction::NewTab),
            Key::Char(c) if !event.modifiers.shift => {
                // Cmd+1..9 for workspace switching
                if let Some(digit) = c.to_digit(10) {
                    if digit >= 1 && digit <= 9 {
                        let idx = (digit - 1) as usize;
                        return Some(GlobalAction::SwitchWorkspace(idx));
                    }
                }
                None
            }
            // Cmd+Shift+Arrow for tab movement
            Key::Right if event.modifiers.shift && !event.modifiers.option => {
                Some(GlobalAction::MoveTab(Direction::Right))
            }
            Key::Left if event.modifiers.shift && !event.modifiers.option => {
                Some(GlobalAction::MoveTab(Direction::Left))
            }
            Key::Down if event.modifiers.shift && !event.modifiers.option => {
                Some(GlobalAction::MoveTab(Direction::Down))
            }
            Key::Up if event.modifiers.shift && !event.modifiers.option => {
                Some(GlobalAction::MoveTab(Direction::Up))
            }
            // Cmd+Option+Arrow for focus switching
            Key::Right if event.modifiers.option && !event.modifiers.shift => {
                Some(GlobalAction::SwitchFocus(Direction::Right))
            }
            Key::Left if event.modifiers.option && !event.modifiers.shift => {
                Some(GlobalAction::SwitchFocus(Direction::Left))
            }
            Key::Down if event.modifiers.option && !event.modifiers.shift => {
                Some(GlobalAction::SwitchFocus(Direction::Down))
            }
            Key::Up if event.modifiers.option && !event.modifiers.shift => {
                Some(GlobalAction::SwitchFocus(Direction::Up))
            }
            _ => None,
        }
    }
}

impl FocusTarget for GlobalShortcutTarget {
    fn layer(&self) -> FocusLayer {
        FocusLayer::GlobalShortcuts
    }

    fn handle_key(&mut self, event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
        if let Some(action) = self.resolve_action(&event) {
            self.pending_action = Some(action);
            Handled::Yes
        } else {
            Handled::No
        }
    }

    fn handle_scroll(&mut self, _delta: ScrollDelta, _ctx: &mut EditorContext) {
        // Global shortcuts don't handle scroll events
    }

    fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {
        // Global shortcuts don't handle mouse events
    }
}

// =============================================================================
// Tests
// Chunk: docs/chunks/focus_stack - GlobalShortcutTarget unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty_region::DirtyRegion;
    use crate::font::FontMetrics;
    use crate::input::Modifiers;
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

    fn cmd_key(ch: char) -> KeyEvent {
        KeyEvent::new(
            Key::Char(ch),
            Modifiers {
                command: true,
                ..Default::default()
            },
        )
    }

    fn plain_key(ch: char) -> KeyEvent {
        KeyEvent::new(Key::Char(ch), Modifiers::default())
    }

    #[test]
    fn global_target_handles_cmd_q() {
        let mut target = GlobalShortcutTarget::new();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(cmd_key('q'), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_action(), Some(GlobalAction::Quit));
    }

    #[test]
    fn global_target_handles_cmd_s() {
        let mut target = GlobalShortcutTarget::new();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(cmd_key('s'), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_action(), Some(GlobalAction::Save));
    }

    #[test]
    fn global_target_ignores_plain_keys() {
        let mut target = GlobalShortcutTarget::new();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(plain_key('a'), &mut ctx);

        assert_eq!(result, Handled::No);
        assert_eq!(target.take_action(), None);
    }

    #[test]
    fn global_target_ignores_unmodified_q() {
        let mut target = GlobalShortcutTarget::new();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(plain_key('q'), &mut ctx);

        assert_eq!(result, Handled::No);
        assert_eq!(target.take_action(), None);
    }

    #[test]
    fn global_target_handles_cmd_p() {
        let mut target = GlobalShortcutTarget::new();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = target.handle_key(cmd_key('p'), &mut ctx);

        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_action(), Some(GlobalAction::ToggleFilePicker));
    }

    #[test]
    fn global_target_handles_cmd_1_through_9() {
        let mut target = GlobalShortcutTarget::new();
        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // Cmd+1 switches to workspace 0
        let result = target.handle_key(cmd_key('1'), &mut ctx);
        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_action(), Some(GlobalAction::SwitchWorkspace(0)));

        // Cmd+9 switches to workspace 8
        let result = target.handle_key(cmd_key('9'), &mut ctx);
        assert_eq!(result, Handled::Yes);
        assert_eq!(target.take_action(), Some(GlobalAction::SwitchWorkspace(8)));
    }

    #[test]
    fn layer_is_global_shortcuts() {
        let target = GlobalShortcutTarget::new();
        assert_eq!(target.layer(), FocusLayer::GlobalShortcuts);
    }
}
