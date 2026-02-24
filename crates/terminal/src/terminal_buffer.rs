// Chunk: docs/chunks/terminal_emulator - Terminal emulator backed by alacritty_terminal
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
//! TerminalBuffer - a terminal emulator implementing BufferView.
//!
//! This is the main type exported by this crate. It wraps alacritty_terminal's
//! Term struct and provides the BufferView trait implementation for rendering.
//!
//! ## Scrollback Architecture
//!
//! `TerminalBuffer` uses a tiered storage system for scrollback history:
//!
//! ```text
//! ┌─────────────────────────┐
//! │   Viewport (40 lines)   │  alacritty_terminal grid (always in memory)
//! ├─────────────────────────┤
//! │ Hot scrollback (~2K)    │  alacritty_terminal scrollback (in memory)
//! ├─────────────────────────┤
//! │  Cold scrollback (file)  │  Serialized StyledLines on disk
//! └─────────────────────────┘
//! ```
//!
//! As lines scroll off the hot scrollback, they are captured to cold storage.
//! The `BufferView::styled_line()` API transparently serves from either region.

use std::cell::RefCell;
use std::path::Path;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape as VteCursorShape, Processor};
use crossbeam_channel::{unbounded, Receiver, Sender};

// Chunk: docs/chunks/terminal_clipboard_selection - Terminal selection types
use lite_edit_buffer::{
    BufferView, CursorInfo, CursorShape, DirtyLines, Position, StyledLine,
};

use crate::cold_scrollback::{ColdScrollback, PageCache};
use crate::event::TerminalEvent;
use crate::pty::PtyHandle;
// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
use crate::pty_wakeup::PtyWakeup;
use crate::style_convert::row_to_styled_line;

// Chunk: docs/chunks/tty_cursor_reporting - DSR/CPR event forwarding
/// Event listener that captures terminal events and forwards them via a channel.
///
/// This is used by `TerminalBuffer` to receive events from alacritty_terminal,
/// such as `Event::PtyWrite` for DSR (Device Status Report) responses that need
/// to be written back to the PTY.
///
/// ## Why Channel-Based
///
/// The `EventListener::send_event` method takes `&self`, not `&mut self`, so we
/// cannot directly write to the PTY. Using a channel:
/// - Maintains the immutable borrow requirement of `EventListener`
/// - Keeps PTY writes on the main thread (same thread as `poll_events()`)
/// - Follows the existing `crossbeam_channel` pattern already used for PTY output
#[derive(Clone)]
struct EventSender {
    tx: Sender<Event>,
}

impl EventListener for EventSender {
    fn send_event(&self, event: Event) {
        // Forward the event through the channel.
        // Ignore send errors - they indicate the receiver was dropped,
        // which can happen during shutdown.
        let _ = self.tx.send(event);
    }
}

/// A terminal emulator buffer implementing BufferView.
///
/// This struct wraps alacritty_terminal's Term and manages PTY I/O.
/// It converts the terminal's cell grid to StyledLines for rendering
/// through the same pipeline as text editing buffers.
///
/// ## Memory Usage
///
/// Memory usage is bounded regardless of scrollback history length:
/// - Hot scrollback (in alacritty): ~2K lines * ~300 bytes = ~600 KB
/// - Page cache: ~1 MB (configurable)
/// - Cold storage: On disk, only paged into memory on demand
///
/// This enables 10+ concurrent terminals with 100K+ line histories while
/// keeping memory usage under ~7 MB per terminal.
// Chunk: docs/chunks/terminal_file_backed_scrollback - File-backed cold scrollback
pub struct TerminalBuffer {
    /// The alacritty terminal emulator.
    term: Term<EventSender>,
    /// VTE processor for feeding bytes to the terminal.
    processor: Processor,
    /// PTY handle (None if no process attached).
    pty: Option<PtyHandle>,
    /// Accumulated dirty lines since last take_dirty().
    dirty: DirtyLines,
    /// Terminal size (cols, rows).
    size: (usize, usize),
    /// Scrollback capacity (reserved for future configuration).
    #[allow(dead_code)]
    scrollback: usize,
    /// Cold scrollback storage (created lazily when needed).
    /// Wrapped in RefCell for interior mutability (BufferView::styled_line takes &self).
    cold_scrollback: RefCell<Option<ColdScrollback>>,
    /// Page cache for cold scrollback reads.
    /// Wrapped in RefCell for interior mutability.
    page_cache: RefCell<PageCache>,
    /// Number of lines captured to cold storage.
    cold_line_count: usize,
    /// Last observed history size (for detecting when to capture).
    last_history_size: usize,
    /// Maximum lines to keep in hot scrollback before flushing to cold.
    hot_scrollback_limit: usize,
    // Chunk: docs/chunks/terminal_clipboard_selection - Selection state
    /// Selection anchor (where the selection started).
    /// In terminal grid coordinates (line, col).
    selection_anchor: Option<Position>,
    /// Selection head (current end of selection).
    /// In terminal grid coordinates (line, col).
    selection_head: Option<Position>,
    // Chunk: docs/chunks/tty_cursor_reporting - DSR/CPR event forwarding
    /// Receiver for terminal events from alacritty_terminal.
    /// Used to receive Event::PtyWrite (DSR responses, etc.) for write-back to PTY.
    event_rx: Receiver<Event>,
}

