// Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
//!
//! Input encoder for terminal escape sequences.
//!
//! This module translates keyboard and mouse events into the escape sequences
//! expected by terminal applications. The encoding depends on the terminal's
//! active modes (APP_CURSOR, BRACKETED_PASTE, SGR_MOUSE, etc.).

use alacritty_terminal::term::TermMode;
use lite_edit_input::{Key, KeyEvent, Modifiers, MouseEvent, MouseEventKind};

/// Encodes input events into terminal escape sequences.
///
/// This is a stateless encoder - all mode information is passed in with each call.
pub struct InputEncoder;

impl InputEncoder {
    /// Encode a key event given active terminal modes.
    ///
    /// Returns an empty vector if the key cannot be encoded.
    pub fn encode_key(event: &KeyEvent, modes: TermMode) -> Vec<u8> {
        // Handle basic character input first
        if let Key::Char(ch) = event.key {
            return Self::encode_char(ch, &event.modifiers);
        }

        // Handle special keys
        Self::encode_special_key(&event.key, &event.modifiers, modes)
    }

    /// Encode a printable character, handling modifiers.
    fn encode_char(ch: char, modifiers: &Modifiers) -> Vec<u8> {
        // Ctrl+key produces control characters
        if modifiers.control {
            return Self::encode_ctrl_char(ch);
        }

        // Alt/Option sends ESC prefix on most terminals
        if modifiers.option {
            let mut result = vec![0x1b]; // ESC prefix
            let mut buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut buf);
            result.extend_from_slice(encoded.as_bytes());
            return result;
        }

