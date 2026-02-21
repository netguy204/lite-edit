// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
//!
//! Input event types for keyboard, mouse, and scroll handling.
//!
//! These types abstract over macOS NSEvent details and provide a clean
//! Rust-native interface for input handling.

/// A keyboard event.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEvent {
    /// The key that was pressed
    pub key: Key,
    /// Modifier keys held during the event
    pub modifiers: Modifiers,
}

impl KeyEvent {
    /// Creates a new KeyEvent with the given key and modifiers.
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Creates a KeyEvent for a single character with no modifiers.
    pub fn char(ch: char) -> Self {
        Self {
            key: Key::Char(ch),
            modifiers: Modifiers::default(),
        }
    }

    /// Creates a KeyEvent for a single character with shift held.
    pub fn char_shifted(ch: char) -> Self {
        Self {
            key: Key::Char(ch),
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        }
    }
}

/// Modifier keys that can be held during a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift key
    pub shift: bool,
    /// Command key (Cmd/⌘)
    pub command: bool,
    /// Option key (Alt/⌥)
    pub option: bool,
    /// Control key (Ctrl/⌃)
    pub control: bool,
}

impl Modifiers {
    /// Returns true if no modifier keys are held.
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.command && !self.option && !self.control
    }

    /// Returns true if only shift is held (for uppercase letters).
    pub fn is_shift_only(&self) -> bool {
        self.shift && !self.command && !self.option && !self.control
    }
}

/// Keys that can be pressed.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// A printable character (already accounts for shift state)
    Char(char),
    /// Backspace / Delete backward
    Backspace,
    /// Forward delete
    Delete,
    /// Return / Enter
    Return,
    /// Left arrow
    Left,
    /// Right arrow
    Right,
    /// Up arrow
    Up,
    /// Down arrow
    Down,
    /// Home key
    Home,
    /// End key
    End,
    /// Tab key
    Tab,
    /// Escape key
    Escape,
    /// Page Up
    PageUp,
    /// Page Down
    PageDown,
}

/// Scroll delta from trackpad or mouse wheel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollDelta {
    /// Horizontal scroll amount (positive = right)
    pub dx: f64,
    /// Vertical scroll amount (positive = down)
    pub dy: f64,
}

impl ScrollDelta {
    pub fn new(dx: f64, dy: f64) -> Self {
        Self { dx, dy }
    }
}

/// A mouse event.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseEvent {
    /// The type of mouse event
    pub kind: MouseEventKind,
    /// Position in view coordinates (pixels from top-left)
    pub position: (f64, f64),
    /// Modifier keys held during the event
    pub modifiers: Modifiers,
}

/// Kind of mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    /// Mouse button pressed
    Down,
    /// Mouse button released
    Up,
    /// Mouse moved (with button held for drag)
    Moved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_char() {
        let event = KeyEvent::char('a');
        assert_eq!(event.key, Key::Char('a'));
        assert!(event.modifiers.is_empty());
    }

    #[test]
    fn test_key_event_char_shifted() {
        let event = KeyEvent::char_shifted('A');
        assert_eq!(event.key, Key::Char('A'));
        assert!(event.modifiers.is_shift_only());
    }

    #[test]
    fn test_modifiers_is_empty() {
        let empty = Modifiers::default();
        assert!(empty.is_empty());

        let with_shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert!(!with_shift.is_empty());
    }

    #[test]
    fn test_modifiers_is_shift_only() {
        let shift_only = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert!(shift_only.is_shift_only());

        let shift_and_cmd = Modifiers {
            shift: true,
            command: true,
            ..Default::default()
        };
        assert!(!shift_and_cmd.is_shift_only());
    }
}