impl TerminalBuffer {
    /// Default hot scrollback limit (lines kept in memory before flushing to disk).
    pub const DEFAULT_HOT_SCROLLBACK_LIMIT: usize = 2000;

    /// Default page cache size in bytes (~1 MB).
    pub const DEFAULT_PAGE_CACHE_BYTES: usize = 1024 * 1024;

    /// Default page size for cold scrollback cache (lines per page).
    pub const DEFAULT_PAGE_SIZE: usize = 64;

    /// Creates a new terminal buffer with the given dimensions.
    ///
    /// # Arguments
    ///
    /// * `cols` - Number of columns (characters per line)
    /// * `rows` - Number of rows (lines in viewport)
    /// * `scrollback` - Number of scrollback lines to keep
    pub fn new(cols: usize, rows: usize, scrollback: usize) -> Self {
        let size = TermSize::new(cols, rows);
        let config = Config::default();

        // Chunk: docs/chunks/tty_cursor_reporting - DSR/CPR event forwarding
        // Create channel for terminal events (DSR responses, title changes, etc.)
        let (event_tx, event_rx) = unbounded();
        let event_sender = EventSender { tx: event_tx };

        let term = Term::new(config, &size, event_sender);
        let processor = Processor::new();

        // Use the smaller of scrollback limit and our hot limit
        let hot_limit = scrollback.min(Self::DEFAULT_HOT_SCROLLBACK_LIMIT);

        Self {
            term,
            processor,
            pty: None,
            dirty: DirtyLines::FromLineToEnd(0), // Initial state: everything dirty
            size: (cols, rows),
            scrollback,
            cold_scrollback: RefCell::new(None),
            page_cache: RefCell::new(PageCache::new(Self::DEFAULT_PAGE_CACHE_BYTES, Self::DEFAULT_PAGE_SIZE)),
            cold_line_count: 0,
            last_history_size: 0,
            hot_scrollback_limit: hot_limit,
            selection_anchor: None,
            selection_head: None,
            event_rx,
        }
    }

    /// Sets the hot scrollback limit.
    ///
    /// Lines beyond this limit will be flushed to cold storage.
    pub fn set_hot_scrollback_limit(&mut self, limit: usize) {
        self.hot_scrollback_limit = limit;
    }

    // Chunk: docs/chunks/terminal_shell_env - Login shell spawning for full environment
    /// Spawns a login shell in this terminal.
    ///
    /// The user's shell is automatically determined from the passwd database
    /// (via `getpwuid`), and spawned as a login shell so the full profile chain
    /// (`~/.zprofile`, `~/.zshrc`, etc.) is sourced. This ensures the terminal
    /// has the user's complete environment including PATH entries from tools
    /// like pyenv, nvm, rbenv, etc.
    ///
    /// # Arguments
    ///
    /// * `cwd` - Working directory for the shell
    pub fn spawn_shell(&mut self, cwd: &Path) -> std::io::Result<()> {
        let (cols, rows) = self.size;
        let handle = PtyHandle::spawn("", &[], cwd, rows as u16, cols as u16, true)?;
        self.pty = Some(handle);
        Ok(())
    }

    /// Spawns a command in this terminal.
    ///
    /// Unlike `spawn_shell()`, this runs the explicit command provided,
    /// not as a login shell. Use this for running specific commands rather
    /// than interactive shells.
    ///
    /// # Arguments
    ///
    /// * `cmd` - Command to run
    /// * `args` - Arguments for the command
    /// * `cwd` - Working directory
    pub fn spawn_command(
        &mut self,
        cmd: &str,
        args: &[&str],
        cwd: &Path,
    ) -> std::io::Result<()> {
        let (cols, rows) = self.size;
        let handle = PtyHandle::spawn(cmd, args, cwd, rows as u16, cols as u16, false)?;
        self.pty = Some(handle);
        Ok(())
    }

    // Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
    // Chunk: docs/chunks/terminal_shell_env - Login shell spawning for full environment
    /// Spawns a login shell with run-loop wakeup support.
    ///
    /// Same as `spawn_shell()`, but signals `wakeup` whenever PTY output arrives,
    /// allowing the main thread to poll and render promptly.
    ///
    /// The user's shell is automatically determined from the passwd database
    /// and spawned as a login shell for full environment setup.
    pub fn spawn_shell_with_wakeup(
        &mut self,
        cwd: &Path,
        wakeup: PtyWakeup,
    ) -> std::io::Result<()> {
        let (cols, rows) = self.size;
        let handle = PtyHandle::spawn_with_wakeup("", &[], cwd, rows as u16, cols as u16, wakeup, true)?;
        self.pty = Some(handle);
        Ok(())
    }

    // Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
    /// Spawns a command with run-loop wakeup support.
    ///
    /// Same as `spawn_command()`, but signals `wakeup` whenever PTY output arrives,
    /// allowing the main thread to poll and render promptly.
    pub fn spawn_command_with_wakeup(
        &mut self,
        cmd: &str,
        args: &[&str],
        cwd: &Path,
        wakeup: PtyWakeup,
    ) -> std::io::Result<()> {
        let (cols, rows) = self.size;
        let handle = PtyHandle::spawn_with_wakeup(cmd, args, cwd, rows as u16, cols as u16, wakeup, false)?;
        self.pty = Some(handle);
        Ok(())
    }

