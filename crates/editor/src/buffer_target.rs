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
    // Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion
    /// Delete backward by one word (Alt+Backspace)
    DeleteBackwardWord,
    // Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
    /// Delete forward by one word (Alt+D)
    DeleteForwardWord,
    // Chunk: docs/chunks/kill_line - Delete from cursor to end of line (Ctrl+K)
    /// Delete from cursor to end of line (kill-line)
    DeleteToLineEnd,
    // Chunk: docs/chunks/delete_to_line_start - Cmd+Backspace command variant
    /// Delete from cursor to start of line (Cmd+Backspace)
    DeleteToLineStart,
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
    // Chunk: docs/chunks/word_jump_navigation - Word jump navigation
    /// Move cursor left by one word (Option+Left)
    MoveWordLeft,
    /// Move cursor right by one word (Option+Right)
    MoveWordRight,
    /// Insert a tab character
    InsertTab,
    // Chunk: docs/chunks/shift_arrow_selection - Shift+Arrow key selection commands
    /// Extend selection left by one character (Shift+Left)
    SelectLeft,
    /// Extend selection right by one character (Shift+Right)
    SelectRight,
    /// Extend selection up by one line (Shift+Up)
    SelectUp,
    /// Extend selection down by one line (Shift+Down)
    SelectDown,
    /// Extend selection to line start (Shift+Home, Shift+Cmd+Left)
    SelectToLineStart,
    /// Extend selection to line end (Shift+End, Shift+Cmd+Right)
    SelectToLineEnd,
    /// Extend selection to buffer start (Shift+Cmd+Up)
    SelectToBufferStart,
    /// Extend selection to buffer end (Shift+Cmd+Down)
    SelectToBufferEnd,
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
// Chunk: docs/chunks/shift_arrow_selection - Shift+Arrow key selection
// Chunk: docs/chunks/line_nav_keybindings - Home/End and Ctrl+A/Ctrl+E line navigation
fn resolve_command(event: &KeyEvent) -> Option<Command> {
    let mods = &event.modifiers;

    match &event.key {
        // Printable characters (no Command/Control modifier)
        Key::Char(ch) if !mods.command && !mods.control => Some(Command::InsertChar(*ch)),

        // Return/Enter
        Key::Return if !mods.command && !mods.control => Some(Command::InsertNewline),

        // Tab
        Key::Tab if !mods.command && !mods.control => Some(Command::InsertTab),

        // Chunk: docs/chunks/delete_to_line_start - Cmd+Backspace key binding
        // Cmd+Backspace → delete to line start
        Key::Backspace if mods.command && !mods.control => Some(Command::DeleteToLineStart),

        // Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion
        // Option+Backspace → delete backward by word (must come before generic Backspace)
        Key::Backspace if mods.option && !mods.command => Some(Command::DeleteBackwardWord),

        // Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
        // Option+D → delete forward by word (must come before generic Char)
        Key::Char('d') if mods.option && !mods.command => Some(Command::DeleteForwardWord),

        // Backspace (Delete backward)
        Key::Backspace => Some(Command::DeleteBackward),

        // Forward delete
        Key::Delete => Some(Command::DeleteForward),

        // === Selection commands (Shift held) ===
        // Chunk: docs/chunks/shift_arrow_selection - Shift+Arrow key selection

        // Shift+Arrow keys (without Command) → extend selection
        Key::Left if mods.shift && !mods.command => Some(Command::SelectLeft),
        Key::Right if mods.shift && !mods.command => Some(Command::SelectRight),
        Key::Up if mods.shift && !mods.command => Some(Command::SelectUp),
        Key::Down if mods.shift && !mods.command => Some(Command::SelectDown),

        // Shift+Cmd+Left or Shift+Home → select to line start
        Key::Left if mods.shift && mods.command => Some(Command::SelectToLineStart),
        Key::Home if mods.shift => Some(Command::SelectToLineStart),

        // Shift+Cmd+Right or Shift+End → select to line end
        Key::Right if mods.shift && mods.command => Some(Command::SelectToLineEnd),
        Key::End if mods.shift => Some(Command::SelectToLineEnd),

        // Shift+Cmd+Up → select to buffer start
        Key::Up if mods.shift && mods.command => Some(Command::SelectToBufferStart),

        // Shift+Cmd+Down → select to buffer end
        Key::Down if mods.shift && mods.command => Some(Command::SelectToBufferEnd),

        // Shift+Ctrl+A → select to line start (Emacs-style)
        Key::Char('a') if mods.shift && mods.control && !mods.command => {
            Some(Command::SelectToLineStart)
        }

        // Shift+Ctrl+E → select to line end (Emacs-style)
        Key::Char('e') if mods.shift && mods.control && !mods.command => {
            Some(Command::SelectToLineEnd)
        }

        // === Movement commands (no Shift) ===

        // Chunk: docs/chunks/word_jump_navigation - Word jump navigation
        // Option+Left → move word left (must come before plain Left)
        Key::Left if mods.option && !mods.command && !mods.shift => Some(Command::MoveWordLeft),

        // Option+Right → move word right (must come before plain Right)
        Key::Right if mods.option && !mods.command && !mods.shift => Some(Command::MoveWordRight),

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

        // Chunk: docs/chunks/kill_line - Ctrl+K key binding resolution to DeleteToLineEnd
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
    // Chunk: docs/chunks/line_nav_keybindings - MoveToLineStart/MoveToLineEnd execution
    fn execute_command(&self, cmd: Command, ctx: &mut EditorContext) {
        let dirty = match cmd {
            Command::InsertChar(ch) => ctx.buffer.insert_char(ch),
            Command::InsertNewline => ctx.buffer.insert_newline(),
            Command::InsertTab => ctx.buffer.insert_char('\t'),
            Command::DeleteBackward => ctx.buffer.delete_backward(),
            Command::DeleteForward => ctx.buffer.delete_forward(),
            // Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion
            Command::DeleteBackwardWord => ctx.buffer.delete_backward_word(),
            // Chunk: docs/chunks/word_forward_delete - Alt+D forward word deletion
            Command::DeleteForwardWord => ctx.buffer.delete_forward_word(),
            // Chunk: docs/chunks/kill_line - Execute DeleteToLineEnd command
            Command::DeleteToLineEnd => ctx.buffer.delete_to_line_end(),
            // Chunk: docs/chunks/delete_to_line_start - Execute DeleteToLineStart command
            Command::DeleteToLineStart => ctx.buffer.delete_to_line_start(),
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
            // Chunk: docs/chunks/word_jump_navigation - Word jump navigation
            Command::MoveWordLeft => {
                ctx.buffer.move_word_left();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }
            Command::MoveWordRight => {
                ctx.buffer.move_word_right();
                ctx.mark_cursor_dirty();
                ctx.ensure_cursor_visible();
                return;
            }

            // Chunk: docs/chunks/shift_arrow_selection - Selection extension commands
            // For all Select* commands:
            // 1. If no selection anchor is set, set it at the current cursor position
            // 2. Save the anchor (since move_* methods clear it)
            // 3. Move the cursor
            // 4. Restore the anchor to preserve the selection

            Command::SelectLeft => {
                self.extend_selection_with_move(ctx, |buf| buf.move_left());
                return;
            }
            Command::SelectRight => {
                self.extend_selection_with_move(ctx, |buf| buf.move_right());
                return;
            }
            Command::SelectUp => {
                self.extend_selection_with_move(ctx, |buf| buf.move_up());
                return;
            }
            Command::SelectDown => {
                self.extend_selection_with_move(ctx, |buf| buf.move_down());
                return;
            }
            Command::SelectToLineStart => {
                self.extend_selection_with_move(ctx, |buf| buf.move_to_line_start());
                return;
            }
            Command::SelectToLineEnd => {
                self.extend_selection_with_move(ctx, |buf| buf.move_to_line_end());
                return;
            }
            Command::SelectToBufferStart => {
                self.extend_selection_with_move(ctx, |buf| buf.move_to_buffer_start());
                return;
            }
            Command::SelectToBufferEnd => {
                self.extend_selection_with_move(ctx, |buf| buf.move_to_buffer_end());
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

    /// Extends the selection by executing a movement operation.
    ///
    /// This implements the core selection extension logic:
    /// 1. If no selection anchor is set, set it at the current cursor position
    /// 2. Save the anchor position (since move_* methods clear it)
    /// 3. Execute the movement operation
    /// 4. Restore the anchor to preserve the selection
    /// 5. Mark affected lines dirty and ensure cursor is visible
    // Chunk: docs/chunks/shift_arrow_selection - Selection extension helper
    fn extend_selection_with_move<F>(&self, ctx: &mut EditorContext, move_fn: F)
    where
        F: FnOnce(&mut lite_edit_buffer::TextBuffer),
    {
        // Determine the anchor position:
        // - If there's already a selection, compute anchor from selection_range and cursor
        // - If no selection, the anchor will be the current cursor position
        let old_cursor = ctx.buffer.cursor_position();
        let anchor_pos = match ctx.buffer.selection_range() {
            Some((start, end)) => {
                // selection_range returns (start, end) in document order
                // The anchor is whichever end is NOT the cursor
                if old_cursor == end {
                    start
                } else {
                    end
                }
            }
            None => {
                // No selection yet - anchor is the current cursor position
                old_cursor
            }
        };

        // Execute the movement (this will clear the selection)
        move_fn(ctx.buffer);

        // Restore the anchor to preserve/establish the selection
        ctx.buffer.set_selection_anchor(anchor_pos);

        // Mark dirty and ensure cursor visible
        ctx.mark_cursor_dirty();
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

    // Chunk: docs/chunks/viewport_fractional_scroll - Smooth scrolling with pixel-level precision
    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext) {
        // Accumulate raw pixel deltas for smooth scrolling
        // Positive dy = scroll down (content moves up, scroll_offset increases)
        // Negative dy = scroll up (content moves down, scroll_offset decreases)
        let current_px = ctx.viewport.scroll_offset_px();
        let new_px = current_px + delta.dy as f32;

        let line_count = ctx.buffer.line_count();
        ctx.viewport.set_scroll_offset_px(new_px, line_count);

        // Mark full viewport dirty if we actually scrolled
        // Any scroll (even sub-pixel) requires a redraw for smooth animation
        if (ctx.viewport.scroll_offset_px() - current_px).abs() > 0.001 {
            ctx.dirty_region.merge(crate::dirty_region::DirtyRegion::FullViewport);
        }
    }

    // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection
    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext) {
        match event.kind {
            MouseEventKind::Down => {
                // Convert pixel position to buffer position and set cursor
                let position = pixel_to_buffer_position(
                    event.position,
                    ctx.view_height,
                    &ctx.font_metrics,
                    ctx.viewport.first_visible_line(),
                    ctx.buffer.line_count(),
                    |line| ctx.buffer.line_len(line),
                );
                ctx.buffer.set_cursor(position);
                // Set selection anchor for potential drag selection
                ctx.buffer.set_selection_anchor_at_cursor();
                ctx.mark_cursor_dirty();
            }
            MouseEventKind::Moved => {
                // Drag: extend selection from anchor to new position
                let old_cursor = ctx.buffer.cursor_position();

                // Convert pixel position to buffer position
                let new_position = pixel_to_buffer_position(
                    event.position,
                    ctx.view_height,
                    &ctx.font_metrics,
                    ctx.viewport.first_visible_line(),
                    ctx.buffer.line_count(),
                    |line| ctx.buffer.line_len(line),
                );

                // Move cursor without clearing selection to extend the selection
                ctx.buffer.move_cursor_preserving_selection(new_position);

                // Mark dirty region covering both old and new selection extents
                // Get the anchor position for computing dirty region
                if let Some((start, end)) = ctx.buffer.selection_range() {
                    // Compute the range of lines affected:
                    // min(old_cursor, start, end) to max(old_cursor, start, end)
                    let min_line = old_cursor.line.min(start.line).min(end.line);
                    let max_line = old_cursor.line.max(start.line).max(end.line);
                    ctx.dirty_region.merge(crate::dirty_region::DirtyRegion::line_range(
                        min_line,
                        max_line + 1,
                    ));
                } else {
                    // No selection (anchor == cursor), just mark cursor line dirty
                    ctx.mark_cursor_dirty();
                }
            }
            MouseEventKind::Up => {
                // Finalize selection: if anchor equals cursor, clear selection (click without drag)
                if !ctx.buffer.has_selection() {
                    ctx.buffer.clear_selection();
                }
                // Otherwise, leave selection active for subsequent copy/replace operations
                // No cursor position change on mouse-up
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
        assert!(viewport.first_visible_line() > 0);
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

    // ==================== Shift+Arrow Selection Tests ====================
    // Chunk: docs/chunks/shift_arrow_selection - Unit tests for selection extension

    #[test]
    fn test_shift_right_creates_selection() {
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
                Key::Right,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        // Cursor should have moved right
        assert_eq!(buffer.cursor_position(), Position::new(0, 1));
        // Selection should exist from (0, 0) to (0, 1)
        assert!(buffer.has_selection());
        assert_eq!(
            buffer.selection_range(),
            Some((Position::new(0, 0), Position::new(0, 1)))
        );
        assert_eq!(buffer.selected_text(), Some("h".to_string()));
    }

    #[test]
    fn test_shift_right_x3_selects_three_chars() {
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
            // Press Shift+Right 3 times
            for _ in 0..3 {
                let event = KeyEvent::new(
                    Key::Right,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                );
                target.handle_key(event, &mut ctx);
            }
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 3));
        assert!(buffer.has_selection());
        assert_eq!(
            buffer.selection_range(),
            Some((Position::new(0, 0), Position::new(0, 3)))
        );
        assert_eq!(buffer.selected_text(), Some("hel".to_string()));
    }

    #[test]
    fn test_shift_left_after_shift_right_shrinks_selection() {
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
            // Shift+Right 3 times
            for _ in 0..3 {
                let event = KeyEvent::new(
                    Key::Right,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                );
                target.handle_key(event, &mut ctx);
            }
            // Now Shift+Left once
            let event = KeyEvent::new(
                Key::Left,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 2));
        assert!(buffer.has_selection());
        assert_eq!(
            buffer.selection_range(),
            Some((Position::new(0, 0), Position::new(0, 2)))
        );
        assert_eq!(buffer.selected_text(), Some("he".to_string()));
    }

    #[test]
    fn test_shift_down_extends_selection_multiline() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        buffer.set_cursor(Position::new(0, 2)); // At "l" in "hello"
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
                Key::Down,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(1, 2));
        assert!(buffer.has_selection());
        // Selection from (0, 2) to (1, 2) which is "llo\nwo"
        assert_eq!(buffer.selected_text(), Some("llo\nwo".to_string()));
    }

    #[test]
    fn test_plain_right_after_selection_clears_selection() {
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
            // Shift+Right 3 times to select "hel"
            for _ in 0..3 {
                let event = KeyEvent::new(
                    Key::Right,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                );
                target.handle_key(event, &mut ctx);
            }
            // Plain Right (no shift) should clear selection and move cursor
            let event = KeyEvent::new(Key::Right, Modifiers::default());
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 4));
        assert!(!buffer.has_selection());
    }

    #[test]
    fn test_shift_home_selects_to_line_start() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 6)); // At "w" in "world"
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
                Key::Home,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("hello ".to_string()));
    }

    #[test]
    fn test_shift_end_selects_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 5)); // At space
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
                Key::End,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 11));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some(" world".to_string()));
    }

    #[test]
    fn test_selection_persists_on_shift_release() {
        // This test verifies that selection persists when no keys are pressed
        // In practice, this is handled by not clearing the selection until
        // a non-shift movement happens
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
            // Shift+Right twice
            for _ in 0..2 {
                let event = KeyEvent::new(
                    Key::Right,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                );
                target.handle_key(event, &mut ctx);
            }
        }

        // Selection should still exist (simulates "shift release")
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("he".to_string()));
    }

    #[test]
    fn test_existing_selection_can_be_extended() {
        // Test that an existing selection (e.g., from mouse drag) can be extended with Shift+Arrow
        let mut buffer = TextBuffer::from_str("hello world");
        // Simulate existing selection from (0, 0) to (0, 5)
        buffer.set_selection_anchor(Position::new(0, 0));
        buffer.set_cursor(Position::new(0, 5));
        // Need to re-set the anchor since set_cursor clears it
        buffer.set_selection_anchor(Position::new(0, 0));

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
            // Shift+Right to extend selection
            let event = KeyEvent::new(
                Key::Right,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        // Selection should extend from (0, 0) to (0, 6)
        assert_eq!(buffer.cursor_position(), Position::new(0, 6));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("hello ".to_string()));
    }

    #[test]
    fn test_shift_ctrl_a_selects_to_line_start() {
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
                    shift: true,
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("hel".to_string()));
    }

    #[test]
    fn test_shift_ctrl_e_selects_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello");
        buffer.set_cursor(Position::new(0, 2));
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
                    shift: true,
                    control: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 5));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("llo".to_string()));
    }

    #[test]
    fn test_shift_cmd_up_selects_to_buffer_start() {
        let mut buffer = TextBuffer::from_str("hello\nworld\ntest");
        buffer.set_cursor(Position::new(1, 3)); // At 'l' in "world"
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
                Key::Up,
                Modifiers {
                    shift: true,
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("hello\nwor".to_string()));
    }

    #[test]
    fn test_shift_cmd_down_selects_to_buffer_end() {
        let mut buffer = TextBuffer::from_str("hello\nworld\ntest");
        buffer.set_cursor(Position::new(0, 2)); // At 'l' in "hello"
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
                Key::Down,
                Modifiers {
                    shift: true,
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(2, 4)); // End of "test"
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("llo\nworld\ntest".to_string()));
    }

    #[test]
    fn test_shift_left_creates_selection() {
        let mut buffer = TextBuffer::from_str("hello");
        buffer.set_cursor(Position::new(0, 3)); // At second 'l'
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
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 2));
        assert!(buffer.has_selection());
        // Anchor was at (0, 3), cursor moved to (0, 2)
        assert_eq!(buffer.selected_text(), Some("l".to_string()));
    }

    #[test]
    fn test_shift_up_creates_selection() {
        let mut buffer = TextBuffer::from_str("hello\nworld");
        buffer.set_cursor(Position::new(1, 3)); // At 'l' in "world"
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
                Key::Up,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 3));
        assert!(buffer.has_selection());
        // Selection from (0, 3) to (1, 3) which is "lo\nwor"
        assert_eq!(buffer.selected_text(), Some("lo\nwor".to_string()));
    }

    #[test]
    fn test_shift_cmd_left_selects_to_line_start() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 8)); // At 'r' in "world"
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
                    shift: true,
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("hello wo".to_string()));
    }

    #[test]
    fn test_shift_cmd_right_selects_to_line_end() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 3)); // At second 'l' in "hello"
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
                    shift: true,
                    command: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 11));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("lo world".to_string()));
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
        assert_eq!(viewport.first_visible_line(), 0);

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

        assert_eq!(viewport.first_visible_line(), 3);
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

        assert_eq!(viewport.first_visible_line(), 7); // 10 - 3 = 7
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
        assert_eq!(viewport.first_visible_line(), 10);
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

    // Chunk: docs/chunks/viewport_fractional_scroll - Sub-pixel scroll accumulates
    #[test]
    fn test_small_scroll_delta_accumulates() {
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
            // Scroll by less than half a line - now accumulates in pixel space
            target.handle_scroll(ScrollDelta::new(0.0, 7.0), &mut ctx);
        }

        // First visible line should still be 0, but we've scrolled 7 pixels
        assert_eq!(viewport.first_visible_line(), 0);
        assert!((viewport.scroll_offset_px() - 7.0).abs() < 0.001);
        assert!((viewport.scroll_fraction_px() - 7.0).abs() < 0.001);
        // Even sub-pixel scrolls mark dirty for smooth animation
        assert_eq!(dirty, DirtyRegion::FullViewport);
    }

    // Chunk: docs/chunks/viewport_fractional_scroll - Accumulated sub-pixel scrolls cross line boundary
    #[test]
    fn test_accumulated_scroll_crosses_line_boundary() {
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut buffer = TextBuffer::from_str(&content);
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut target = BufferFocusTarget::new();

        // Scroll 7 pixels three times = 21 pixels = 1 line + 5 pixels
        for _ in 0..3 {
            let mut dirty = DirtyRegion::None;
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_scroll(ScrollDelta::new(0.0, 7.0), &mut ctx);
        }

        // Should now be on line 1 with 5 pixel remainder
        assert_eq!(viewport.first_visible_line(), 1);
        assert!((viewport.scroll_offset_px() - 21.0).abs() < 0.001);
        assert!((viewport.scroll_fraction_px() - 5.0).abs() < 0.001);
    }

    // ==================== Mouse Drag Selection Tests ====================
    // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection

    #[test]
    fn test_mouse_down_sets_selection_anchor() {
        // After a mouse down event, the selection anchor should be set at the clicked position
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Click on "world" at column 2 (character 'r')
        // line 1 is at flipped_y in [16, 32), y = 160 - 20 = 140 for flipped_y = 20
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

        // Cursor should be at clicked position
        assert_eq!(buffer.cursor_position(), Position::new(1, 2));
        // Selection anchor should be set at the same position
        assert!(!buffer.has_selection()); // anchor == cursor means no selection yet
        // But anchor should be Some
        buffer.move_cursor_preserving_selection(Position::new(1, 4));
        assert!(buffer.has_selection()); // now anchor != cursor
    }

    #[test]
    fn test_mouse_drag_extends_selection() {
        // Simulate down → moved → moved sequence and verify selection
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at line 0, column 1 (character 'e')
        // line 0 is at flipped_y in [0, 16), y = 160 - 5 = 155 for flipped_y = 5
        // x = 8 for column 1
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
                position: (8.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(0, 1));
        dirty = DirtyRegion::None;

        // Drag to line 1, column 3
        // line 1 is at flipped_y in [16, 32), y = 160 - 20 = 140
        // x = 24 for column 3
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                position: (24.0, 140.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Cursor should be at new position
        assert_eq!(buffer.cursor_position(), Position::new(1, 3));
        // Selection should span from anchor to cursor
        assert!(buffer.has_selection());
        let range = buffer.selection_range().unwrap();
        assert_eq!(range, (Position::new(0, 1), Position::new(1, 3)));
        // Selected text should be "ello\nwor"
        assert_eq!(buffer.selected_text(), Some("ello\nwor".to_string()));
        // Dirty region should cover affected lines
        assert!(dirty.is_dirty());

        // Drag further to line 2, column 2
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                position: (16.0, 124.0), // flipped_y = 36, line 2, x = 16 for col 2
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        assert_eq!(buffer.cursor_position(), Position::new(2, 2));
        assert_eq!(buffer.selected_text(), Some("ello\nworld\nfo".to_string()));
    }

    #[test]
    fn test_click_without_drag_clears_selection() {
        // Set up an existing selection, then click without drag - should clear selection
        let mut buffer = TextBuffer::from_str("hello\nworld");
        buffer.select_all();
        assert!(buffer.has_selection());

        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at some position
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
                position: (16.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Mouse up at same position (no drag)
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Up,
                position: (16.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Selection should be cleared (anchor == cursor means no selection)
        assert!(!buffer.has_selection());
    }

    #[test]
    fn test_drag_then_release_preserves_selection() {
        // Drag to create selection, then release - selection should remain
        let mut buffer = TextBuffer::from_str("hello");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at column 1
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
                position: (8.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Drag to column 4
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                position: (32.0, 155.0), // x = 32 for column 4
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Mouse up
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Up,
                position: (32.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Selection should remain
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("ell".to_string()));
    }

    #[test]
    fn test_drag_past_line_end_clamps_column() {
        // Click, drag past line end, verify column clamped
        let mut buffer = TextBuffer::from_str("hi\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at line 0, column 0
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
                position: (0.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Drag to x = 80 (would be column 10, but "hi" is only 2 chars)
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                position: (80.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Column should be clamped to 2 (end of "hi")
        assert_eq!(buffer.cursor_position(), Position::new(0, 2));
        assert_eq!(buffer.selected_text(), Some("hi".to_string()));
    }

    #[test]
    fn test_drag_below_last_line_clamps_to_last_line() {
        // Drag below buffer, verify line clamped
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at line 0, column 0
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
                position: (0.0, 155.0),
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Drag to y = 60 (flipped_y = 100, which would be line 6)
        // But buffer only has 2 lines
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                position: (0.0, 60.0), // flipped_y = 100, line 6 if existed
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Line should be clamped to last line (1)
        assert_eq!(buffer.cursor_position(), Position::new(1, 0));
    }

    #[test]
    fn test_drag_above_first_line_clamps_to_first_line() {
        // Drag with y that results in negative screen line (above view)
        let mut buffer = TextBuffer::from_str("hello\nworld");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at line 1
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
                position: (0.0, 140.0), // line 1
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Drag to y > view_height (below the coordinate origin, i.e., above the view)
        // NSView uses bottom-left origin, so y > view_height means above the top of the view
        // flipped_y = view_height - y would be negative
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            let event = MouseEvent {
                kind: MouseEventKind::Moved,
                position: (0.0, 200.0), // y > view_height, flipped_y < 0
                modifiers: Modifiers::default(),
            };
            target.handle_mouse(event, &mut ctx);
        }

        // Line should be clamped to first line (0)
        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_mouse_sequence_down_moved_up() {
        // Full lifecycle test: down → moved → moved → up
        let mut buffer = TextBuffer::from_str("hello\nworld\nfoo\nbar");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at line 0, column 1
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down,
                    position: (8.0, 155.0),
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        assert_eq!(buffer.cursor_position(), Position::new(0, 1));
        assert!(!buffer.has_selection());

        // Drag to line 1, column 2
        dirty = DirtyRegion::None;
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Moved,
                    position: (16.0, 140.0),
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        assert_eq!(buffer.cursor_position(), Position::new(1, 2));
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("ello\nwo".to_string()));
        assert!(dirty.is_dirty());

        // Drag to line 2, column 1
        dirty = DirtyRegion::None;
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Moved,
                    position: (8.0, 124.0),
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        assert_eq!(buffer.cursor_position(), Position::new(2, 1));
        assert_eq!(buffer.selected_text(), Some("ello\nworld\nf".to_string()));
        assert!(dirty.is_dirty());

        // Mouse up
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Up,
                    position: (8.0, 124.0),
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        // Selection should remain after mouse up
        assert!(buffer.has_selection());
        assert_eq!(buffer.selected_text(), Some("ello\nworld\nf".to_string()));
    }

    #[test]
    fn test_selection_range_during_drag() {
        // Verify selection_range() returns correct ordered range at each step
        let mut buffer = TextBuffer::from_str("hello");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at column 4
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down,
                    position: (32.0, 155.0),
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        // No selection yet (anchor == cursor)
        assert!(buffer.selection_range().is_none());

        // Drag backward to column 1 (backward selection)
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Moved,
                    position: (8.0, 155.0),
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        // selection_range() should return ordered (start, end)
        let range = buffer.selection_range().unwrap();
        assert_eq!(range.0, Position::new(0, 1)); // start
        assert_eq!(range.1, Position::new(0, 4)); // end

        // Selected text should be "ell"
        assert_eq!(buffer.selected_text(), Some("ell".to_string()));
    }

    #[test]
    fn test_drag_updates_dirty_region() {
        // Verify correct lines are marked dirty during drag
        let mut buffer = TextBuffer::from_str("line0\nline1\nline2\nline3");
        let mut viewport = Viewport::new(16.0);
        viewport.update_size(160.0);
        let mut dirty = DirtyRegion::None;
        let mut target = BufferFocusTarget::new();

        // Mouse down at line 1
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Down,
                    position: (0.0, 140.0), // line 1
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }
        assert_eq!(buffer.cursor_position(), Position::new(1, 0));

        // Reset dirty
        dirty = DirtyRegion::None;

        // Drag to line 3
        {
            let mut ctx = EditorContext::new(
                &mut buffer,
                &mut viewport,
                &mut dirty,
                test_font_metrics(),
                160.0,
            );
            target.handle_mouse(
                MouseEvent {
                    kind: MouseEventKind::Moved,
                    position: (16.0, 108.0), // flipped_y = 52, line 3
                    modifiers: Modifiers::default(),
                },
                &mut ctx,
            );
        }

        // Dirty region should cover lines 1 through 3 (the selection span)
        // DirtyRegion::Lines uses [from, to) half-open interval
        match dirty {
            DirtyRegion::Lines { from, to } => {
                assert!(from <= 1, "dirty from should include line 1");
                assert!(to > 3, "dirty to should be past line 3");
            }
            DirtyRegion::FullViewport => {
                // Also acceptable
            }
            DirtyRegion::None => {
                panic!("Expected dirty region after drag");
            }
        }
    }

    // ==================== Delete Backward Word Tests (Alt+Backspace) ====================
    // Chunk: docs/chunks/delete_backward_word - Alt+Backspace word deletion integration tests

    #[test]
    fn test_option_backspace_deletes_word() {
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 11)); // After "world"
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
                Key::Backspace,
                Modifiers {
                    option: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx)
        };

        assert_eq!(result, Handled::Yes);
        assert_eq!(buffer.content(), "hello ");
        assert_eq!(buffer.cursor_position(), Position::new(0, 6));
        assert!(dirty.is_dirty());
    }

    #[test]
    fn test_option_backspace_deletes_whitespace() {
        let mut buffer = TextBuffer::from_str("hello   ");
        buffer.set_cursor(Position::new(0, 8)); // After trailing spaces
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
                Key::Backspace,
                Modifiers {
                    option: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        assert_eq!(buffer.content(), "hello");
        assert_eq!(buffer.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_option_backspace_at_start_is_noop() {
        let mut buffer = TextBuffer::from_str("hello");
        // Cursor at start
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
                Key::Backspace,
                Modifiers {
                    option: true,
                    ..Default::default()
                },
            );
            target.handle_key(event, &mut ctx);
        }

        // Should be unchanged
        assert_eq!(buffer.content(), "hello");
        assert_eq!(buffer.cursor_position(), Position::new(0, 0));
    }

    #[test]
    fn test_plain_backspace_still_works() {
        // Ensure we didn't break plain backspace
        let mut buffer = TextBuffer::from_str("hello world");
        buffer.set_cursor(Position::new(0, 11)); // After "world"
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
            // Plain backspace - no modifiers
            let event = KeyEvent::new(Key::Backspace, Modifiers::default());
            target.handle_key(event, &mut ctx);
        }

        // Should only delete one character
        assert_eq!(buffer.content(), "hello worl");
        assert_eq!(buffer.cursor_position(), Position::new(0, 10));
    }
}
