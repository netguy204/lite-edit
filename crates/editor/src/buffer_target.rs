// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/viewport_scrolling - Scroll event handling
//!
//! Buffer focus target implementation.
//!
//! This is the focus target for the main text buffer. It handles standard
//! editing keystrokes: printable characters, backspace, delete, arrow keys,
//! etc. Chord resolution is a stateless pure function per the investigation's
//! H2 findings — all target chords are single-step modifier+key combinations.
//!
//! Also handles mouse click events for cursor positioning, converting pixel
//! coordinates to buffer positions using font metrics and viewport scroll offset.

use crate::context::EditorContext;
use crate::focus::{FocusTarget, Handled};
use crate::font::FontMetrics;
use crate::input::{Key, KeyEvent, MouseEvent, MouseEventKind, ScrollDelta};
use lite_edit_buffer::Position;

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
    /// Delete from cursor to end of line (kill-line)
    DeleteToLineEnd,
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
    // Chunk: docs/chunks/clipboard_operations - Clipboard command variants
    /// Select the entire buffer (Cmd+A)
    SelectAll,
    /// Copy selection to clipboard (Cmd+C)
    Copy,
    /// Paste from clipboard at cursor (Cmd+V)
    Paste,
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

        // Chunk: docs/chunks/clipboard_operations - Clipboard key bindings
        // Cmd+A → select all (must come before Ctrl+A)
        Key::Char('a') if mods.command && !mods.control => Some(Command::SelectAll),

        // Cmd+C → copy selection to clipboard
        Key::Char('c') if mods.command && !mods.control => Some(Command::Copy),

        // Cmd+V → paste from clipboard
        Key::Char('v') if mods.command && !mods.control => Some(Command::Paste),

        // Ctrl+A → start of line (Emacs-style)
        Key::Char('a') if mods.control && !mods.command => Some(Command::MoveToLineStart),

        // Ctrl+E → end of line (Emacs-style)
        Key::Char('e') if mods.control && !mods.command => Some(Command::MoveToLineEnd),

        // Ctrl+K → kill line (delete to end of line)
        Key::Char('k') if mods.control && !mods.command => Some(Command::DeleteToLineEnd),

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
            Command::DeleteToLineEnd => ctx.buffer.delete_to_line_end(),
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
            // Chunk: docs/chunks/clipboard_operations - Clipboard command execution
            Command::SelectAll => {
                ctx.buffer.select_all();
                // Mark full viewport dirty since all visible lines now have selection highlight
                ctx.dirty_region.merge(crate::dirty_region::DirtyRegion::FullViewport);
                return;
            }
            Command::Copy => {
                // Get selected text; no-op if no selection
                if let Some(text) = ctx.buffer.selected_text() {
                    crate::clipboard::copy_to_clipboard(&text);
                }
                // Do not modify buffer or clear selection (standard copy behavior)
                return;
            }
            Command::Paste => {
                if let Some(text) = crate::clipboard::paste_from_clipboard() {
                    let dirty = ctx.buffer.insert_str(&text);
                    ctx.mark_dirty(dirty);
                    ctx.ensure_cursor_visible();
                }
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

    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext) {
        match event.kind {
            MouseEventKind::Down => {
                // Convert pixel position to buffer position and set cursor
                let position = pixel_to_buffer_position(
                    event.position,
                    ctx.view_height,
                    &ctx.font_metrics,
                    ctx.viewport.scroll_offset,
                    ctx.buffer.line_count(),
                    |line| ctx.buffer.line_len(line),
                );
                ctx.buffer.set_cursor(position);
                ctx.mark_cursor_dirty();
            }
            MouseEventKind::Up | MouseEventKind::Moved => {
                // Selection (drag) is a future concern
            }
        }
    }
}

