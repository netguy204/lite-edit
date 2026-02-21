// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
//!
//! Buffer focus target implementation.
//!
//! This is the focus target for the main text buffer. It handles standard
//! editing keystrokes: printable characters, backspace, delete, arrow keys,
//! etc. Chord resolution is a stateless pure function per the investigation's
//! H2 findings — all target chords are single-step modifier+key combinations.

use crate::context::EditorContext;
use crate::focus::{FocusTarget, Handled};
use crate::input::{Key, KeyEvent, MouseEvent, ScrollDelta};

/// Commands that can be executed on the buffer.
///
/// These are resolved from key events by the stateless `resolve_command` function.
#[derive(Debug, Clone, PartialEq)]
enum Command {
    /// Insert a character at the cursor
    InsertChar(char),
    /// Insert a newline at the cursor
    InsertNewline,
    /// Delete the character before the cursor (Backspace)
    DeleteBackward,
    /// Delete the character after the cursor (Delete key)
    DeleteForward,
    /// Move cursor left by one character
    MoveLeft,
    /// Move cursor right by one character
    MoveRight,
    /// Move cursor up by one line
    MoveUp,
    /// Move cursor down by one line
    MoveDown,
    /// Move cursor to the start of the line
    MoveToLineStart,
    /// Move cursor to the end of the line
    MoveToLineEnd,
    /// Move cursor to the start of the buffer
    MoveToBufferStart,
    /// Move cursor to the end of the buffer
    MoveToBufferEnd,
    /// Insert a tab character
    InsertTab,
}

/// Resolves a key event to a command.
///
/// This is a pure stateless function: (modifiers, key) → Option<Command>.
/// Per the H2 investigation finding, all target chords are single-step
/// modifier+key combinations, so no state machine is needed.
fn resolve_command(event: &KeyEvent) -> Option<Command> {
    let mods = &event.modifiers;

    match &event.key {
        // Printable characters (no Command/Control modifier)
        Key::Char(ch) if !mods.command && !mods.control => Some(Command::InsertChar(*ch)),

        // Return/Enter
        Key::Return if !mods.command && !mods.control => Some(Command::InsertNewline),

        // Tab
        Key::Tab if !mods.command && !mods.control => Some(Command::InsertTab),

        // Backspace (Delete backward)
        Key::Backspace => Some(Command::DeleteBackward),

        // Forward delete
        Key::Delete => Some(Command::DeleteForward),

        // Arrow keys
        Key::Left if !mods.command => Some(Command::MoveLeft),
        Key::Right if !mods.command => Some(Command::MoveRight),
        Key::Up if !mods.command => Some(Command::MoveUp),
        Key::Down if !mods.command => Some(Command::MoveDown),

        // Cmd+Left or Home → start of line
        Key::Left if mods.command => Some(Command::MoveToLineStart),
        Key::Home => Some(Command::MoveToLineStart),

        // Cmd+Right or End → end of line
        Key::Right if mods.command => Some(Command::MoveToLineEnd),
        Key::End => Some(Command::MoveToLineEnd),

        // Cmd+Up → start of buffer
        Key::Up if mods.command => Some(Command::MoveToBufferStart),

        // Cmd+Down → end of buffer
        Key::Down if mods.command => Some(Command::MoveToBufferEnd),

        // Ctrl+A → start of line (Emacs-style)
        Key::Char('a') if mods.control && !mods.command => Some(Command::MoveToLineStart),

        // Ctrl+E → end of line (Emacs-style)
        Key::Char('e') if mods.control && !mods.command => Some(Command::MoveToLineEnd),

        // Unhandled
        _ => None,
    }
}

/// The focus target for the main text buffer.
///
/// Handles standard editing keystrokes via stateless chord resolution.
#[derive(Debug, Default)]
pub struct BufferFocusTarget;

impl BufferFocusTarget {
    /// Creates a new BufferFocusTarget.
    pub fn new() -> Self {
        Self
    }