        // Regular character - encode as UTF-8
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        encoded.as_bytes().to_vec()
    }

    /// Encode Ctrl+key combinations.
    ///
    /// Ctrl+A through Ctrl+Z produce 0x01 through 0x1A.
    /// Ctrl+[ produces ESC (0x1B), Ctrl+\ produces 0x1C, etc.
    fn encode_ctrl_char(ch: char) -> Vec<u8> {
        let control_code = match ch.to_ascii_lowercase() {
            'a'..='z' => Some((ch.to_ascii_lowercase() as u8) - b'a' + 1),
            '[' => Some(0x1b), // ESC
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' => Some(0x1f),
            '@' => Some(0x00), // NUL
            ' ' => Some(0x00), // Ctrl+Space also produces NUL
            _ => None,
        };

        match control_code {
            Some(code) => vec![code],
            None => {
                // Fall back to regular character if no control mapping
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                encoded.as_bytes().to_vec()
            }
        }
    }

    /// Encode special (non-character) keys.
    fn encode_special_key(key: &Key, modifiers: &Modifiers, modes: TermMode) -> Vec<u8> {
        match key {
            // Basic control keys
            Key::Return => vec![0x0d], // CR
            Key::Tab => vec![0x09],    // HT
            Key::Escape => vec![0x1b], // ESC
            // Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace sends ESC+DEL
            Key::Backspace => {
                if modifiers.option {
                    vec![0x1b, 0x7f] // ESC + DEL for backward word delete
                } else {
                    vec![0x7f] // DEL (most modern terminals)
                }
            }

            // Arrow keys - mode dependent
            Key::Up => Self::encode_arrow(b'A', modifiers, modes),
            Key::Down => Self::encode_arrow(b'B', modifiers, modes),
            Key::Right => Self::encode_arrow(b'C', modifiers, modes),
            Key::Left => Self::encode_arrow(b'D', modifiers, modes),

            // Navigation keys
            Key::Home => Self::encode_nav_key(1, b'H', modifiers, modes),
            Key::End => Self::encode_nav_key(4, b'F', modifiers, modes),
            Key::Insert => Self::encode_tilde_key(2, modifiers),
            Key::Delete => Self::encode_tilde_key(3, modifiers),
            Key::PageUp => Self::encode_tilde_key(5, modifiers),
            Key::PageDown => Self::encode_tilde_key(6, modifiers),

            // Function keys
            Key::F1 => Self::encode_f1_f4(b'P', modifiers),
            Key::F2 => Self::encode_f1_f4(b'Q', modifiers),
            Key::F3 => Self::encode_f1_f4(b'R', modifiers),
            Key::F4 => Self::encode_f1_f4(b'S', modifiers),
            Key::F5 => Self::encode_f5_plus(15, modifiers),
            Key::F6 => Self::encode_f5_plus(17, modifiers),
            Key::F7 => Self::encode_f5_plus(18, modifiers),
            Key::F8 => Self::encode_f5_plus(19, modifiers),
            Key::F9 => Self::encode_f5_plus(20, modifiers),
            Key::F10 => Self::encode_f5_plus(21, modifiers),
            Key::F11 => Self::encode_f5_plus(23, modifiers),
            Key::F12 => Self::encode_f5_plus(24, modifiers),

            Key::Char(_) => unreachable!("Char handled above"),
        }
    }

    /// Encode arrow keys with mode and modifier awareness.
    ///
    /// In APP_CURSOR mode: ESC O A/B/C/D
    /// In normal mode: ESC [ A/B/C/D
    /// With modifiers: ESC [ 1 ; modifier A/B/C/D
    fn encode_arrow(direction: u8, modifiers: &Modifiers, modes: TermMode) -> Vec<u8> {
        let modifier_code = Self::modifier_code(modifiers);

        if modifier_code > 1 {
            // With modifiers, always use CSI format: ESC [ 1 ; modifier direction
            format!("\x1b[1;{}{}", modifier_code, direction as char)
                .into_bytes()
        } else if modes.contains(TermMode::APP_CURSOR) {
            // Application cursor mode: ESC O direction
            vec![0x1b, b'O', direction]
        } else {
            // Normal mode: ESC [ direction
            vec![0x1b, b'[', direction]
        }
    }

    /// Encode Home/End keys with mode awareness.
    ///
    /// Home: ESC [ H or ESC [ 1 ~ or ESC O H (APP_CURSOR)
    /// End: ESC [ F or ESC [ 4 ~ or ESC O F (APP_CURSOR)
    fn encode_nav_key(_tilde_num: u8, app_char: u8, modifiers: &Modifiers, modes: TermMode) -> Vec<u8> {
        let modifier_code = Self::modifier_code(modifiers);

        if modifier_code > 1 {
            // With modifiers: ESC [ 1 ; modifier H/F
            format!("\x1b[1;{}{}", modifier_code, app_char as char)
                .into_bytes()
        } else if modes.contains(TermMode::APP_CURSOR) {
            // Application cursor mode: ESC O H/F
            vec![0x1b, b'O', app_char]
        } else {
            // Normal mode: ESC [ H/F
            vec![0x1b, b'[', app_char]
        }
    }

    /// Encode keys that use the tilde format: ESC [ number ~
    ///
    /// Insert (2), Delete (3), PageUp (5), PageDown (6)
    fn encode_tilde_key(num: u8, modifiers: &Modifiers) -> Vec<u8> {
        let modifier_code = Self::modifier_code(modifiers);

        if modifier_code > 1 {
            format!("\x1b[{};{}~", num, modifier_code).into_bytes()
        } else {
            format!("\x1b[{}~", num).into_bytes()
        }
    }

    /// Encode F1-F4 using SS3 sequences: ESC O P/Q/R/S
    ///
    /// With modifiers, uses CSI format: ESC [ 1 ; modifier P/Q/R/S
    fn encode_f1_f4(char_code: u8, modifiers: &Modifiers) -> Vec<u8> {
        let modifier_code = Self::modifier_code(modifiers);

        if modifier_code > 1 {
            format!("\x1b[1;{}{}", modifier_code, char_code as char)
                .into_bytes()
        } else {
            vec![0x1b, b'O', char_code]
        }
    }

    /// Encode F5-F12 using tilde format: ESC [ number ~
    ///
    /// F5=15, F6=17, F7=18, F8=19, F9=20, F10=21, F11=23, F12=24
    /// (Note the gaps at 16 and 22 - historical VT220 legacy)
    fn encode_f5_plus(num: u8, modifiers: &Modifiers) -> Vec<u8> {
        Self::encode_tilde_key(num, modifiers)
    }

    /// Calculate the xterm modifier code.
    ///
    /// Modifier code = 1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (ctrl ? 4 : 0)
    ///
    /// Returns 1 for no modifiers (which means "don't include modifier").
    fn modifier_code(modifiers: &Modifiers) -> u8 {
        let mut code: u8 = 1;
        if modifiers.shift {
            code += 1;
        }
        if modifiers.option {
            // Option = Alt
            code += 2;
        }
        if modifiers.control {
            code += 4;
        }
        code
    }

    /// Encode pasted text, respecting bracketed paste mode.
    ///
    /// When BRACKETED_PASTE mode is active, wraps text in:
    /// ESC [ 200 ~ ... ESC [ 201 ~
    pub fn encode_paste(text: &str, modes: TermMode) -> Vec<u8> {
        if modes.contains(TermMode::BRACKETED_PASTE) {
            let mut result = b"\x1b[200~".to_vec();
            result.extend_from_slice(text.as_bytes());
            result.extend_from_slice(b"\x1b[201~");
            result
        } else {
            text.as_bytes().to_vec()
        }
    }

    /// Encode a mouse event given active terminal modes.
    ///
    /// Returns an empty vector if mouse reporting is not active.
    pub fn encode_mouse(event: &MouseEvent, col: usize, row: usize, modes: TermMode) -> Vec<u8> {
        // Check if any mouse mode is active
        if !modes.intersects(TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) {
            return Vec::new();
        }

        // Determine button code
        let button = Self::mouse_button_code(event, &event.modifiers);

        // Use SGR encoding if available, otherwise fall back to X10/normal
        if modes.contains(TermMode::SGR_MOUSE) {
            Self::encode_mouse_sgr(button, col, row, event.kind)
        } else {
            Self::encode_mouse_legacy(button, col, row)
        }
    }

    /// Calculate the mouse button code.
    ///
    /// Button encoding:
    /// - 0 = left click
    /// - 1 = middle click
    /// - 2 = right click
    /// - 3 = release (in normal mode)
    /// - 64 = scroll up
    /// - 65 = scroll down
    ///
    /// Modifier bits are added:
    /// - 4 = Shift
    /// - 8 = Alt/Option
    /// - 16 = Ctrl
    /// - 32 = motion (for drag events)
    fn mouse_button_code(event: &MouseEvent, modifiers: &Modifiers) -> u8 {
        // Base button code
        // For now, assume left button (0). In a real implementation,
        // we'd need to track which button was pressed.
        let mut button: u8 = match event.kind {
            MouseEventKind::Down => 0,      // Left button press
            MouseEventKind::Up => 3,        // Release
            MouseEventKind::Moved => 32,    // Motion (with button 0 held)
        };

        // Add modifier bits
        if modifiers.shift {
            button |= 4;
        }
        if modifiers.option {
            button |= 8;
        }
        if modifiers.control {
            button |= 16;
        }

        button
    }

    /// Encode mouse event using SGR format.
    ///
    /// Format: ESC [ < button ; x ; y M/m
    /// - M for press, m for release
    /// - Coordinates are 1-based
    fn encode_mouse_sgr(button: u8, col: usize, row: usize, kind: MouseEventKind) -> Vec<u8> {
        let terminator = match kind {
            MouseEventKind::Up => 'm',
            _ => 'M',
        };
        // SGR uses 1-based coordinates
        format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, terminator)
            .into_bytes()
    }

    /// Encode mouse event using legacy X10/normal format.
    ///
    /// Format: ESC [ M button+32 x+32 y+32
    /// - Coordinates are 1-based and offset by 32 to make them printable
    /// - Limited to coordinates <= 223
    fn encode_mouse_legacy(button: u8, col: usize, row: usize) -> Vec<u8> {
        // Clamp coordinates to printable range (1-223 after +32 offset)
        let x = (col.min(222) + 1 + 32) as u8;
        let y = (row.min(222) + 1 + 32) as u8;
        let btn = button + 32;

        vec![0x1b, b'[', b'M', btn, x, y]
    }

    // Chunk: docs/chunks/terminal_scrollback_viewport - Scroll wheel encoding for alternate screen
    /// Encode a scroll wheel event for terminal applications.
    ///
    /// This encodes scroll events as mouse button 64 (scroll up) or 65 (scroll down).
    /// These are the standard xterm scroll wheel button codes.
    ///
    /// Returns an empty vector if no mouse mode is active.
    ///
    /// # Arguments
    ///
    /// * `lines` - Number of lines to scroll (positive = down, negative = up)
    /// * `col` - Column position of the mouse cursor
    /// * `row` - Row position of the mouse cursor
    /// * `modifiers` - Active keyboard modifiers
    /// * `modes` - Terminal mode flags
    pub fn encode_scroll(
        lines: i32,
        col: usize,
        row: usize,
        modifiers: &Modifiers,
        modes: TermMode,
    ) -> Vec<u8> {
        // Check if any mouse mode is active
        if !modes.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG,
        ) {
            return Vec::new();
        }

        if lines == 0 {
            return Vec::new();
        }

        // Calculate base button code:
        // - 64 = scroll up (lines < 0)
        // - 65 = scroll down (lines > 0)
        let base_button: u8 = if lines < 0 { 64 } else { 65 };

        // Add modifier bits
        let mut button = base_button;
        if modifiers.shift {
            button |= 4;
        }
        if modifiers.option {
            button |= 8;
        }
        if modifiers.control {
            button |= 16;
        }

        // Generate the sequence for each line of scroll
        // Most terminal applications expect one event per "scroll tick"
        let count = lines.abs() as usize;
        let mut result = Vec::with_capacity(count * 10); // Approximate size

        for _ in 0..count {
            if modes.contains(TermMode::SGR_MOUSE) {
                // SGR format: ESC [ < button ; col ; row M
                // SGR uses 1-based coordinates
                let seq = format!("\x1b[<{};{};{}M", button, col + 1, row + 1);
                result.extend_from_slice(seq.as_bytes());
            } else {
                // Legacy X10/normal format: ESC [ M button+32 x+32 y+32
                let x = (col.min(222) + 1 + 32) as u8;
                let y = (row.min(222) + 1 + 32) as u8;
                let btn = button + 32;
                result.extend_from_slice(&[0x1b, b'[', b'M', btn, x, y]);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Basic Character Encoding Tests
    // =========================================================================

    #[test]
    fn test_encode_printable_ascii() {
        let event = KeyEvent::char('a');
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, b"a");
    }

    #[test]
    fn test_encode_uppercase() {
        let event = KeyEvent::char('A');
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, b"A");
    }

    #[test]
    fn test_encode_unicode() {
        let event = KeyEvent::char('\u{1F600}'); // Grinning face emoji
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, "\u{1F600}".as_bytes());
    }

    // =========================================================================
    // Control Character Tests
    // =========================================================================

    #[test]
    fn test_encode_ctrl_c() {
        let event = KeyEvent {
            key: Key::Char('c'),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        };
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x03]); // ETX
    }

    #[test]
    fn test_encode_ctrl_d() {
        let event = KeyEvent {
            key: Key::Char('d'),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        };
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x04]); // EOT
    }

    #[test]
    fn test_encode_ctrl_z() {
        let event = KeyEvent {
            key: Key::Char('z'),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        };
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x1a]); // SUB
    }

    #[test]
    fn test_encode_ctrl_bracket() {
        let event = KeyEvent {
            key: Key::Char('['),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        };
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x1b]); // ESC
    }

    // =========================================================================
    // Special Key Tests
    // =========================================================================

    #[test]
    fn test_encode_return() {
        let event = KeyEvent::new(Key::Return, Modifiers::default());
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x0d]);
    }

    #[test]
    fn test_encode_tab() {
        let event = KeyEvent::new(Key::Tab, Modifiers::default());
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x09]);
    }

    #[test]
    fn test_encode_escape() {
        let event = KeyEvent::new(Key::Escape, Modifiers::default());
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x1b]);
    }

    #[test]
    fn test_encode_backspace() {
        let event = KeyEvent::new(Key::Backspace, Modifiers::default());
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, vec![0x7f]);
    }

    // =========================================================================
    // Arrow Key Tests
    // =========================================================================

    #[test]
    fn test_encode_arrow_normal_mode() {
        let up = KeyEvent::new(Key::Up, Modifiers::default());
        let down = KeyEvent::new(Key::Down, Modifiers::default());
        let right = KeyEvent::new(Key::Right, Modifiers::default());
        let left = KeyEvent::new(Key::Left, Modifiers::default());

        assert_eq!(InputEncoder::encode_key(&up, TermMode::NONE), b"\x1b[A");
        assert_eq!(InputEncoder::encode_key(&down, TermMode::NONE), b"\x1b[B");
        assert_eq!(InputEncoder::encode_key(&right, TermMode::NONE), b"\x1b[C");
        assert_eq!(InputEncoder::encode_key(&left, TermMode::NONE), b"\x1b[D");
    }

    #[test]
    fn test_encode_arrow_app_cursor_mode() {
        let up = KeyEvent::new(Key::Up, Modifiers::default());
        let down = KeyEvent::new(Key::Down, Modifiers::default());
        let right = KeyEvent::new(Key::Right, Modifiers::default());
        let left = KeyEvent::new(Key::Left, Modifiers::default());

        let modes = TermMode::APP_CURSOR;
        assert_eq!(InputEncoder::encode_key(&up, modes), b"\x1bOA");
        assert_eq!(InputEncoder::encode_key(&down, modes), b"\x1bOB");
        assert_eq!(InputEncoder::encode_key(&right, modes), b"\x1bOC");
        assert_eq!(InputEncoder::encode_key(&left, modes), b"\x1bOD");
    }

    #[test]
    fn test_encode_arrow_with_shift() {
        let event = KeyEvent {
            key: Key::Up,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        // Shift = modifier code 2
        assert_eq!(InputEncoder::encode_key(&event, TermMode::NONE), b"\x1b[1;2A");
    }

    #[test]
    fn test_encode_arrow_with_ctrl() {
        let event = KeyEvent {
            key: Key::Right,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        };
        // Ctrl = modifier code 5
        assert_eq!(InputEncoder::encode_key(&event, TermMode::NONE), b"\x1b[1;5C");
    }

    #[test]
    fn test_encode_arrow_with_shift_ctrl() {
        let event = KeyEvent {
            key: Key::Left,
            modifiers: Modifiers {
                shift: true,
                control: true,
                ..Default::default()
            },
        };
        // Shift + Ctrl = modifier code 6
        assert_eq!(InputEncoder::encode_key(&event, TermMode::NONE), b"\x1b[1;6D");
    }

    // =========================================================================
    // Function Key Tests
    // =========================================================================

    #[test]
    fn test_encode_f1_f4() {
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F1, Modifiers::default()), TermMode::NONE),
            b"\x1bOP"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F2, Modifiers::default()), TermMode::NONE),
            b"\x1bOQ"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F3, Modifiers::default()), TermMode::NONE),
            b"\x1bOR"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F4, Modifiers::default()), TermMode::NONE),
            b"\x1bOS"
        );
    }

    #[test]
    fn test_encode_f5_f12() {
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F5, Modifiers::default()), TermMode::NONE),
            b"\x1b[15~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F6, Modifiers::default()), TermMode::NONE),
            b"\x1b[17~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F7, Modifiers::default()), TermMode::NONE),
            b"\x1b[18~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F8, Modifiers::default()), TermMode::NONE),
            b"\x1b[19~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F9, Modifiers::default()), TermMode::NONE),
            b"\x1b[20~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F10, Modifiers::default()), TermMode::NONE),
            b"\x1b[21~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F11, Modifiers::default()), TermMode::NONE),
            b"\x1b[23~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::F12, Modifiers::default()), TermMode::NONE),
            b"\x1b[24~"
        );
    }

    #[test]
    fn test_encode_f5_with_shift() {
        let event = KeyEvent {
            key: Key::F5,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        assert_eq!(InputEncoder::encode_key(&event, TermMode::NONE), b"\x1b[15;2~");
    }

    // =========================================================================
    // Navigation Key Tests
    // =========================================================================

    #[test]
    fn test_encode_navigation_keys() {
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::Insert, Modifiers::default()), TermMode::NONE),
            b"\x1b[2~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::Delete, Modifiers::default()), TermMode::NONE),
            b"\x1b[3~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::PageUp, Modifiers::default()), TermMode::NONE),
            b"\x1b[5~"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::PageDown, Modifiers::default()), TermMode::NONE),
            b"\x1b[6~"
        );
    }

    #[test]
    fn test_encode_home_end_normal() {
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::Home, Modifiers::default()), TermMode::NONE),
            b"\x1b[H"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::End, Modifiers::default()), TermMode::NONE),
            b"\x1b[F"
        );
    }

    #[test]
    fn test_encode_home_end_app_cursor() {
        let modes = TermMode::APP_CURSOR;
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::Home, Modifiers::default()), modes),
            b"\x1bOH"
        );
        assert_eq!(
            InputEncoder::encode_key(&KeyEvent::new(Key::End, Modifiers::default()), modes),
            b"\x1bOF"
        );
    }

    // =========================================================================
    // Alt/Option Key Tests
    // =========================================================================

    #[test]
    fn test_encode_alt_character() {
        let event = KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers {
                option: true,
                ..Default::default()
            },
        };
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        assert_eq!(result, b"\x1ba"); // ESC + a
    }

    // Chunk: docs/chunks/terminal_alt_backspace - Alt+Backspace sends ESC+DEL
    #[test]
    fn test_encode_alt_backspace() {
        let event = KeyEvent {
            key: Key::Backspace,
            modifiers: Modifiers {
                option: true,
                ..Default::default()
            },
        };
        let result = InputEncoder::encode_key(&event, TermMode::NONE);
        // Alt+Backspace should send ESC + DEL for backward word delete
        assert_eq!(result, b"\x1b\x7f");
    }

    // =========================================================================
    // Bracketed Paste Tests
    // =========================================================================

    #[test]
    fn test_encode_paste_no_bracketed_mode() {
        let result = InputEncoder::encode_paste("hello world", TermMode::NONE);
        assert_eq!(result, b"hello world");
    }

    #[test]
    fn test_encode_paste_bracketed_mode() {
        let result = InputEncoder::encode_paste("hello world", TermMode::BRACKETED_PASTE);
        assert_eq!(result, b"\x1b[200~hello world\x1b[201~");
    }

    // =========================================================================
    // Mouse Encoding Tests
    // =========================================================================

    #[test]
    fn test_encode_mouse_no_mode() {
        let event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (100.0, 200.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        let result = InputEncoder::encode_mouse(&event, 10, 5, TermMode::NONE);
        assert!(result.is_empty());
    }

    #[test]
    fn test_encode_mouse_sgr_click() {
        let event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (100.0, 200.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        let result = InputEncoder::encode_mouse(&event, 10, 5, TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // Button 0 (left), col 11 (1-based), row 6 (1-based), M for press
        assert_eq!(result, b"\x1b[<0;11;6M");
    }

    #[test]
    fn test_encode_mouse_sgr_release() {
        let event = MouseEvent {
            kind: MouseEventKind::Up,
            position: (100.0, 200.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        let result = InputEncoder::encode_mouse(&event, 10, 5, TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // Button 3 (release), lowercase m for release
        assert_eq!(result, b"\x1b[<3;11;6m");
    }

    #[test]
    fn test_encode_mouse_legacy() {
        let event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (100.0, 200.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        let result = InputEncoder::encode_mouse(&event, 10, 5, TermMode::MOUSE_REPORT_CLICK);
        // ESC [ M, button+32, x+32+1, y+32+1
        // button=0+32=32=' ', x=10+33=43='+', y=5+33=38='&'
        assert_eq!(result, b"\x1b[M +&");
    }

    #[test]
    fn test_encode_mouse_with_modifiers() {
        let event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (100.0, 200.0),
            modifiers: Modifiers {
                shift: true,
                control: true,
                ..Default::default()
            },
            click_count: 1,
        };
        let result = InputEncoder::encode_mouse(&event, 10, 5, TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // Button 0 + shift(4) + ctrl(16) = 20
        assert_eq!(result, b"\x1b[<20;11;6M");
    }

    // =========================================================================
    // Scroll Encoding Tests
    // Chunk: docs/chunks/terminal_scrollback_viewport - Scroll wheel encoding tests
    // =========================================================================

    #[test]
    fn test_encode_scroll_no_mode() {
        let result = InputEncoder::encode_scroll(3, 10, 5, &Modifiers::default(), TermMode::NONE);
        assert!(result.is_empty());
    }

    #[test]
    fn test_encode_scroll_zero_lines() {
        let result = InputEncoder::encode_scroll(0, 10, 5, &Modifiers::default(), TermMode::MOUSE_REPORT_CLICK);
        assert!(result.is_empty());
    }

    #[test]
    fn test_encode_scroll_down_sgr() {
        // Scroll down 1 line
        let result = InputEncoder::encode_scroll(1, 10, 5, &Modifiers::default(), TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // Button 65 (scroll down), col 11 (1-based), row 6 (1-based), M for press
        assert_eq!(result, b"\x1b[<65;11;6M");
    }

    #[test]
    fn test_encode_scroll_up_sgr() {
        // Scroll up 1 line (negative)
        let result = InputEncoder::encode_scroll(-1, 10, 5, &Modifiers::default(), TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // Button 64 (scroll up)
        assert_eq!(result, b"\x1b[<64;11;6M");
    }

    #[test]
    fn test_encode_scroll_multiple_lines_sgr() {
        // Scroll down 3 lines - should produce 3 separate events
        let result = InputEncoder::encode_scroll(3, 10, 5, &Modifiers::default(), TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // 3 events: ESC[<65;11;6M repeated 3 times
        assert_eq!(result, b"\x1b[<65;11;6M\x1b[<65;11;6M\x1b[<65;11;6M");
    }

    #[test]
    fn test_encode_scroll_down_legacy() {
        // Scroll down 1 line in legacy mode
        let result = InputEncoder::encode_scroll(1, 10, 5, &Modifiers::default(), TermMode::MOUSE_REPORT_CLICK);
        // ESC [ M, button+32, x+32+1, y+32+1
        // button=65+32=97='a', x=10+33=43='+', y=5+33=38='&'
        assert_eq!(result, b"\x1b[Ma+&");
    }

    #[test]
    fn test_encode_scroll_up_legacy() {
        // Scroll up 1 line in legacy mode
        let result = InputEncoder::encode_scroll(-1, 10, 5, &Modifiers::default(), TermMode::MOUSE_REPORT_CLICK);
        // button=64+32=96='`'
        assert_eq!(result, b"\x1b[M`+&");
    }

    #[test]
    fn test_encode_scroll_with_modifiers() {
        // Scroll down with Shift + Ctrl
        let mods = Modifiers {
            shift: true,
            control: true,
            ..Default::default()
        };
        let result = InputEncoder::encode_scroll(1, 10, 5, &mods, TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE);
        // Button 65 + shift(4) + ctrl(16) = 85
        assert_eq!(result, b"\x1b[<85;11;6M");
    }
}