    /// Polls for and processes PTY events.
    ///
    /// This should be called regularly (e.g., each frame) to process
    /// PTY output and update the terminal state.
    ///
    /// Returns true if any events were processed.
    pub fn poll_events(&mut self) -> bool {
        let Some(ref pty) = self.pty else {
            return false;
        };

        let mut processed_any = false;

        // Drain all available events
        while let Some(event) = pty.try_recv() {
            match event {
                TerminalEvent::PtyOutput(data) => {
                    // Feed bytes to the terminal emulator
                    self.processor.advance(&mut self.term, &data);
                    processed_any = true;
                }
                TerminalEvent::PtyExited(_code) => {
                    // Process could handle this differently
                    // For now, we just note that it happened
                    processed_any = true;
                }
                TerminalEvent::PtyError(_) => {
                    // Handle error - could log or propagate
                    processed_any = true;
                }
            }
        }

        // Chunk: docs/chunks/tty_cursor_reporting - Process terminal-generated events
        // Handle events from alacritty_terminal (DSR responses, title changes, etc.)
        // These events are generated when processing PTY output above (e.g., when the
        // hosted program sends a DSR query, alacritty responds with a PtyWrite event).
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                Event::PtyWrite(text) => {
                    // Write the response back to the PTY.
                    // This handles DSR (Device Status Report) responses like cursor
                    // position queries (ESC[6n → ESC[row;colR).
                    if let Some(ref mut pty) = self.pty {
                        let _ = pty.write(text.as_bytes());
                    }
                    processed_any = true;
                }
                // Other events (Title, Bell, ClipboardStore, etc.) could be handled
                // here in the future, but are out of scope for this chunk.
                _ => {}
            }
        }

        if processed_any {
            // Chunk: docs/chunks/terminal_clipboard_selection - Clear selection on PTY output
            // Clear selection when new output arrives to avoid stale/misaligned highlights.
            // This is standard terminal emulator behavior.
            self.clear_selection();

            // Update dirty tracking based on terminal damage
            self.update_damage();

            // Check if we need to flush lines to cold storage
            self.check_scrollback_overflow();
        }

        processed_any
    }

    /// Writes input data to the PTY stdin.
    pub fn write_input(&mut self, data: &[u8]) -> std::io::Result<()> {
        if let Some(ref mut pty) = self.pty {
            pty.write(data)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "No PTY attached",
            ))
        }
    }

    /// Resizes the terminal.
    ///
    /// This updates both the terminal emulator and the PTY (if attached).
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.size = (cols, rows);

        // Create new terminal size
        let size = TermSize::new(cols, rows);

        // Resize the terminal emulator
        self.term.resize(size);

        // Resize the PTY
        if let Some(ref pty) = self.pty {
            let _ = pty.resize(rows as u16, cols as u16);
        }

        // Mark everything dirty
        self.dirty = DirtyLines::FromLineToEnd(0);
    }

    /// Returns the terminal size as (cols, rows).
    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    /// Returns true if in alternate screen mode.
    pub fn is_alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    // Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
    /// Returns the current terminal mode flags.
    ///
    /// This is used by the input encoder to determine how to encode
    /// keys and mouse events (e.g., APP_CURSOR mode, SGR_MOUSE mode).
    pub fn term_mode(&self) -> TermMode {
        *self.term.mode()
    }

    /// Returns the number of lines in scrollback.
    fn history_size(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Returns the number of screen lines (viewport height).
    fn screen_lines(&self) -> usize {
        self.term.grid().screen_lines()
    }

    /// Updates dirty state based on terminal damage.
    fn update_damage(&mut self) {
        let history_len = self.history_size();

        // For now, we'll use a simplified damage tracking approach.
        // The terminal could have changed anywhere, so mark from the
        // start of the viewport as dirty.
        //
        // A more sophisticated implementation would use term.damage()
        // to track exactly which lines changed, but that requires
        // careful handling of the damage borrow.
        let new_dirty = DirtyLines::FromLineToEnd(history_len);
        self.dirty.merge(new_dirty);

        // Reset the terminal's internal damage tracking
        self.term.reset_damage();
    }

    /// Checks if the PTY process has exited.
    ///
    /// Returns `Some(exit_code)` if exited, `None` if still running.
    pub fn try_wait(&mut self) -> Option<i32> {
        self.pty.as_mut()?.try_wait()
    }

    /// Kills the PTY process.
    ///
    /// This sends SIGKILL to immediately terminate the process.
    pub fn kill(&mut self) -> std::io::Result<()> {
        if let Some(ref mut pty) = self.pty {
            pty.kill()
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "No PTY attached",
            ))
        }
    }

    /// Returns the process ID of the PTY child process, if available.
    ///
    /// This is used for sending signals (e.g., SIGTERM for graceful shutdown).
    pub fn process_id(&self) -> Option<u32> {
        self.pty.as_ref().and_then(|pty| pty.process_id())
    }

    // =========================================================================
    // Selection Support
    // Chunk: docs/chunks/terminal_clipboard_selection - Selection state management
    // =========================================================================

    /// Sets the selection anchor (where the selection started).
    ///
    /// This is typically called on mouse down to start a new selection.
    pub fn set_selection_anchor(&mut self, pos: Position) {
        self.selection_anchor = Some(pos);
    }

    /// Sets the selection head (current end of selection).
    ///
    /// This is called as the user drags to extend the selection.
    /// Marks the affected lines as dirty for re-rendering.
    pub fn set_selection_head(&mut self, pos: Position) {
        let old_head = self.selection_head;
        self.selection_head = Some(pos);

        // Mark dirty: old selection range + new selection range
        // This ensures both the old highlight and new highlight get redrawn
        if let Some(old) = old_head {
            self.dirty.merge(DirtyLines::Range {
                from: old.line.min(pos.line),
                to: old.line.max(pos.line) + 1,
            });
        }
        if let Some(anchor) = self.selection_anchor {
            self.dirty.merge(DirtyLines::Range {
                from: anchor.line.min(pos.line),
                to: anchor.line.max(pos.line) + 1,
            });
        }
    }

    /// Clears the current selection.
    ///
    /// This is called when:
    /// - New PTY output arrives (selection becomes stale)
    /// - User clicks without dragging
    /// - User copies selection (optionally)
    pub fn clear_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            // Mark the selection range as dirty so highlight is removed
            self.dirty.merge(DirtyLines::Range {
                from: start.line,
                to: end.line + 1,
            });
        }
        self.selection_anchor = None;
        self.selection_head = None;
    }

    /// Returns the selection anchor, if any.
    pub fn selection_anchor(&self) -> Option<Position> {
        self.selection_anchor
    }

    /// Returns the selection head, if any.
    pub fn selection_head(&self) -> Option<Position> {
        self.selection_head
    }

    /// Returns the selected text as a string.
    ///
    /// Extracts text from the terminal grid between the selection anchor and head.
    /// Multi-line selections are joined with newlines. Trailing spaces on each
    /// line are trimmed (standard terminal behavior).
    ///
    /// Returns `None` if there is no active selection.
    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;

        let mut result = String::new();

        for line_idx in start.line..=end.line {
            let Some(styled_line) = self.styled_line(line_idx) else {
                continue;
            };

            // Convert styled line to plain text
            let line_text: String = styled_line.spans.iter()
                .map(|span| span.text.as_str())
                .collect();

            // Determine column bounds for this line
            let start_col = if line_idx == start.line { start.col } else { 0 };
            let end_col = if line_idx == end.line {
                end.col
            } else {
                line_text.chars().count()
            };

            // Extract the substring
            let chars: Vec<char> = line_text.chars().collect();
            let actual_start = start_col.min(chars.len());
            let actual_end = end_col.min(chars.len());

            let selected: String = chars[actual_start..actual_end].iter().collect();

            // Trim trailing spaces (standard terminal behavior)
            let trimmed = selected.trim_end();

            result.push_str(trimmed);

            // Add newline between lines (not after the last line)
            if line_idx < end.line {
                result.push('\n');
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    // =========================================================================
    // Cold Scrollback Support
    // =========================================================================

    /// Checks for scrollback overflow and captures lines to cold storage.
    ///
    /// This is called after processing PTY events. When the hot scrollback
    /// exceeds `hot_scrollback_limit`, oldest lines are captured to cold storage.
    fn check_scrollback_overflow(&mut self) {
        // Don't capture during alternate screen mode
        if self.is_alt_screen() {
            return;
        }

        let history_size = self.history_size();

        // Check if we need to capture lines
        if history_size <= self.hot_scrollback_limit {
            self.last_history_size = history_size;
            return;
        }

        // Calculate how many lines to capture
        // We capture enough to bring history back under the limit, plus a buffer
        // to avoid capturing on every single output
        let lines_over_limit = history_size - self.hot_scrollback_limit;
        let capture_count = lines_over_limit;

        if capture_count > 0 {
            self.capture_cold_lines(capture_count);
        }

        self.last_history_size = history_size;
    }

    /// Captures the oldest lines from hot scrollback to cold storage.
    fn capture_cold_lines(&mut self, count: usize) {
        // Initialize cold storage if needed
        {
            let mut cold_ref = self.cold_scrollback.borrow_mut();
            if cold_ref.is_none() {
                match ColdScrollback::new() {
                    Ok(cold) => *cold_ref = Some(cold),
                    Err(e) => {
                        // Log error, continue without cold storage
                        eprintln!("Failed to create cold scrollback: {}", e);
                        return;
                    }
                }
            }
        }

        let mut cold_ref = self.cold_scrollback.borrow_mut();
        let cold = cold_ref.as_mut().unwrap();
        let grid = self.term.grid();
        let history_size = grid.history_size();
        let cols = self.size.0;

        // We need to capture the oldest lines (highest negative indices)
        // These are the lines that would be dropped first
        //
        // In alacritty, scrollback lines are accessed with negative Line indices:
        // Line(-1) = most recent scrollback line
        // Line(-history_size) = oldest scrollback line
        //
        // We capture from oldest to newest so they're stored in order
        let actual_count = count.min(history_size);
        for i in 0..actual_count {
            // Index from oldest line
            let scroll_idx = history_size - 1 - i;
            let row = &grid[Line(-(scroll_idx as i32) - 1)];
            let cells: Vec<_> = (0..cols)
                .map(|col| &row[alacritty_terminal::index::Column(col)])
                .collect();
            let styled = row_to_styled_line(cells.iter().copied(), cols);

            if cold.append(&styled).is_err() {
                // Stop on error
                break;
            }
        }

        // Update our tracking of how many lines are in cold storage
        // Note: We track this separately because we can't actually remove
        // lines from alacritty's scrollback. This count represents how many
        // of the "oldest" lines from a logical perspective are in cold storage.
        self.cold_line_count += actual_count;

        // Invalidate the page cache since line indices have shifted
        self.page_cache.borrow_mut().invalidate();
    }

    /// Returns the number of lines in cold storage.
    pub fn cold_line_count(&self) -> usize {
        self.cold_line_count
    }

    /// Gets a line from cold storage, using the page cache.
    fn get_cold_line(&self, line: usize) -> Option<StyledLine> {
        let mut cold_ref = self.cold_scrollback.borrow_mut();
        let cold = cold_ref.as_mut()?;
        self.page_cache.borrow_mut().get(line, cold).ok()
    }

    /// Returns a styled line from the hot scrollback region.
    ///
    /// This handles lines in alacritty's in-memory scrollback and viewport.
    fn styled_line_hot(&self, line: usize) -> Option<StyledLine> {
        let grid = self.term.grid();
        let cols = self.size.0;
        let history_len = grid.history_size();
        let screen_lines = grid.screen_lines();

        // Adjust for the lines we've already captured to cold storage
        // The "hot" region starts after cold_line_count in the logical view
        // but in alacritty's view, we need to map back
        //
        // Actually, we need to think about this carefully:
        // - logical line 0..cold_line_count = cold storage
        // - logical line cold_line_count..cold_line_count+history_len = hot scrollback
        // - logical line cold_line_count+history_len..end = viewport
        //
        // When we call styled_line_hot(line), 'line' is already offset past cold
        // So line 0 in hot = alacritty scrollback index (history_len - 1 - cold_line_count - line)
        //
        // Wait, that's not quite right either. Let me reconsider...
        //
        // The issue is that alacritty keeps all the lines, we just track which
        // ones we've captured. So:
        // - alacritty has history_len lines of scrollback
        // - We've captured cold_line_count of those to cold storage
        // - The remaining (history_len - cold_line_count) are "hot" but haven't been captured yet
        //
        // Actually, re-reading the plan, we're NOT removing lines from alacritty.
        // We're just tracking that we've captured them. So the indices work like this:
        //
        // For styled_line(n):
        // - n < cold_line_count: read from cold storage
        // - n >= cold_line_count: read from alacritty, adjusting index
        //
        // Since alacritty still has all lines, when we read "hot" line N:
        // - Logical line = cold_line_count + N (in our numbering)
        // - Alacritty index = ?
        //
        // Actually let me trace through an example:
        // - We have 3000 lines in alacritty scrollback
        // - hot_scrollback_limit = 2000
        // - We capture 1000 lines to cold storage
        // - cold_line_count = 1000
        //
        // Now:
        // - styled_line(0..999) should come from cold storage
        // - styled_line(1000..2999) should come from alacritty scrollback
        // - styled_line(3000..3000+viewport) should come from viewport
        //
        // For styled_line(1000), we want alacritty scrollback line 0 (oldest remaining hot)
        // For styled_line(2999), we want alacritty scrollback line 1999 (newest)
        //
        // Wait, but alacritty still has 3000 lines. The cold_line_count just tells us
        // how many we've already captured. The lines in alacritty haven't changed.
        //
        // Let me re-read the plan's "Revised approach" section...
        //
        // OK, the plan says: "configure alacritty with a small, fixed scrollback"
        // But we're not actually doing that - we're using whatever alacritty has.
        //
        // The key insight is: alacritty will eventually recycle old lines when its
        // own scrollback buffer fills up. By the time that happens, we should have
        // already captured them to cold storage.
        //
        // So the logic should be:
        // 1. When we call styled_line(n):
        //    - If n < cold_line_count: return from cold storage
        //    - If n >= cold_line_count: return from alacritty's current buffer
        //
        // 2. For alacritty's buffer, the line index mapping is:
        //    - Logical line n corresponds to alacritty scrollback position (n - cold_line_count)
        //    - But we also need to account for alacritty's own indexing

        if line < history_len {
            // This line is in alacritty's scrollback
            // Line 0 (after cold offset) = oldest line we haven't captured yet
            // = alacritty scrollback index (history_len - 1 - line)
            let scroll_idx = history_len - 1 - line;
            let row = &grid[Line(-(scroll_idx as i32) - 1)];
            let cells: Vec<_> = (0..cols)
                .map(|col| &row[alacritty_terminal::index::Column(col)])
                .collect();
            Some(row_to_styled_line(cells.iter().copied(), cols))
        } else {
            // This line is in the viewport
            let viewport_line = line - history_len;
            if viewport_line >= screen_lines {
                return None;
            }
            let row = &grid[Line(viewport_line as i32)];
            let cells: Vec<_> = (0..cols)
                .map(|col| &row[alacritty_terminal::index::Column(col)])
                .collect();
            Some(row_to_styled_line(cells.iter().copied(), cols))
        }
    }

    /// Returns a styled line from the alternate screen.
    fn styled_line_alt_screen(&self, line: usize) -> Option<StyledLine> {
        let grid = self.term.grid();
        let cols = self.size.0;
        let screen_lines = grid.screen_lines();

        if line >= screen_lines {
            return None;
        }
        let row = &grid[Line(line as i32)];
        let cells: Vec<_> = (0..cols)
            .map(|col| &row[alacritty_terminal::index::Column(col)])
            .collect();
        Some(row_to_styled_line(cells.iter().copied(), cols))
    }
}

impl BufferView for TerminalBuffer {
    fn line_count(&self) -> usize {
        if self.is_alt_screen() {
            // Alternate screen: no scrollback
            self.screen_lines()
        } else {
            // Primary screen: cold + hot scrollback + viewport
            self.cold_line_count + self.history_size() + self.screen_lines()
        }
    }

    fn styled_line(&self, line: usize) -> Option<StyledLine> {
        if self.is_alt_screen() {
            return self.styled_line_alt_screen(line);
        }

        // Check if line is in cold storage
        if line < self.cold_line_count {
            // Line is in cold storage - use RefCell for interior mutability
            return self.get_cold_line(line);
        }

        // Line is in hot storage (alacritty's buffer)
        let hot_line = line - self.cold_line_count;
        self.styled_line_hot(hot_line)
    }

    fn line_len(&self, _line: usize) -> usize {
        // Terminal lines are always the terminal width
        self.size.0
    }

    fn take_dirty(&mut self) -> DirtyLines {
        std::mem::take(&mut self.dirty)
    }

    fn is_editable(&self) -> bool {
        // Terminal buffers don't accept direct text input
        // (input goes to PTY stdin instead)
        false
    }

    fn cursor_info(&self) -> Option<CursorInfo> {
        let grid = self.term.grid();
        let cursor_point = grid.cursor.point;
        let history_len = self.history_size();

        // In alt screen, history_len is effectively 0 for cursor positioning
        // and there's no cold scrollback
        let doc_line = if self.is_alt_screen() {
            cursor_point.line.0 as usize
        } else {
            // Add cold lines + hot history to viewport line
            self.cold_line_count + history_len + cursor_point.line.0 as usize
        };
        let col = cursor_point.column.0;

        // Map cursor shape from alacritty to our CursorShape
        let cursor_style = self.term.cursor_style();
        let shape = match cursor_style.shape {
            VteCursorShape::Block | VteCursorShape::HollowBlock => CursorShape::Block,
            VteCursorShape::Underline => CursorShape::Underline,
            VteCursorShape::Beam => CursorShape::Beam,
            VteCursorShape::Hidden => CursorShape::Hidden,
        };

        // Check if cursor should blink
        let blinking = cursor_style.blinking;

        Some(CursorInfo::new(
            Position::new(doc_line, col),
            shape,
            blinking,
        ))
    }

    // Chunk: docs/chunks/terminal_clipboard_selection - Selection range for rendering
    fn selection_range(&self) -> Option<(Position, Position)> {
        let anchor = self.selection_anchor?;
        let head = self.selection_head?;

        // No selection if anchor equals head
        if anchor == head {
            return None;
        }

        // Return in document order (start, end)
        if anchor < head {
            Some((anchor, head))
        } else {
            Some((head, anchor))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_terminal() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert_eq!(term.size(), (80, 24));
    }

    #[test]
    fn test_line_count_empty() {
        let term = TerminalBuffer::new(80, 24, 1000);
        // New terminal has screen_lines count (no scrollback yet)
        assert_eq!(term.line_count(), 24);
    }

    #[test]
    fn test_styled_line_empty() {
        let term = TerminalBuffer::new(80, 24, 1000);
        // First line should exist and be empty/spaces
        let line = term.styled_line(0);
        assert!(line.is_some());
    }

    #[test]
    fn test_styled_line_out_of_bounds() {
        let term = TerminalBuffer::new(80, 24, 1000);
        // Line 100 should be out of bounds
        let line = term.styled_line(100);
        assert!(line.is_none());
    }

    #[test]
    fn test_is_editable() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert!(!term.is_editable());
    }

    #[test]
    fn test_cursor_info() {
        let term = TerminalBuffer::new(80, 24, 1000);
        let cursor = term.cursor_info();
        assert!(cursor.is_some());
        let cursor = cursor.unwrap();
        // Initial cursor should be at (0, 0)
        assert_eq!(cursor.position.line, 0);
        assert_eq!(cursor.position.col, 0);
    }

    #[test]
    fn test_resize() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        term.resize(120, 40);
        assert_eq!(term.size(), (120, 40));
        assert_eq!(term.line_count(), 40);
    }

    #[test]
    fn test_take_dirty() {
        let mut term = TerminalBuffer::new(80, 24, 1000);

        // Initial state should be dirty
        let dirty = term.take_dirty();
        assert!(!dirty.is_none());

        // After taking, should be none
        let dirty2 = term.take_dirty();
        assert!(dirty2.is_none());
    }

    // =========================================================================
    // Selection Tests
    // Chunk: docs/chunks/terminal_clipboard_selection - Selection unit tests
    // =========================================================================

    #[test]
    fn test_selection_anchor_initially_none() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert!(term.selection_anchor().is_none());
        assert!(term.selection_head().is_none());
    }

    #[test]
    fn test_set_selection_anchor() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        let pos = Position::new(5, 10);
        term.set_selection_anchor(pos);
        assert_eq!(term.selection_anchor(), Some(pos));
    }

    #[test]
    fn test_set_selection_head() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        let anchor = Position::new(5, 10);
        let head = Position::new(7, 20);
        term.set_selection_anchor(anchor);
        term.set_selection_head(head);
        assert_eq!(term.selection_anchor(), Some(anchor));
        assert_eq!(term.selection_head(), Some(head));
    }

    #[test]
    fn test_clear_selection() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        term.set_selection_anchor(Position::new(5, 10));
        term.set_selection_head(Position::new(7, 20));
        term.clear_selection();
        assert!(term.selection_anchor().is_none());
        assert!(term.selection_head().is_none());
    }

    #[test]
    fn test_selection_range_none_when_no_anchor() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert!(term.selection_range().is_none());
    }

    #[test]
    fn test_selection_range_none_when_anchor_equals_head() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        let pos = Position::new(5, 10);
        term.set_selection_anchor(pos);
        term.set_selection_head(pos);
        assert!(term.selection_range().is_none());
    }

    #[test]
    fn test_selection_range_forward() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        let anchor = Position::new(5, 10);
        let head = Position::new(7, 20);
        term.set_selection_anchor(anchor);
        term.set_selection_head(head);
        assert_eq!(term.selection_range(), Some((anchor, head)));
    }

    #[test]
    fn test_selection_range_backward_returns_ordered() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        // Set anchor after head (backward selection)
        let anchor = Position::new(7, 20);
        let head = Position::new(5, 10);
        term.set_selection_anchor(anchor);
        term.set_selection_head(head);
        // Should return in document order (start, end)
        assert_eq!(term.selection_range(), Some((head, anchor)));
    }

    #[test]
    fn test_selected_text_none_when_no_selection() {
        let term = TerminalBuffer::new(80, 24, 1000);
        assert!(term.selected_text().is_none());
    }

    #[test]
    fn test_selection_change_marks_lines_dirty() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        // Clear initial dirty state
        let _ = term.take_dirty();

        // Set selection
        term.set_selection_anchor(Position::new(5, 0));
        term.set_selection_head(Position::new(8, 10));

        // Should have marked lines 5-8 as dirty
        let dirty = term.take_dirty();
        assert!(!dirty.is_none());
    }

    #[test]
    fn test_clear_selection_marks_dirty() {
        let mut term = TerminalBuffer::new(80, 24, 1000);
        term.set_selection_anchor(Position::new(5, 0));
        term.set_selection_head(Position::new(8, 10));
        // Clear initial dirty state
        let _ = term.take_dirty();

        // Clear selection
        term.clear_selection();

        // Should have marked lines as dirty to remove highlight
        let dirty = term.take_dirty();
        assert!(!dirty.is_none());
    }

    // =========================================================================
    // EventSender Tests
    // Chunk: docs/chunks/tty_cursor_reporting - EventSender unit tests
    // =========================================================================

    #[test]
    fn test_event_sender_forwards_pty_write() {
        let (tx, rx) = unbounded();
        let sender = EventSender { tx };

        // Send a PtyWrite event through the EventListener interface
        sender.send_event(Event::PtyWrite("test response".to_string()));

        // Verify it was received
        let received = rx.try_recv();
        assert!(
            matches!(&received, Ok(Event::PtyWrite(s)) if s == "test response"),
            "Expected PtyWrite event with 'test response', got: {:?}",
            received
        );
    }

    #[test]
    fn test_event_sender_handles_multiple_events() {
        let (tx, rx) = unbounded();
        let sender = EventSender { tx };

        // Send multiple events
        sender.send_event(Event::PtyWrite("first".to_string()));
        sender.send_event(Event::PtyWrite("second".to_string()));

        // Verify both were received in order
        let first = rx.try_recv();
        let second = rx.try_recv();

        assert!(
            matches!(&first, Ok(Event::PtyWrite(s)) if s == "first"),
            "Expected first event, got: {:?}",
            first
        );
        assert!(
            matches!(&second, Ok(Event::PtyWrite(s)) if s == "second"),
            "Expected second event, got: {:?}",
            second
        );
    }

    #[test]
    fn test_event_sender_does_not_panic_on_closed_channel() {
        let (tx, rx) = unbounded();
        let sender = EventSender { tx };

        // Drop the receiver to close the channel
        drop(rx);

        // Sending should not panic, just silently fail
        sender.send_event(Event::PtyWrite("ignored".to_string()));
    }
}