    /// Executes a command on the buffer through the editor context.
    fn execute_command(&self, cmd: Command, ctx: &mut EditorContext) {
        let dirty = match cmd {
            Command::InsertChar(ch) => ctx.buffer.insert_char(ch),
            Command::InsertNewline => ctx.buffer.insert_newline(),
            Command::InsertTab => ctx.buffer.insert_char('\t'),
            Command::DeleteBackward => ctx.buffer.delete_backward(),
            Command::DeleteForward => ctx.buffer.delete_forward(),
            Command::MoveLeft => {
                ctx.buffer.move_left();
                // Cursor movement doesn't dirty buffer content, but we need to redraw
                // the old and new cursor positions. For simplicity, mark cursor line dirty.
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
            Command::MoveRight => {
                ctx.buffer.move_right();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
            Command::MoveUp => {
                ctx.buffer.move_up();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
            Command::MoveDown => {
                ctx.buffer.move_down();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
            Command::MoveToLineStart => {
                ctx.buffer.move_to_line_start();
                ctx.mark_cursor_dirty();
                return;
            }
            Command::MoveToLineEnd => {
                ctx.buffer.move_to_line_end();
                ctx.mark_cursor_dirty();
                return;
            }
            Command::MoveToBufferStart => {
                ctx.buffer.move_to_buffer_start();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
            Command::MoveToBufferEnd => {
                ctx.buffer.move_to_buffer_end();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
        };

        // Mark the affected lines dirty
        ctx.mark_dirty(dirty);

        // Ensure cursor is visible after mutation
        ctx.ensure_cursor_visible();
    }
}

impl FocusTarget for BufferFocusTarget {
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled {
        match resolve_command(&event) {
            Some(cmd) => {
                self.execute_command(cmd, ctx);
                Handled::Yes
            }
            None => Handled::No,
        }
    }

    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext) {
        // Convert scroll delta to line offset
        // Positive dy = scroll down (content moves up, scroll_offset increases)
        // Negative dy = scroll up (content moves down, scroll_offset decreases)
        let line_height = ctx.viewport.line_height();
        let lines_to_scroll = (delta.dy / line_height as f64).round() as i32;

        if lines_to_scroll == 0 {
            return;
        }

        let line_count = ctx.buffer.line_count();
        let current_offset = ctx.viewport.scroll_offset;

        let new_offset = if lines_to_scroll > 0 {
            // Scroll down
            current_offset.saturating_add(lines_to_scroll as usize)
        } else {
            // Scroll up
            current_offset.saturating_sub((-lines_to_scroll) as usize)
        };

        ctx.viewport.scroll_to(new_offset, line_count);

        // Mark full viewport dirty if we actually scrolled
        if ctx.viewport.scroll_offset != current_offset {
            ctx.dirty_region.merge(crate::dirty_region::DirtyRegion::FullViewport);
        }
    }

    fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {
        // Mouse handling is a future concern - stub for now
        // Future: cursor placement, selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty_region::DirtyRegion;
    use crate::input::Modifiers;
    use crate::viewport::Viewport;
    use lite_edit_buffer::{Position, TextBuffer};

    fn create_test_context() -> (TextBuffer, Viewport, DirtyRegion) {
        let buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0); // 10 visible lines
        let dirty = DirtyRegion::None;
        (buffer, viewport, dirty)
    }

    #[test]
    fn test_typing_hello() {
        let (mut buffer, mut viewport, mut dirty) = create_test_context();
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::char('H'), &mut ctx);
            target.handle_key(KeyEvent::char('i'), &mut ctx);
        }

        assert_eq!(buffer.content(), "Hi");
        assert_eq!(buffer.cursor_position(), Position::new(0, 2));
        assert!(dirty.is_dirty());
    }

    #[test]
    fn test_typing_then_backspace() {
        let (mut buffer, mut viewport, mut dirty) = create_test_context();
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::char('a'), &mut ctx);
            target.handle_key(KeyEvent::char('b'), &mut ctx);
            target.handle_key(KeyEvent::char('c'), &mut ctx);
            target.handle_key(KeyEvent::new(Key::Backspace, Modifiers::default()), &mut ctx);
        }

        assert_eq!(buffer.content(), "ab");
        assert_eq!(buffer.cursor_position(), Position::new(0, 2));
    }

    #[test]
    fn test_arrow_keys_move_cursor() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Move right
        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::new(Key::Right, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(0, 1));

        // Move down
        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::new(Key::Down, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(1, 1));

        // Move left
        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::new(Key::Left, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(1, 0));

        // Move up
        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::new(Key::Up, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_enter_creates_newline() {
        let (mut buffer, mut viewport, mut dirty) = create_test_context();
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::char('a'), &mut ctx);
            target.handle_key(KeyEvent::new(Key::Return, Modifiers::default()), &mut ctx);
            target.handle_key(KeyEvent::char('b'), &mut ctx);
        }

        assert_eq!(buffer.line_count(), 2);
        assert_eq!(buffer.line_content(0), "a");
        assert_eq!(buffer.line_content(1), "b");
        assert_eq!(buffer.cursor_position(), Position::new(1, 1));
    }