/// Converts pixel coordinates to buffer position.
///
/// This is the core math for mouse click → cursor positioning:
/// 1. Flip y-coordinate (macOS uses bottom-left origin, buffer uses top-left)
/// 2. Compute line from y: `line = (flipped_y / line_height) + scroll_offset`
/// 3. Compute column from x: `col = x / char_width`
/// 4. Clamp to valid buffer bounds
///
/// # Arguments
/// * `position` - Pixel position (x, y) in view coordinates
/// * `view_height` - Total view height in pixels
/// * `font_metrics` - Font metrics for character dimensions
/// * `scroll_offset` - Current viewport scroll offset (first visible buffer line)
/// * `line_count` - Total number of lines in the buffer
/// * `line_len_fn` - Closure to get the length of a specific line
///
/// # Returns
/// A buffer `Position` with line and column clamped to valid ranges.
fn pixel_to_buffer_position<F>(
    position: (f64, f64),
    view_height: f32,
    font_metrics: &FontMetrics,
    scroll_offset: usize,
    line_count: usize,
    line_len_fn: F,
) -> Position
where
    F: Fn(usize) -> usize,
{
    let (x, y) = position;
    let line_height = font_metrics.line_height;
    let char_width = font_metrics.advance_width;

    // Flip y-coordinate: macOS uses bottom-left origin, buffer uses top-left
    let flipped_y = (view_height as f64) - y;

    // Compute screen line (which line on screen, 0-indexed from top)
    // Use truncation (floor for positive values) so clicking in the top half
    // of a line targets that line
    let screen_line = if flipped_y >= 0.0 && line_height > 0.0 {
        (flipped_y / line_height).floor() as usize
    } else {
        0
    };

    // Convert screen line to buffer line
    let buffer_line = scroll_offset.saturating_add(screen_line);

    // Clamp to valid line range (0..line_count, but line_count could be 0)
    let clamped_line = if line_count == 0 {
        0
    } else {
        buffer_line.min(line_count - 1)
    };

    // Compute column from x position
    // Use truncation so clicking in the left half of a character targets that column
    let col = if x >= 0.0 && char_width > 0.0 {
        (x / char_width).floor() as usize
    } else {
        0
    };

    // Clamp column to line length (can't position cursor past end of line)
    let line_len = line_len_fn(clamped_line);
    let clamped_col = col.min(line_len);

    Position::new(clamped_line, clamped_col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty_region::DirtyRegion;
    use crate::font::FontMetrics;
    use crate::input::{Modifiers, ScrollDelta};
    use crate::viewport::Viewport;
    use lite_edit_buffer::{Position, TextBuffer};

    /// Creates test font metrics with known values
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_key(KeyEvent::new(Key::Right, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(0, 1));

        // Move down
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_key(KeyEvent::new(Key::Down, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(1, 1));

        // Move left
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_key(KeyEvent::new(Key::Left, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(1, 0));

        // Move up
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_key(KeyEvent::new(Key::Up, Modifiers::default()), &mut ctx);
        }
        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_enter_creates_newline() {
        let (mut buffer, mut viewport, mut dirty) = create_test_context();
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
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
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );

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

    // ==================== Pixel to Position Tests ====================

    #[test]
    fn test_pixel_to_position_first_character() {
        // Click at top-left corner (first character of first line)
        // View height = 160, line_height = 16, char_width = 8
        // macOS y-coordinate is from bottom, so clicking at the top means y = 160 - small
        // To hit line 0 (top line), we need flipped_y in [0, 16)
        // flipped_y = 160 - y, so y should be in (144, 160]
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (0.0, 155.0), // x=0, y near top (160-155=5, which is in first line)
            160.0,
            &metrics,
            0, // no scroll
            5, // 5 lines
            |_| 10, // all lines have 10 chars
        );
        assert_eq!(position, Position::new(0, 0));
    }

    #[test]
    fn test_pixel_to_position_second_line() {
        // Click on second line
        // line_height = 16, so line 1 is at flipped_y in [16, 32)
        // To get flipped_y = 20, we need y = 160 - 20 = 140
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (0.0, 140.0), // flipped_y = 20, line 1
            160.0,
            &metrics,
            0,
            5,
            |_| 10,
        );
        assert_eq!(position, Position::new(1, 0));
    }

    #[test]
    fn test_pixel_to_position_column_calculation() {
        // Click at x = 24 pixels with char_width = 8
        // col = floor(24 / 8) = 3
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (24.0, 155.0), // x=24, line 0
            160.0,
            &metrics,
            0,
            5,
            |_| 10,
        );
        assert_eq!(position, Position::new(0, 3));
    }

    #[test]
    fn test_pixel_to_position_past_line_end() {
        // Click past end of line (line has 5 chars, click at col 10)
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (80.0, 155.0), // x=80, would be col 10, but line only has 5 chars
            160.0,
            &metrics,
            0,
            5,
            |_| 5, // lines have 5 chars
        );
        // Should clamp to column 5 (end of line)
        assert_eq!(position, Position::new(0, 5));
    }

    #[test]
    fn test_pixel_to_position_below_last_line() {
        // Click below last line (buffer has 3 lines, click on what would be line 5)
        // line_height = 16, line 5 is at flipped_y in [80, 96)
        // flipped_y = 85 means y = 160 - 85 = 75
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (0.0, 75.0), // would be line 5 if it existed
            160.0,
            &metrics,
            0,
            3, // only 3 lines
            |line| if line < 3 { 10 } else { 0 },
        );
        // Should clamp to last line (line 2)
        assert_eq!(position, Position::new(2, 0));
    }

    #[test]
    fn test_pixel_to_position_with_scroll_offset() {
        // Viewport is scrolled down 5 lines
        // Click on screen line 0, which should map to buffer line 5
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (0.0, 155.0), // screen line 0
            160.0,
            &metrics,
            5, // scrolled 5 lines
            20, // 20 lines total
            |_| 10,
        );
        assert_eq!(position, Position::new(5, 0));
    }

    #[test]
    fn test_pixel_to_position_empty_buffer() {
        // Click on empty buffer
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (50.0, 100.0),
            160.0,
            &metrics,
            0,
            0, // empty buffer
            |_| 0,
        );
        // Should return (0, 0) for empty buffer
        assert_eq!(position, Position::new(0, 0));
    }

    #[test]
    fn test_pixel_to_position_negative_x() {
        // Click with negative x (shouldn't happen but handle gracefully)
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (-10.0, 155.0),
            160.0,
            &metrics,
            0,
            5,
            |_| 10,
        );
        assert_eq!(position, Position::new(0, 0));
    }

    #[test]
    fn test_pixel_to_position_fractional_coordinates() {
        // Click at x = 12.7 (between col 1 and 2 with char_width=8)
        // Should use floor/truncation to target col 1
        let metrics = test_font_metrics();
        let position = super::pixel_to_buffer_position(
            (12.7, 155.0),
            160.0,
            &metrics,
            0,
            5,
            |_| 10,
        );
        // floor(12.7 / 8) = floor(1.5875) = 1
        assert_eq!(position, Position::new(0, 1));
    }

    #[test]
    fn test_mouse_click_positions_cursor() {
        // Integration test: click event positions cursor
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Click on "world" at column 2 (character 'r')
        // line 1 is at flipped_y in [16, 32)
        // y = 160 - 20 = 140 for flipped_y = 20
        // x = 16 for column 2 (8 * 2)
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Down,
                position: (16.0, 140.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(1, 2));
        assert!(dirty.is_dirty()); // Should have marked cursor line dirty
    }

    // ==================== Ctrl+K Kill Line Tests ====================

    #[test]
    fn test_ctrl_k_deletes_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 5)); // After "hello"
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        let result = {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = KeyEvent::new(
                Key::Char('k'),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx)
        };

        assert_eq!(result, Handled::Yes);
        assert_eq!(buffer.content(), "hello");
        assert_eq!(buffer.cursor_position(), Position::new(0, 5));
        assert!(dirty.is_dirty());
    }

    #[test]
    fn test_ctrl_k_joins_lines_at_end_of_line() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        buffer.set_cursor(Position::new(0, 5)); // At end of "hello"
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = KeyEvent::new(
                Key::Char('k'),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.content(), "helloworld");
        assert_eq!(buffer.line_count(), 1);
        assert_eq!(buffer.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_ctrl_k_at_buffer_end_is_noop() {
        let mut buffer = TextBuffer::from_str("hello");
        buffer.move_to_buffer_end();
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = KeyEvent::new(
                Key::Char('k'),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.content(), "hello");
        assert_eq!(buffer.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_ctrl_k_from_start_of_line() {
        let mut buffer = TextBuffer::from_str("hello");
        buffer.set_cursor(Position::new(0, 0)); // At start
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = KeyEvent::new(
                Key::Char('k'),
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.content(), "");
        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    // ==================== Clipboard Operations Tests ====================
    // Chunk: docs/chunks/clipboard_operations - Tests for Cmd+A, Cmd+C, Cmd+V

    #[test]
    fn test_cmd_a_resolves_to_select_all() {
        let event = KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        assert_eq!(resolve_command(&event), Some(Command::SelectAll));
    }

    #[test]
    fn test_cmd_c_resolves_to_copy() {
        let event = KeyEvent::new(
            Key::Char('c'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        assert_eq!(resolve_command(&event), Some(Command::Copy));
    }

    #[test]
    fn test_cmd_v_resolves_to_paste() {
        let event = KeyEvent::new(
            Key::Char('v'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        assert_eq!(resolve_command(&event), Some(Command::Paste));
    }

    #[test]
    fn test_cmd_a_vs_ctrl_a_precedence() {
        // Cmd+A should be SelectAll, not MoveToLineStart
        let cmd_a = KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        assert_eq!(resolve_command(&cmd_a), Some(Command::SelectAll));

        // Ctrl+A should still be MoveToLineStart
        let ctrl_a = KeyEvent::new(
            Key::Char('a'),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(resolve_command(&ctrl_a), Some(Command::MoveToLineStart));
    }

    #[test]
    fn test_cmd_a_selects_entire_buffer() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = KeyEvent::new(
                Key::Char('a'),
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("hello\nworld".to_string()));
        assert_eq!(dirty, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_cmd_c_with_no_selection_is_noop() {
        let mut buffer = TextBuffer::from_str("hello");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Ensure no selection
        assert!(!buffer.has_selection());

        let result = {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = KeyEvent::new(
                Key::Char('c'),
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx)
        };

        assert_eq!(result, Handled::Yes); // Command was recognized
        // Buffer unchanged, no dirty region
        assert_eq!(buffer.content(), "hello");
        assert_eq!(dirty, DirtyRegion::None);
    }

    #[test]
    fn test_cmd_a_then_type_replaces_selection() {
        // Test that Cmd+A selects all, then typing replaces the selection
        let mut buffer = TextBuffer::from_str("hello");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );

            // Cmd+A to select all
            let select_all = KeyEvent::new(
                Key::Char('a'),
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(select_all, &mut ctx);

            // Type 'X' to replace the selection
            target.handle_key(KeyEvent::char('X'), &mut ctx);
        }

        assert_eq!(buffer.content(), "X");
        assert!(!buffer.has_selection());
    }

    #[test]
    fn test_cmd_c_preserves_selection() {
        // Copy should not clear the selection
        let mut buffer = TextBuffer::from_str("hello");
        buffer.set_selection_anchor(lite_edit_buffer::Position::new(0, 0));
        buffer.set_cursor(lite_edit_buffer::Position::new(0, 5));
        // Manually set anchor without clearing (workaround since set_cursor clears)
        buffer.set_selection_anchor(lite_edit_buffer::Position::new(0, 0));

        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Verify we have a selection first
        // Need to do this differently since set_cursor clears selection
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            // Select all first
            let select_all = KeyEvent::new(
                Key::Char('a'),
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(select_all, &mut ctx);
        }

        assert!(buffer.has_selection());
        dirty = DirtyRegion::None; // Reset dirty

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let copy = KeyEvent::new(
                Key::Char('c'),
                Modifiers {
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(copy, &mut ctx);
        }

        // Selection should still be present after copy
        assert!(buffer.has_selection());
        // No dirty region since copy doesn't modify the buffer
        assert_eq!(dirty, DirtyRegion::None);
    }

    // ==================== Scroll Event Tests ====================
    // Chunk: docs/chunks/viewport_scrolling - Scroll event handling tests

    #[test]
    fn test_scroll_down_increases_offset() {
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0); // 10 visible lines
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Initial offset
        assert_eq!(viewport.scroll_offset, 0);

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            // Scroll down by 3 lines (positive dy)
            // line_height = 16, so 3 lines = 48 pixels
            target.handle_scroll(ScrollDelta::new(0.0, 48.0), &mut ctx);
        }

        assert_eq!(viewport.scroll_offset, 3);
        assert_eq!(dirty, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_scroll_up_decreases_offset() {
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        viewport.scroll_to(10, 50); // Start scrolled down
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            // Scroll up by 3 lines (negative dy)
            target.handle_scroll(ScrollDelta::new(0.0, -48.0), &mut ctx);
        }

        assert_eq!(viewport.scroll_offset, 7); // 10 - 3 = 7
        assert_eq!(dirty, DirtyRegion::FullViewport);
    }

    #[test]
    fn test_scroll_clamps_to_bounds() {
        let content = (0..20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0); // 10 visible lines, 20 total lines
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            // Try to scroll down by 30 lines (more than buffer length)
            target.handle_scroll(ScrollDelta::new(0.0, 30.0 * 16.0), &mut ctx);
        }

        // Should be clamped to max (20 - 10 = 10)
        assert_eq!(viewport.scroll_offset, 10);
    }

    #[test]
    fn test_scroll_does_not_move_cursor() {
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        buffer.set_cursor(Position::new(5, 3));
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        let original_cursor = buffer.cursor_position();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_scroll(ScrollDelta::new(0.0, 160.0), &mut ctx);
        }

        // Cursor position should be unchanged
        assert_eq!(buffer.cursor_position(), original_cursor);
    }

    #[test]
    fn test_small_scroll_delta_ignored() {
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            // Scroll by less than half a line (won't round to a full line)
            target.handle_scroll(ScrollDelta::new(0.0, 7.0), &mut ctx);
        }

        // Should remain at 0 since 7/16 rounds to 0
        assert_eq!(viewport.scroll_offset, 0);
        assert_eq!(dirty, DirtyRegion::None); // No change, no dirty
    }
}