    #[test]
    fn test_delete_forward() {
        let mut buffer = TextBuffer::from_str("hello");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            target.handle_key(KeyEvent::new(Key::Delete, Modifiers::default()), &mut ctx);
        }

        assert_eq!(buffer.content(), "ello");
    }

    #[test]
    fn test_cmd_left_moves_to_line_start() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 6));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            let event = KeyEvent::new(
                Key::Left,
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_cmd_right_moves_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 0));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            let event = KeyEvent::new(
                Key::Right,
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 11));
    }

    #[test]
    fn test_ctrl_a_moves_to_line_start() {
        let mut buffer = TextBuffer::from_str("hello");
        buffer.set_cursor(Position::new(0, 3));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            let event = KeyEvent::new(
                Key::Char('a'),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_ctrl_e_moves_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello");
        buffer.set_cursor(Position::new(0, 0));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            let event = KeyEvent::new(
                Key::Char('e'),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_home_moves_to_line_start() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 6));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            let event = KeyEvent::new(Key::Home, Modifiers::default());
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_end_moves_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 0));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            let event = KeyEvent::new(Key::End, Modifiers::default());
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 11));
    }

    #[test]
    fn test_cursor_movement_past_viewport_scrolls() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0); // 10 visible lines
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Move cursor to line 15 (beyond viewport)
        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            for _ in 0..15 {
                target.handle_key(KeyEvent::new(Key::Down, Modifiers::default()), &mut ctx);
            }
        }

        // Viewport should have scrolled
        assert!(viewport.scroll_offset > 0);
        // Should have marked full viewport dirty for the scroll
        assert_eq!(dirty, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_unhandled_key_returns_no() {
        let (mut buffer, mut viewport, mut dirty) = create_test_context();
        let mut target = BufferFocusTarget::new();

        let result = {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);
            // Cmd+Z is not handled
            target.handle_key(
                KeyEvent::new(
                    Key::Char('z'),
                    Modifiers {
                        command: true,
                        ..Default::default()
                    },
                ),
                &mut ctx,
            )
        };

        assert_eq!(result, Handled::No);
    }

    #[test]
    fn test_multiple_events_accumulate_dirty_regions() {
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo\nbar");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(&mut buffer, &mut viewport, &mut dirty);

            // Type on line 0
            target.handle_key(KeyEvent::char('x'), &mut ctx);

            // Move to line 2 and type
            target.handle_key(KeyEvent::new(Key::Down, Modifiers::default()), &mut ctx);
            target.handle_key(KeyEvent::new(Key::Down, Modifiers::default()), &mut ctx);
            target.handle_key(KeyEvent::char('y'), &mut ctx);
        }

        // Should have accumulated dirty from multiple lines
        assert!(dirty.is_dirty());
    }
}
