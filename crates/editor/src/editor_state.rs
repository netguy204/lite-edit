// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/viewport_scrolling - Scroll event handling
// Chunk: docs/chunks/quit_command - Cmd+Q quit flag and key interception
// Chunk: docs/chunks/file_picker - File picker (Cmd+P) integration
// Chunk: docs/chunks/file_save - File-buffer association and Cmd+S save
// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
//!
//! Editor state container.
//!
//! This module consolidates all mutable editor state into a single struct
//! that the main loop can work with. It provides the EditorContext for
//! focus target event handling.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::buffer_target::BufferFocusTarget;
use crate::context::EditorContext;
use crate::dirty_region::DirtyRegion;
use crate::file_index::FileIndex;
use crate::focus::FocusTarget;
use crate::font::FontMetrics;
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};
use crate::left_rail::{calculate_left_rail_geometry, RAIL_WIDTH};
use crate::mini_buffer::MiniBuffer;
// Chunk: docs/chunks/content_tab_bar - Tab bar click handling
use crate::tab_bar::{calculate_tab_bar_geometry, tabs_from_workspace, TAB_BAR_HEIGHT};
use crate::selector::{SelectorOutcome, SelectorWidget};
use crate::selector_overlay::calculate_overlay_geometry;
use crate::viewport::Viewport;
use crate::workspace::Editor;
use lite_edit_buffer::{Position, TextBuffer};

/// Duration in milliseconds for cursor blink interval
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// Which UI element currently owns keyboard/mouse focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorFocus {
    /// Normal buffer editing mode
    #[default]
    Buffer,
    /// Selector overlay is active (file picker, command palette, etc.)
    Selector,
    /// Find-in-file strip is active
    FindInFile,
}

/// Consolidated editor state.
///
/// This struct holds all mutable state that the main loop needs:
/// - The workspace/tab model (Editor) containing buffers and viewports
/// - The active focus target
/// - Cursor visibility state
/// - Dirty region tracking
/// - Font metrics for pixel-to-position conversion
/// - Application quit flag
/// - File picker state (focus, selector widget, file index)
///
/// The `buffer`, `viewport`, and `associated_file` are now accessed through
/// delegate methods that forward to the active workspace's active tab.
pub struct EditorState {
    /// The workspace/tab model containing all buffers and viewports
    pub editor: Editor,
    /// Accumulated dirty region for the current event batch
    pub dirty_region: DirtyRegion,
    /// The active focus target (currently always the buffer target)
    pub focus_target: BufferFocusTarget,
    /// Whether the cursor is currently visible (for blink animation)
    pub cursor_visible: bool,
    /// Time of the last keystroke (for cursor blink reset)
    pub last_keystroke: Instant,
    /// Font metrics for pixel-to-position conversion
    font_metrics: FontMetrics,
    /// View height in pixels (for y-coordinate flipping in mouse events)
    view_height: f32,
    /// View width in pixels (for selector overlay geometry)
    view_width: f32,
    /// Whether the app should quit (set by Cmd+Q)
    pub should_quit: bool,
    /// Which UI element currently owns focus
    pub focus: EditorFocus,
    /// The active selector widget (when focus == Selector)
    pub active_selector: Option<SelectorWidget>,
    /// The file index for fuzzy file matching
    file_index: Option<FileIndex>,
    /// The cache version at the last query (for streaming refresh)
    last_cache_version: u64,
    /// The resolved path from the last selector confirmation
    /// (consumed by file_save chunk for buffer association)
    pub resolved_path: Option<PathBuf>,
    /// The MiniBuffer for the find query (when focus == FindInFile)
    pub find_mini_buffer: Option<MiniBuffer>,
    /// The buffer position from which the current search started
    /// (used as the search origin; only advances when Enter is pressed)
    pub search_origin: Position,
}

// =============================================================================
// Delegate accessors for backward compatibility
// =============================================================================

impl EditorState {
    /// Returns a reference to the active tab's buffer.
    ///
    /// # Panics
    /// Panics if there is no active workspace or active tab (should never happen
    /// as EditorState always creates at least one workspace with one tab).
    pub fn buffer(&self) -> &TextBuffer {
        self.editor
            .active_workspace()
            .expect("no active workspace")
            .active_tab()
            .expect("no active tab")
            .as_text_buffer()
            .expect("active tab is not a file tab")
    }

    /// Returns a mutable reference to the active tab's buffer.
    ///
    /// # Panics
    /// Panics if there is no active workspace or active tab.
    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        self.editor
            .active_workspace_mut()
            .expect("no active workspace")
            .active_tab_mut()
            .expect("no active tab")
            .as_text_buffer_mut()
            .expect("active tab is not a file tab")
    }

    /// Returns a reference to the active tab's viewport.
    ///
    /// # Panics
    /// Panics if there is no active workspace or active tab.
    pub fn viewport(&self) -> &Viewport {
        &self.editor
            .active_workspace()
            .expect("no active workspace")
            .active_tab()
            .expect("no active tab")
            .viewport
    }

    /// Returns a mutable reference to the active tab's viewport.
    ///
    /// # Panics
    /// Panics if there is no active workspace or active tab.
    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.editor
            .active_workspace_mut()
            .expect("no active workspace")
            .active_tab_mut()
            .expect("no active tab")
            .viewport
    }

    /// Returns a reference to the active tab's associated file path.
    pub fn associated_file(&self) -> Option<&PathBuf> {
        self.editor
            .active_workspace()
            .and_then(|ws| ws.active_tab())
            .and_then(|tab| tab.associated_file.as_ref())
    }

    /// Sets the associated file for the active tab.
    fn set_associated_file(&mut self, path: Option<PathBuf>) {
        if let Some(tab) = self.editor
            .active_workspace_mut()
            .and_then(|ws| ws.active_tab_mut())
        {
            tab.associated_file = path;
        }
    }
}

// =============================================================================
// Core EditorState methods
// =============================================================================

impl EditorState {
    /// Creates a new EditorState with the given buffer and font metrics.
    ///
    /// # Arguments
    /// * `buffer` - The text buffer being edited
    /// * `font_metrics` - Font metrics for pixel-to-position conversion
    ///
    /// Note: `view_width` defaults to 10000.0 to avoid unintended line wrapping in tests.
    /// Call `update_viewport_dimensions` to set the actual width for wrap calculations.
    pub fn new(buffer: TextBuffer, font_metrics: FontMetrics) -> Self {
        let line_height = font_metrics.line_height as f32;

        // Create editor with one workspace
        let mut editor = Editor::new(line_height);

        // Replace the empty buffer in the first workspace's first tab with the provided buffer
        if let Some(ws) = editor.active_workspace_mut() {
            if let Some(tab) = ws.active_tab_mut() {
                if let Some(text_buf) = tab.as_text_buffer_mut() {
                    *text_buf = buffer;
                }
            }
        }

        Self {
            editor,
            dirty_region: DirtyRegion::None,
            focus_target: BufferFocusTarget::new(),
            cursor_visible: true,
            last_keystroke: Instant::now(),
            font_metrics,
            view_height: 0.0,
            // Default to a large width to prevent unintended wrapping in tests
            // Chunk: docs/chunks/line_wrap_rendering - Large default to avoid test breakage
            view_width: 10000.0,
            should_quit: false,
            focus: EditorFocus::Buffer,
            active_selector: None,
            file_index: None,
            last_cache_version: 0,
            resolved_path: None,
            find_mini_buffer: None,
            search_origin: Position::new(0, 0),
        }
    }

    /// Creates an EditorState with an empty buffer.
    pub fn empty(font_metrics: FontMetrics) -> Self {
        Self::new(TextBuffer::new(), font_metrics)
    }

    /// Returns the font metrics.
    pub fn font_metrics(&self) -> &FontMetrics {
        &self.font_metrics
    }

    /// Updates the viewport size based on window dimensions in pixels.
    ///
    /// This also updates the stored view_height and view_width for
    /// mouse event coordinate flipping and selector overlay geometry.
    pub fn update_viewport_size(&mut self, window_height: f32) {
        self.viewport_mut().update_size(window_height);
        self.view_height = window_height;
    }

    /// Updates the viewport size with both width and height.
    ///
    /// This is the preferred method when both dimensions are available.
    pub fn update_viewport_dimensions(&mut self, window_width: f32, window_height: f32) {
        self.viewport_mut().update_size(window_height);
        self.view_height = window_height;
        self.view_width = window_width;
    }

    /// Handles a key event by forwarding to the active focus target.
    ///
    /// This records the keystroke time (for cursor blink reset) and
    /// ensures the cursor is visible after any keystroke.
    ///
    /// App-level shortcuts (like Cmd+Q for quit, Cmd+P for file picker) are
    /// intercepted here before being forwarded to the focus target.
    ///
    /// If the cursor has been scrolled off-screen, we snap the viewport back
    /// to make the cursor visible BEFORE processing the keystroke.
    pub fn handle_key(&mut self, event: KeyEvent) {
        use crate::input::Key;

        // Check for app-level shortcuts before delegating to focus target
        // Cmd+Q (without Ctrl) triggers quit
        if event.modifiers.command && !event.modifiers.control {
            if let Key::Char('q') = event.key {
                self.should_quit = true;
                return;
            }

            // Cmd+P (without Ctrl) toggles file picker
            if let Key::Char('p') = event.key {
                self.handle_cmd_p();
                return;
            }

            // Cmd+S (without Ctrl) saves the current file
            if let Key::Char('s') = event.key {
                self.save_file();
                return;
            }

            // Cmd+F (without Ctrl) opens find-in-file
            if let Key::Char('f') = event.key {
                self.handle_cmd_f();
                return;
            }

            // Cmd+N (without Shift) creates a new workspace
            if let Key::Char('n') = event.key {
                if !event.modifiers.shift {
                    self.new_workspace();
                    return;
                }
            }

            // Cmd+W closes the active tab, Cmd+Shift+W closes the active workspace
            if let Key::Char('w') = event.key {
                if event.modifiers.shift {
                    self.close_active_workspace();
                    return;
                } else {
                    // Chunk: docs/chunks/content_tab_bar - Close active tab
                    self.close_active_tab();
                    return;
                }
            }

            // Chunk: docs/chunks/content_tab_bar - Tab cycling shortcuts
            // Cmd+Shift+] switches to next tab
            if let Key::Char(']') = event.key {
                if event.modifiers.shift {
                    self.next_tab();
                    return;
                }
            }

            // Cmd+Shift+[ switches to previous tab
            if let Key::Char('[') = event.key {
                if event.modifiers.shift {
                    self.prev_tab();
                    return;
                }
            }

            // Chunk: docs/chunks/content_tab_bar - Create new tab
            // Cmd+T creates a new empty tab in the current workspace
            // Note: Creates an empty file tab; terminal tab support is in terminal_emulator chunk
            if let Key::Char('t') = event.key {
                if !event.modifiers.shift {
                    self.new_tab();
                    return;
                }
            }

            // Cmd+1..9 switches workspaces
            if let Key::Char(c) = event.key {
                if !event.modifiers.shift {
                    if let Some(digit) = c.to_digit(10) {
                        if digit >= 1 && digit <= 9 {
                            let idx = (digit - 1) as usize;
                            self.switch_workspace(idx);
                            return;
                        }
                    }
                }
            }
        }

        // Route based on current focus
        match self.focus {
            EditorFocus::Selector => {
                self.handle_key_selector(event);
            }
            EditorFocus::Buffer => {
                self.handle_key_buffer(event);
            }
            EditorFocus::FindInFile => {
                self.handle_key_find(event);
            }
        }
    }

    /// Handles Cmd+P to toggle the file picker.
    fn handle_cmd_p(&mut self) {
        match self.focus {
            EditorFocus::Buffer => {
                // Open the file picker
                self.open_file_picker();
            }
            EditorFocus::Selector => {
                // Close the file picker (toggle behavior)
                self.close_selector();
            }
            EditorFocus::FindInFile => {
                // Don't open file picker while find is active
            }
        }
    }

    /// Opens the file picker selector.
    fn open_file_picker(&mut self) {
        // Get the current working directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Initialize file_index if needed
        if self.file_index.is_none() {
            self.file_index = Some(FileIndex::start(cwd.clone()));
        }

        // Query with empty string to get initial results
        let results = self.file_index.as_ref().unwrap().query("");

        // Create a new selector widget
        let mut selector = SelectorWidget::new();

        // Map results to display strings
        let items: Vec<String> = results
            .iter()
            .map(|r| r.path.display().to_string())
            .collect();
        selector.set_items(items);

        // Store the selector and update focus
        self.active_selector = Some(selector);
        self.focus = EditorFocus::Selector;
        self.last_cache_version = self.file_index.as_ref().unwrap().cache_version();

        // Mark full viewport dirty for overlay rendering
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Closes the active selector.
    fn close_selector(&mut self) {
        self.active_selector = None;
        self.focus = EditorFocus::Buffer;
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    // =========================================================================
    // Find-in-File (Chunk: docs/chunks/find_in_file)
    // =========================================================================

    /// Handles Cmd+F to open the find strip.
    ///
    /// - If `focus == Buffer`: creates a new `MiniBuffer`, records the cursor
    ///   position as `search_origin`, transitions to `FindInFile`, marks dirty.
    /// - If `focus == FindInFile`: no-op (does not close or reset).
    /// - If `focus == Selector`: no-op (don't open find while file picker is open).
    fn handle_cmd_f(&mut self) {
        match self.focus {
            EditorFocus::Buffer => {
                // Record cursor position as search origin
                self.search_origin = self.buffer().cursor_position();

                // Create a new MiniBuffer for the find query
                self.find_mini_buffer = Some(MiniBuffer::new(self.font_metrics));

                // Transition focus
                self.focus = EditorFocus::FindInFile;

                // Mark full viewport dirty for overlay rendering
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            EditorFocus::FindInFile => {
                // No-op: Cmd+F while open does nothing
            }
            EditorFocus::Selector => {
                // No-op: don't open find while file picker is open
            }
        }
    }

    /// Closes the find-in-file strip.
    ///
    /// Clears the `find_mini_buffer`, resets focus to `Buffer`, and marks dirty.
    /// Leaves the main buffer's cursor and selection at their current positions
    /// (the last match position).
    fn close_find_strip(&mut self) {
        self.find_mini_buffer = None;
        self.focus = EditorFocus::Buffer;
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Finds the next match for the query starting from start_pos.
    ///
    /// Performs a case-insensitive substring search. If no match is found
    /// forward from start_pos, wraps around to the beginning of the buffer.
    ///
    /// # Arguments
    /// * `buffer` - The text buffer to search in
    /// * `query` - The search query string
    /// * `start_pos` - The position to start searching from
    ///
    /// # Returns
    /// * `Some((start, end))` - The match range as (start position, end position)
    /// * `None` - If query is empty or no match was found
    fn find_next_match(
        buffer: &TextBuffer,
        query: &str,
        start_pos: Position,
    ) -> Option<(Position, Position)> {
        if query.is_empty() {
            return None;
        }

        let content = buffer.content();
        let query_lower = query.to_lowercase();

        // Convert start_pos to byte offset
        let start_byte = Self::position_to_byte_offset(buffer, start_pos);

        // Search forward from start_byte
        let search_content = content.to_lowercase();

        // First, search from start_byte to end
        if let Some(rel_offset) = search_content[start_byte..].find(&query_lower) {
            let match_start = start_byte + rel_offset;
            let match_end = match_start + query.len();
            let start = Self::byte_offset_to_position(buffer, match_start);
            let end = Self::byte_offset_to_position(buffer, match_end);
            return Some((start, end));
        }

        // Wrap around: search from beginning to start_byte
        if start_byte > 0 {
            if let Some(match_start) = search_content[..start_byte].find(&query_lower) {
                let match_end = match_start + query.len();
                let start = Self::byte_offset_to_position(buffer, match_start);
                let end = Self::byte_offset_to_position(buffer, match_end);
                return Some((start, end));
            }
        }

        None
    }

    /// Converts a Position (line, col) to a byte offset in the buffer content.
    fn position_to_byte_offset(buffer: &TextBuffer, pos: Position) -> usize {
        let content = buffer.content();
        let mut byte_offset = 0;
        let mut current_line = 0;

        for (idx, ch) in content.char_indices() {
            if current_line == pos.line {
                // We're on the target line, count columns
                let mut col = 0;
                for (sub_idx, sub_ch) in content[idx..].char_indices() {
                    if col == pos.col {
                        return idx + sub_idx;
                    }
                    if sub_ch == '\n' {
                        // Reached end of line before finding column
                        return idx + sub_idx;
                    }
                    col += 1;
                }
                // Column is past end of line
                return content.len();
            }
            if ch == '\n' {
                current_line += 1;
            }
            byte_offset = idx + ch.len_utf8();
        }

        byte_offset.min(content.len())
    }

    /// Converts a byte offset in the buffer content to a Position (line, col).
    fn byte_offset_to_position(buffer: &TextBuffer, byte_offset: usize) -> Position {
        let content = buffer.content();
        let mut line = 0;
        let mut col = 0;
        let mut current_offset = 0;

        for ch in content.chars() {
            if current_offset >= byte_offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
            current_offset += ch.len_utf8();
        }

        Position::new(line, col)
    }

    /// Handles a key event when focus == FindInFile.
    ///
    /// Key routing:
    /// - Escape → close the find strip
    /// - Return → advance search_origin past current match, re-run search
    /// - All other keys → delegate to find_mini_buffer.handle_key(), then
    ///   if content changed, run live search
    fn handle_key_find(&mut self, event: KeyEvent) {
        use crate::input::Key;

        match &event.key {
            Key::Escape => {
                self.close_find_strip();
                return;
            }
            Key::Return => {
                // Advance to next match: move search_origin past the current match
                self.advance_to_next_match();
                return;
            }
            _ => {
                // Delegate to mini buffer and run live search on content change
                if let Some(ref mut mini_buffer) = self.find_mini_buffer {
                    let prev_content = mini_buffer.content();
                    mini_buffer.handle_key(event);
                    let new_content = mini_buffer.content();

                    // If content changed, run live search
                    if prev_content != new_content {
                        self.run_live_search();
                    }

                    // Mark dirty for any visual update
                    self.dirty_region.merge(DirtyRegion::FullViewport);
                }
            }
        }
    }

    /// Runs the live search and updates the buffer selection.
    ///
    /// Called after every key event that changes the minibuffer's content.
    fn run_live_search(&mut self) {
        let query = match &self.find_mini_buffer {
            Some(mb) => mb.content(),
            None => return,
        };

        // Perform the search
        let buffer = self.buffer();
        let search_origin = self.search_origin;
        #[cfg(test)]
        eprintln!("run_live_search: query={:?}, search_origin={:?}, buffer_content={:?}",
            query, search_origin, buffer.content());
        let match_result = Self::find_next_match(buffer, &query, search_origin);
        #[cfg(test)]
        eprintln!("run_live_search: match_result={:?}", match_result);

        // Now update the buffer based on the result
        match match_result {
            Some((start, end)) => {
                #[cfg(test)]
                eprintln!("run_live_search: Setting selection from {:?} to {:?}", start, end);
                // Set the buffer selection to cover the match range
                // Note: set_cursor clears the selection anchor, so we must call
                // set_selection_anchor AFTER set_cursor
                self.buffer_mut().set_cursor(end);
                self.buffer_mut().set_selection_anchor(start);
                #[cfg(test)]
                eprintln!("run_live_search: After setting selection, selection_range={:?}", self.buffer().selection_range());

                // Scroll viewport to make match visible
                let line_count = self.buffer().line_count();
                let match_line = start.line;
                if self.viewport_mut().ensure_visible(match_line, line_count) {
                    self.dirty_region.merge(DirtyRegion::FullViewport);
                }
            }
            None => {
                // No match: clear the selection
                self.buffer_mut().clear_selection();
            }
        }

        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Advances the search to the next match (Enter in find mode).
    ///
    /// Moves search_origin past the end of the current match and re-runs search.
    fn advance_to_next_match(&mut self) {
        let query = match &self.find_mini_buffer {
            Some(mb) => mb.content(),
            None => return,
        };

        if query.is_empty() {
            return;
        }

        // Get current match end position (the cursor position when there's a selection)
        // If there's a match selection, the cursor is at the end
        let cursor_pos = self.buffer().cursor_position();

        // Move search origin to cursor position (one past the current match start)
        // This ensures we find the next match, not the same one
        self.search_origin = cursor_pos;

        // Run the search from the new origin
        self.run_live_search();
    }

    /// Handles a key event when the selector is focused.
    fn handle_key_selector(&mut self, event: KeyEvent) {
        let selector = match self.active_selector.as_mut() {
            Some(s) => s,
            None => return,
        };

        // Calculate overlay geometry to get visible_items for arrow key navigation
        let line_height = self.font_metrics.line_height as f32;
        let geometry = calculate_overlay_geometry(
            self.view_width,
            self.view_height,
            line_height,
            selector.items().len(),
        );

        // Update visible_items on the selector (for arrow key navigation scroll)
        selector.set_visible_items(geometry.visible_items);

        // Capture the previous query for change detection
        let prev_query = selector.query();

        // Forward to the selector widget
        let outcome = selector.handle_key(&event);

        match outcome {
            SelectorOutcome::Pending => {
                // Check if query changed
                let current_query = selector.query();
                if current_query != prev_query {
                    // Re-query the file index with the new query
                    if let Some(ref file_index) = self.file_index {
                        let results = file_index.query(&current_query);
                        let items: Vec<String> = results
                            .iter()
                            .map(|r| r.path.display().to_string())
                            .collect();
                        // Need to reborrow selector mutably
                        if let Some(ref mut sel) = self.active_selector {
                            sel.set_items(items);
                        }
                        self.last_cache_version = file_index.cache_version();
                    }
                }
                // Mark dirty for any visual update (selection, query, etc.)
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            SelectorOutcome::Confirmed(idx) => {
                // Resolve the path and handle confirmation
                self.handle_selector_confirm(idx);
            }
            SelectorOutcome::Cancelled => {
                self.close_selector();
            }
        }
    }

    /// Handles selector confirmation (Enter pressed).
    fn handle_selector_confirm(&mut self, idx: usize) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Get items and query from selector
        let (items, query) = if let Some(ref selector) = self.active_selector {
            (selector.items().to_vec(), selector.query())
        } else {
            return;
        };

        // Resolve the path
        let resolved = self.resolve_picker_path(idx, &items, &query, &cwd);

        // Record the selection for recency
        if let Some(ref file_index) = self.file_index {
            file_index.record_selection(&resolved);
        }

        // Store the resolved path for file_save chunk to consume
        self.resolved_path = Some(resolved.clone());

        // Immediately associate the file with the buffer
        self.associate_file(resolved);

        // Close the selector
        self.close_selector();
    }

    /// Resolves the path from a selector confirmation.
    ///
    /// If `idx < items.len()`: returns `cwd / items[idx]`
    /// If `idx == usize::MAX` or query doesn't match: returns `cwd / query` (new file)
    /// If the resolved file doesn't exist, creates it as an empty file.
    fn resolve_picker_path(
        &self,
        idx: usize,
        items: &[String],
        query: &str,
        cwd: &Path,
    ) -> PathBuf {
        let resolved = if idx < items.len() {
            cwd.join(&items[idx])
        } else {
            // idx == usize::MAX (empty items) or out of range
            // Use the query as the new filename
            cwd.join(query)
        };

        // Create the file if it doesn't exist
        if !resolved.exists() && !query.is_empty() {
            // Attempt to create the file (ignore errors for now)
            let _ = std::fs::File::create(&resolved);
        }

        resolved
    }

    /// Handles a key event when the buffer is focused.
    fn handle_key_buffer(&mut self, event: KeyEvent) {
        // Record keystroke time for cursor blink reset
        self.last_keystroke = Instant::now();

        // Ensure cursor blink visibility is on when typing
        if !self.cursor_visible {
            self.cursor_visible = true;
            // Mark cursor line dirty to show it
            let cursor_line = self.buffer().cursor_position().line;
            let dirty = self.viewport().dirty_lines_to_region(
                &lite_edit_buffer::DirtyLines::Single(cursor_line),
                self.buffer().line_count(),
            );
            self.dirty_region.merge(dirty);
        }

        // If the cursor is off-screen (scrolled away), snap the viewport back
        // to make the cursor visible BEFORE processing the keystroke.
        // This ensures typing after scrolling doesn't edit at a position
        // the user can't see.
        let cursor_line = self.buffer().cursor_position().line;
        if self
            .viewport()
            .buffer_line_to_screen_line(cursor_line)
            .is_none()
        {
            // Cursor is off-screen - scroll to make it visible
            let line_count = self.buffer().line_count();
            if self.viewport_mut().ensure_visible(cursor_line, line_count) {
                // Viewport scrolled - mark full viewport dirty
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
        }

        // Create context and forward to focus target
        // We need to borrow the active tab's buffer and viewport
        let font_metrics = self.font_metrics;
        // Chunk: docs/chunks/content_tab_bar - Use content area dimensions
        // Adjust dimensions to account for left rail and tab bar
        let content_height = self.view_height - TAB_BAR_HEIGHT;
        let content_width = self.view_width - RAIL_WIDTH;
        let ws = self.editor.active_workspace_mut().expect("no active workspace");
        let tab = ws.active_tab_mut().expect("no active tab");
        let (buffer, viewport) = tab.buffer_and_viewport_mut().expect("not a file tab");

        let mut ctx = EditorContext::new(
            buffer,
            viewport,
            &mut self.dirty_region,
            font_metrics,
            content_height,
            content_width,
        );
        self.focus_target.handle_key(event, &mut ctx);
    }

    /// Handles a mouse event by forwarding to the active focus target.
    ///
    /// This records the event time (for cursor blink reset) and
    /// ensures the cursor is visible after any mouse interaction.
    ///
    /// When the selector is focused, mouse events are forwarded to the selector
    /// widget using the overlay geometry.
    ///
    /// Mouse clicks in the left rail switch workspaces.
    /// Mouse clicks in the tab bar switch tabs.
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        use crate::input::MouseEventKind;

        // Check if click is in left rail region
        let (mouse_x, mouse_y) = event.position;
        if mouse_x < RAIL_WIDTH as f64 {
            if let MouseEventKind::Down = event.kind {
                // Calculate which workspace was clicked
                let geometry = calculate_left_rail_geometry(self.view_height, self.editor.workspace_count());
                for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
                    if tile_rect.contains(mouse_x as f32, mouse_y as f32) {
                        self.switch_workspace(idx);
                        return;
                    }
                }
            }
            // Don't forward rail clicks to buffer
            return;
        }

        // Chunk: docs/chunks/content_tab_bar - Tab bar click handling
        // Check if click is in tab bar region (top of content area)
        // NSView uses bottom-left origin, so tab bar is at y >= (view_height - TAB_BAR_HEIGHT)
        if mouse_y >= (self.view_height - TAB_BAR_HEIGHT) as f64 {
            if let MouseEventKind::Down = event.kind {
                self.handle_tab_bar_click(mouse_x as f32, mouse_y as f32);
            }
            // Don't forward tab bar clicks to buffer
            return;
        }

        // Route based on current focus
        match self.focus {
            EditorFocus::Selector => {
                self.handle_mouse_selector(event);
            }
            EditorFocus::Buffer | EditorFocus::FindInFile => {
                // In FindInFile mode, mouse events still go to the buffer
                // so the user can scroll/click while searching
                self.handle_mouse_buffer(event);
            }
        }
    }

    /// Handles a mouse event when the selector is focused.
    fn handle_mouse_selector(&mut self, event: MouseEvent) {
        let selector = match self.active_selector.as_mut() {
            Some(s) => s,
            None => return,
        };

        // Calculate overlay geometry to map mouse coordinates
        let line_height = self.font_metrics.line_height as f32;
        let geometry = calculate_overlay_geometry(
            self.view_width,
            self.view_height,
            line_height,
            selector.items().len(),
        );

        // Update visible_items on the selector (for consistency with scroll/key handling)
        selector.set_visible_items(geometry.visible_items);

        // Chunk: docs/chunks/selector_coord_flip - Y-coordinate flip for macOS mouse events
        // Flip y-coordinate: macOS uses bottom-left origin (y=0 at bottom),
        // but overlay geometry uses top-left origin (y=0 at top).
        // This matches the pattern in buffer_target.rs for buffer hit-testing.
        let flipped_y = (self.view_height as f64) - event.position.1;
        let flipped_position = (event.position.0, flipped_y);

        // Forward to selector widget with flipped coordinates
        let outcome = selector.handle_mouse(
            flipped_position,
            event.kind,
            geometry.item_height as f64,
            geometry.list_origin_y as f64,
        );

        match outcome {
            SelectorOutcome::Pending => {
                // Mark dirty for visual update
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
            SelectorOutcome::Confirmed(idx) => {
                self.handle_selector_confirm(idx);
            }
            SelectorOutcome::Cancelled => {
                self.close_selector();
            }
        }
    }

    /// Handles a mouse event when the buffer is focused.
    fn handle_mouse_buffer(&mut self, event: MouseEvent) {
        // Record event time for cursor blink reset (same as keystroke)
        self.last_keystroke = Instant::now();

        // Ensure cursor is visible when clicking
        if !self.cursor_visible {
            self.cursor_visible = true;
            // Mark cursor line dirty to show it
            let cursor_line = self.buffer().cursor_position().line;
            let dirty = self.viewport().dirty_lines_to_region(
                &lite_edit_buffer::DirtyLines::Single(cursor_line),
                self.buffer().line_count(),
            );
            self.dirty_region.merge(dirty);
        }

        // Chunk: docs/chunks/content_tab_bar - Coordinate transformation for content area
        // Transform mouse coordinates from full window space to content area space:
        // - X offset: subtract RAIL_WIDTH (content starts after left rail)
        // - Y offset: adjust for TAB_BAR_HEIGHT (in NSView coords, subtract from view_height)
        //
        // NSView uses bottom-left origin, so:
        // - y=0 is at BOTTOM of view
        // - y=view_height is at TOP of view (where tab bar is)
        //
        // The content area in NSView coords spans y=[0, view_height - TAB_BAR_HEIGHT)
        // We adjust view_height so the flip calculation maps correctly to content rows.
        let (original_x, original_y) = event.position;
        let adjusted_x = original_x - RAIL_WIDTH as f64;
        // We pass adjusted_y unchanged but use a reduced view_height for the flip calc
        // This effectively shifts the coordinate system down by TAB_BAR_HEIGHT
        let adjusted_y = original_y;

        let adjusted_event = MouseEvent {
            kind: event.kind,
            position: (adjusted_x, adjusted_y),
            modifiers: event.modifiers,
            click_count: event.click_count,
        };

        // Adjust view_height for content area (subtract tab bar height)
        // This makes the y-flip calculation in pixel_to_buffer_position correct
        let content_height = self.view_height - TAB_BAR_HEIGHT;

        // Create context and forward to focus target
        let font_metrics = self.font_metrics;
        let ws = self.editor.active_workspace_mut().expect("no active workspace");
        let tab = ws.active_tab_mut().expect("no active tab");
        let (buffer, viewport) = tab.buffer_and_viewport_mut().expect("not a file tab");

        let mut ctx = EditorContext::new(
            buffer,
            viewport,
            &mut self.dirty_region,
            font_metrics,
            content_height,
            self.view_width - RAIL_WIDTH, // Content width also adjusted
        );
        self.focus_target.handle_mouse(adjusted_event, &mut ctx);
    }

    /// Handles a scroll event by forwarding to the active focus target.
    ///
    /// Scroll events only affect the viewport, not the cursor position or buffer.
    /// The cursor may end up off-screen after scrolling, which is intentional.
    ///
    /// When the selector is open, scroll events are forwarded to the selector
    /// to scroll the item list.
    ///
    /// When find-in-file is open, scroll events go to the main buffer (the user
    /// can scroll while searching).
    pub fn handle_scroll(&mut self, delta: ScrollDelta) {
        // When selector is open, forward scroll to selector
        if self.focus == EditorFocus::Selector {
            self.handle_scroll_selector(delta);
            return;
        }

        // Chunk: docs/chunks/content_tab_bar - Tab bar horizontal scrolling
        // Note: horizontal scroll in tab bar region is handled via handle_scroll_tab_bar
        // which is called from handle_mouse when scroll events occur in tab bar area

        // In Buffer or FindInFile mode, scroll the buffer
        // Create context and forward to focus target
        let font_metrics = self.font_metrics;
        // Chunk: docs/chunks/content_tab_bar - Use content area dimensions
        let content_height = self.view_height - TAB_BAR_HEIGHT;
        let content_width = self.view_width - RAIL_WIDTH;
        let ws = self.editor.active_workspace_mut().expect("no active workspace");
        let tab = ws.active_tab_mut().expect("no active tab");
        let (buffer, viewport) = tab.buffer_and_viewport_mut().expect("not a file tab");

        let mut ctx = EditorContext::new(
            buffer,
            viewport,
            &mut self.dirty_region,
            font_metrics,
            content_height,
            content_width,
        );
        self.focus_target.handle_scroll(delta, &mut ctx);
    }

    /// Handles a scroll event when the selector is focused.
    fn handle_scroll_selector(&mut self, delta: ScrollDelta) {
        let selector = match self.active_selector.as_mut() {
            Some(s) => s,
            None => return,
        };

        // Calculate overlay geometry to get item_height and visible_items
        let line_height = self.font_metrics.line_height as f32;
        let geometry = calculate_overlay_geometry(
            self.view_width,
            self.view_height,
            line_height,
            selector.items().len(),
        );

        // Update visible_items on the selector (for arrow key navigation)
        selector.set_visible_items(geometry.visible_items);

        // Forward scroll to selector
        selector.handle_scroll(
            delta.dy as f64,
            geometry.item_height as f64,
            geometry.visible_items,
        );

        // Mark full viewport dirty for redraw
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Returns true if any screen region needs re-rendering.
    pub fn is_dirty(&self) -> bool {
        self.dirty_region.is_dirty()
    }

    /// Called periodically to check for streaming file index updates.
    ///
    /// When the selector is open and the file index has discovered new paths,
    /// this re-queries the index with the current query and updates the selector's
    /// item list. This is the mechanism by which results stream in during the
    /// initial directory walk.
    ///
    /// Returns `DirtyRegion::FullViewport` if items were updated, `None` otherwise.
    pub fn tick_picker(&mut self) -> DirtyRegion {
        // Only relevant when selector is active
        if self.focus != EditorFocus::Selector {
            return DirtyRegion::None;
        }

        let file_index = match &self.file_index {
            Some(idx) => idx,
            None => return DirtyRegion::None,
        };

        // Check if cache version has changed
        let current_version = file_index.cache_version();
        if current_version <= self.last_cache_version {
            return DirtyRegion::None;
        }

        // Re-query with current query
        let query = self
            .active_selector
            .as_ref()
            .map(|s| s.query())
            .unwrap_or_default();

        let results = file_index.query(&query);
        let items: Vec<String> = results
            .iter()
            .map(|r| r.path.display().to_string())
            .collect();

        // Update the selector items
        if let Some(ref mut widget) = self.active_selector {
            widget.set_items(items);
        }

        self.last_cache_version = current_version;

        DirtyRegion::FullViewport
    }

    // =========================================================================
    // Agent Polling (Chunk: docs/chunks/agent_lifecycle)
    // =========================================================================

    /// Polls all agents in all workspaces for PTY events.
    ///
    /// Call this each frame to:
    /// 1. Process PTY output from agent processes
    /// 2. Update agent state machines (Running → NeedsInput → Stale)
    /// 3. Update workspace status indicators
    ///
    /// Returns `DirtyRegion::FullViewport` if any agent had activity,
    /// otherwise `DirtyRegion::None`.
    pub fn poll_agents(&mut self) -> DirtyRegion {
        let mut any_activity = false;

        for workspace in &mut self.editor.workspaces {
            if workspace.poll_agent() {
                any_activity = true;
            }
        }

        if any_activity {
            DirtyRegion::FullViewport
        } else {
            DirtyRegion::None
        }
    }

    /// Takes the dirty region, leaving `DirtyRegion::None` in its place.
    ///
    /// Call this after rendering to reset the dirty state.
    pub fn take_dirty_region(&mut self) -> DirtyRegion {
        std::mem::take(&mut self.dirty_region)
    }

    /// Toggles cursor visibility for blink animation.
    ///
    /// Returns the dirty region for the cursor line if visibility changed.
    /// If the user recently typed, this keeps the cursor solid instead of toggling.
    pub fn toggle_cursor_blink(&mut self) -> DirtyRegion {
        let now = Instant::now();
        let since_keystroke = now.duration_since(self.last_keystroke);

        // If user typed recently, keep cursor solid
        if since_keystroke.as_millis() < CURSOR_BLINK_INTERVAL_MS as u128 {
            if !self.cursor_visible {
                self.cursor_visible = true;
                return self.cursor_dirty_region();
            }
            return DirtyRegion::None;
        }

        // Toggle visibility
        self.cursor_visible = !self.cursor_visible;
        self.cursor_dirty_region()
    }

    /// Returns the dirty region for just the cursor line.
    fn cursor_dirty_region(&self) -> DirtyRegion {
        let cursor_line = self.buffer().cursor_position().line;
        self.viewport().dirty_lines_to_region(
            &lite_edit_buffer::DirtyLines::Single(cursor_line),
            self.buffer().line_count(),
        )
    }

    /// Marks the full viewport as dirty (e.g., after buffer replacement).
    pub fn mark_full_dirty(&mut self) {
        self.dirty_region = DirtyRegion::FullViewport;
    }

    // =========================================================================
    // File Association (Chunk: docs/chunks/file_save)
    // =========================================================================

    /// Associates a file path with the current buffer.
    ///
    /// If the file at `path` exists:
    /// - Reads its contents as UTF-8 (with lossy conversion for invalid bytes)
    /// - Replaces the buffer with those contents
    /// - Resets cursor to (0, 0)
    /// - Resets viewport scroll offset to 0
    ///
    /// If the file does not exist (newly created by file picker):
    /// - Leaves the buffer as-is
    ///
    /// In both cases:
    /// - Stores `path` in `associated_file`
    /// - Marks `DirtyRegion::FullViewport`
    pub fn associate_file(&mut self, path: PathBuf) {
        if path.exists() {
            // Read file contents with UTF-8 lossy conversion
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let contents = String::from_utf8_lossy(&bytes);
                    *self.buffer_mut() = TextBuffer::from_str(&contents);
                    self.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));
                    let line_count = self.buffer().line_count();
                    self.viewport_mut().scroll_to(0, line_count);
                }
                Err(_) => {
                    // Silently ignore read errors (out of scope for this chunk)
                }
            }
        }
        // For non-existent files, leave buffer as-is (file picker already created empty file)

        self.set_associated_file(Some(path));
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Returns the window title based on the current file association.
    ///
    /// Returns the filename if a file is associated, or "Untitled" otherwise.
    /// When multiple workspaces exist, includes the workspace label.
    pub fn window_title(&self) -> String {
        let tab_name = self.associated_file()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled");

        if self.editor.workspace_count() > 1 {
            if let Some(workspace) = self.editor.active_workspace() {
                return format!("{} — {}", tab_name, workspace.label);
            }
        }

        tab_name.to_string()
    }

    /// Saves the buffer content to the associated file.
    ///
    /// If no file is associated, this is a no-op.
    /// On write error, this silently fails (error reporting is out of scope).
    fn save_file(&mut self) {
        let path = match self.associated_file() {
            Some(p) => p.clone(),
            None => return, // No file associated - no-op
        };

        let content = self.buffer().content();
        let _ = std::fs::write(&path, content.as_bytes());
        // Silently ignore write errors (out of scope for this chunk)

        // Cmd+S does NOT mark dirty (buffer unchanged visually)
    }
}

impl Default for EditorState {
    fn default() -> Self {
        // Sensible default font metrics
        let font_metrics = FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        };
        Self::empty(font_metrics)
    }
}

// =============================================================================
// Workspace Commands (Chunk: docs/chunks/workspace_model)
// =============================================================================

impl EditorState {
    /// Creates a new workspace and switches to it.
    ///
    /// The new workspace has one empty tab.
    pub fn new_workspace(&mut self) {
        let root_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        self.editor.new_workspace("untitled".to_string(), root_path);
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Closes the active workspace.
    ///
    /// Does nothing if this is the last workspace.
    pub fn close_active_workspace(&mut self) {
        if self.editor.workspace_count() > 1 {
            self.editor.close_workspace(self.editor.active_workspace);
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
    }

    /// Switches to the workspace at the given index (0-based).
    ///
    /// Does nothing if the index is out of bounds.
    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.editor.workspace_count() && index != self.editor.active_workspace {
            self.editor.switch_workspace(index);
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
    }

    // =========================================================================
    // Tab Management (Chunk: docs/chunks/content_tab_bar)
    // =========================================================================

    /// Switches to the tab at the given index in the active workspace.
    ///
    /// Does nothing if the index is out of bounds or if it's the current tab.
    pub fn switch_tab(&mut self, index: usize) {
        if let Some(workspace) = self.editor.active_workspace_mut() {
            if index < workspace.tabs.len() && index != workspace.active_tab {
                workspace.switch_tab(index);
                // Clear unread badge when switching to a tab
                if let Some(tab) = workspace.tabs.get_mut(index) {
                    tab.unread = false;
                }
                self.dirty_region.merge(DirtyRegion::FullViewport);
            }
        }
    }

    /// Closes the tab at the given index in the active workspace.
    ///
    /// If this is the last tab, creates a new empty tab instead of closing.
    pub fn close_tab(&mut self, index: usize) {
        if let Some(workspace) = self.editor.active_workspace_mut() {
            // Guard: don't close dirty tabs (confirmation UI is future work)
            if let Some(tab) = workspace.tabs.get(index) {
                if tab.dirty {
                    return;
                }
            }
            if workspace.tabs.len() > 1 {
                workspace.close_tab(index);
            } else {
                // Create a new empty tab and close the old one
                let tab_id = self.editor.gen_tab_id();
                let line_height = self.editor.line_height();
                let new_tab = crate::workspace::Tab::empty_file(tab_id, line_height);
                if let Some(workspace) = self.editor.active_workspace_mut() {
                    workspace.tabs[0] = new_tab;
                    workspace.active_tab = 0;
                }
            }
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
    }

    /// Closes the active tab in the active workspace.
    pub fn close_active_tab(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            let active = workspace.active_tab;
            self.close_tab(active);
        }
    }

    /// Cycles to the next tab in the active workspace.
    ///
    /// Wraps around from the last tab to the first.
    /// Does nothing if there's only one tab.
    pub fn next_tab(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            if workspace.tabs.len() > 1 {
                let next = (workspace.active_tab + 1) % workspace.tabs.len();
                self.switch_tab(next);
            }
        }
    }

    /// Cycles to the previous tab in the active workspace.
    ///
    /// Wraps around from the first tab to the last.
    /// Does nothing if there's only one tab.
    pub fn prev_tab(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            if workspace.tabs.len() > 1 {
                let prev = if workspace.active_tab == 0 {
                    workspace.tabs.len() - 1
                } else {
                    workspace.active_tab - 1
                };
                self.switch_tab(prev);
            }
        }
    }

    /// Creates a new empty tab in the active workspace and switches to it.
    ///
    /// This is triggered by Cmd+T. For now, this creates an empty file tab.
    /// Terminal tab creation will be added in the terminal_emulator chunk.
    pub fn new_tab(&mut self) {
        let tab_id = self.editor.gen_tab_id();
        let line_height = self.editor.line_height();
        let new_tab = crate::workspace::Tab::empty_file(tab_id, line_height);

        if let Some(workspace) = self.editor.active_workspace_mut() {
            workspace.add_tab(new_tab);
        }

        // Ensure the new tab is visible in the tab bar
        self.ensure_active_tab_visible();
        self.dirty_region.merge(DirtyRegion::FullViewport);
    }

    /// Scrolls the tab bar horizontally.
    ///
    /// Positive delta scrolls right (reveals more tabs to the right),
    /// negative delta scrolls left (reveals more tabs to the left).
    pub fn scroll_tab_bar(&mut self, delta: f32) {
        if let Some(workspace) = self.editor.active_workspace_mut() {
            workspace.tab_bar_view_offset += delta;
            // Clamp to valid range (minimum 0)
            if workspace.tab_bar_view_offset < 0.0 {
                workspace.tab_bar_view_offset = 0.0;
            }
            self.dirty_region.merge(DirtyRegion::FullViewport);
        }
    }

    /// Ensures the active tab is visible in the tab bar.
    ///
    /// If the active tab is scrolled out of view, adjusts the scroll offset
    /// to bring it into view.
    pub fn ensure_active_tab_visible(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            let tabs = tabs_from_workspace(workspace);
            let glyph_width = self.font_metrics.advance_width as f32;
            let geometry = calculate_tab_bar_geometry(
                self.view_width,
                &tabs,
                glyph_width,
                workspace.tab_bar_view_offset,
            );

            // Check if active tab is visible
            if let Some(active_rect) = geometry.tab_rects.get(workspace.active_tab) {
                let visible_start = RAIL_WIDTH;
                let visible_end = self.view_width;

                // If tab is to the left of visible area, scroll left
                if active_rect.x < visible_start {
                    let scroll_amount = visible_start - active_rect.x;
                    if let Some(workspace) = self.editor.active_workspace_mut() {
                        workspace.tab_bar_view_offset -= scroll_amount;
                        if workspace.tab_bar_view_offset < 0.0 {
                            workspace.tab_bar_view_offset = 0.0;
                        }
                    }
                }

                // If tab is to the right of visible area, scroll right
                let tab_right = active_rect.x + active_rect.width;
                if tab_right > visible_end {
                    let scroll_amount = tab_right - visible_end;
                    if let Some(workspace) = self.editor.active_workspace_mut() {
                        workspace.tab_bar_view_offset += scroll_amount;
                    }
                }
            }
        }
    }

    /// Handles a mouse click in the tab bar region.
    ///
    /// Determines which tab was clicked and switches to it, or handles
    /// close button clicks.
    fn handle_tab_bar_click(&mut self, mouse_x: f32, mouse_y: f32) {
        if let Some(workspace) = self.editor.active_workspace() {
            let tabs = tabs_from_workspace(workspace);
            let glyph_width = self.font_metrics.advance_width as f32;
            let geometry = calculate_tab_bar_geometry(
                self.view_width,
                &tabs,
                glyph_width,
                workspace.tab_bar_view_offset,
            );

            // Check each tab rect
            for (idx, tab_rect) in geometry.tab_rects.iter().enumerate() {
                if tab_rect.contains(mouse_x, mouse_y) {
                    // Check if close button was clicked (close button is part of TabRect)
                    if tab_rect.is_close_button(mouse_x, mouse_y) {
                        self.close_tab(idx);
                        return;
                    }
                    // Otherwise, switch to the tab
                    self.switch_tab(idx);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, Modifiers, MouseEvent, MouseEventKind, ScrollDelta};
    use std::time::Duration;

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

    #[test]
    fn test_new_state() {
        let state = EditorState::empty(test_font_metrics());
        assert!(state.buffer().is_empty());
        assert!(!state.is_dirty());
        assert!(state.cursor_visible);
        assert!(!state.should_quit);
    }

    #[test]
    fn test_handle_key_marks_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));

        assert!(state.is_dirty());
        assert_eq!(state.buffer().content(), "a");
    }

    #[test]
    fn test_take_dirty_region_resets() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        state.handle_key(KeyEvent::char('a'));
        assert!(state.is_dirty());

        let dirty = state.take_dirty_region();
        assert!(dirty.is_dirty());
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_keystroke_shows_cursor() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);
        state.cursor_visible = false;

        state.handle_key(KeyEvent::char('a'));

        assert!(state.cursor_visible);
    }

    #[test]
    fn test_toggle_cursor_blink() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Set last_keystroke to the past so blink toggle works
        state.last_keystroke = Instant::now() - Duration::from_secs(1);

        assert!(state.cursor_visible);
        state.toggle_cursor_blink();
        assert!(!state.cursor_visible);
        state.toggle_cursor_blink();
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_recent_keystroke_keeps_cursor_solid() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Keystroke just happened
        state.last_keystroke = Instant::now();

        // Toggle should keep cursor visible
        state.toggle_cursor_blink();
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_viewport_size_update() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        assert_eq!(state.viewport().visible_lines(), 20); // 320 / 16 = 20
        assert_eq!(state.view_height, 320.0);
    }

    // =========================================================================
    // Quit flag tests (Cmd+Q behavior)
    // =========================================================================

    #[test]
    fn test_cmd_q_sets_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Cmd+Q should set should_quit
        let cmd_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_q);

        assert!(state.should_quit);
    }

    #[test]
    fn test_cmd_q_does_not_modify_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content first
        state.handle_key(KeyEvent::char('a'));
        assert_eq!(state.buffer().content(), "a");

        // Cmd+Q should NOT add 'q' to the buffer
        let cmd_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_q);

        // Buffer should be unchanged
        assert_eq!(state.buffer().content(), "a");
        assert!(state.should_quit);
    }

    #[test]
    fn test_ctrl_q_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Ctrl+Q should NOT set should_quit (different binding)
        let ctrl_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        state.handle_key(ctrl_q);

        assert!(!state.should_quit);
    }

    #[test]
    fn test_cmd_ctrl_q_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Cmd+Ctrl+Q should NOT set should_quit (we explicitly check !control)
        let cmd_ctrl_q = KeyEvent::new(
            Key::Char('q'),
            Modifiers {
                command: true,
                control: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_ctrl_q);

        assert!(!state.should_quit);
    }

    #[test]
    fn test_cmd_z_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Other Cmd+ combinations should NOT set should_quit
        let cmd_z = KeyEvent::new(
            Key::Char('z'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_z);

        assert!(!state.should_quit);
    }

    #[test]
    fn test_plain_q_does_not_set_quit_flag() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Plain 'q' should type 'q', not quit
        state.handle_key(KeyEvent::char('q'));

        assert!(!state.should_quit);
        assert_eq!(state.buffer().content(), "q");
    }

    // =========================================================================
    // Scroll handling tests
    // =========================================================================

    #[test]
    fn test_handle_scroll_moves_viewport() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0); // 10 visible lines

        // Initial scroll offset should be 0
        assert_eq!(state.viewport().first_visible_line(), 0);

        // Scroll down by 5 lines (positive dy = scroll down)
        // line_height is 16.0, so 5 lines = 80 pixels
        state.handle_scroll(ScrollDelta::new(0.0, 80.0));

        // Viewport should have scrolled
        assert_eq!(state.viewport().first_visible_line(), 5);
        assert!(state.is_dirty()); // Should be dirty after scroll
    }

    #[test]
    fn test_handle_scroll_does_not_move_cursor() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0);

        // Set cursor to line 3
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(3, 5));

        // Scroll down by 10 lines
        state.handle_scroll(ScrollDelta::new(0.0, 160.0));

        // Cursor position should be unchanged
        assert_eq!(
            state.buffer().cursor_position(),
            lite_edit_buffer::Position::new(3, 5)
        );
    }

    #[test]
    fn test_keystroke_snaps_back_when_cursor_off_screen() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0); // 10 visible lines

        // Cursor starts at line 0
        assert_eq!(state.buffer().cursor_position().line, 0);

        // Scroll down so cursor is off-screen (scroll to show lines 15-24)
        state.handle_scroll(ScrollDelta::new(0.0, 15.0 * 16.0)); // 15 lines * 16 pixels
        assert_eq!(state.viewport().first_visible_line(), 15);

        // Clear dirty flag
        let _ = state.take_dirty_region();

        // Now type a character - viewport should snap back to show cursor
        state.handle_key(KeyEvent::char('X'));

        // Cursor should still be at line 0, and viewport should have scrolled
        // back to make line 0 visible
        assert_eq!(state.buffer().cursor_position().line, 0);
        assert_eq!(state.viewport().first_visible_line(), 0);
        assert!(state.is_dirty()); // Should be dirty after snap-back
    }

    #[test]
    fn test_no_snapback_when_cursor_visible() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0); // 10 visible lines

        // Move cursor to line 15
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(15, 0));

        // Scroll to make line 15 visible (show lines 10-19)
        state.viewport_mut().scroll_to(10, 50);
        assert_eq!(state.viewport().first_visible_line(), 10);

        // Clear dirty flag
        let _ = state.take_dirty_region();

        // Type a character - viewport should NOT snap back since cursor is visible
        state.handle_key(KeyEvent::char('X'));

        // Scroll offset should remain at 10
        assert_eq!(state.viewport().first_visible_line(), 10);
    }

    // =========================================================================
    // File Picker Tests (Cmd+P behavior)
    // =========================================================================

    #[test]
    fn test_initial_focus_is_buffer() {
        let state = EditorState::empty(test_font_metrics());
        assert_eq!(state.focus, EditorFocus::Buffer);
    }

    #[test]
    fn test_initial_active_selector_is_none() {
        let state = EditorState::empty(test_font_metrics());
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_cmd_p_transitions_to_selector_focus() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        assert_eq!(state.focus, EditorFocus::Selector);
    }

    #[test]
    fn test_cmd_p_opens_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        assert!(state.active_selector.is_some());
    }

    #[test]
    fn test_cmd_p_does_not_insert_p() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Buffer should remain empty - 'p' should not be inserted
        assert!(state.buffer().is_empty());
    }

    #[test]
    fn test_cmd_p_when_selector_open_closes_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );

        // Open the selector
        state.handle_key(cmd_p.clone());
        assert_eq!(state.focus, EditorFocus::Selector);

        // Press Cmd+P again - should close
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_escape_closes_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Press Escape
        let escape = KeyEvent::new(Key::Escape, Modifiers::default());
        state.handle_key(escape);

        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_typing_in_selector_appends_to_query() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Type some characters
        state.handle_key(KeyEvent::char('t'));
        state.handle_key(KeyEvent::char('e'));
        state.handle_key(KeyEvent::char('s'));
        state.handle_key(KeyEvent::char('t'));

        // Check query
        let query = state.active_selector.as_ref().unwrap().query();
        assert_eq!(query, "test");
    }

    #[test]
    fn test_down_arrow_moves_selection_in_selector() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Set some items manually for testing
        if let Some(ref mut selector) = state.active_selector {
            selector.set_items(vec!["file1.rs".into(), "file2.rs".into(), "file3.rs".into()]);
            assert_eq!(selector.selected_index(), 0);
        }

        // Press Down
        state.handle_key(KeyEvent::new(Key::Down, Modifiers::default()));

        let selected = state.active_selector.as_ref().unwrap().selected_index();
        assert_eq!(selected, 1);
    }

    #[test]
    fn test_scroll_when_selector_open_scrolls_selector_not_buffer() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_dimensions(800.0, 160.0); // 10 visible lines

        // Initial scroll offset should be 0
        assert_eq!(state.viewport().scroll_offset(), 0);

        // Open the selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Set up many items in the selector for scrolling
        if let Some(ref mut selector) = state.active_selector {
            selector.set_items((0..50).map(|i| format!("file{}.rs", i)).collect());
        }

        // Try to scroll
        state.handle_scroll(ScrollDelta::new(0.0, 80.0));

        // Buffer viewport should NOT have scrolled
        assert_eq!(state.viewport().scroll_offset(), 0);

        // But the selector should have scrolled
        let view_offset = state.active_selector.as_ref().unwrap().view_offset();
        assert!(view_offset > 0, "Selector should have scrolled");
    }

    #[test]
    fn test_scroll_when_selector_open_updates_view_offset() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open the selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Set up many items in the selector
        if let Some(ref mut selector) = state.active_selector {
            selector.set_items((0..100).map(|i| format!("file{}.rs", i)).collect());
        }

        // Initial view_offset should be 0
        assert_eq!(state.active_selector.as_ref().unwrap().view_offset(), 0);

        // Scroll down (positive delta = scroll down)
        // line_height is 16.0, so 48 pixels = 3 rows
        state.handle_scroll(ScrollDelta::new(0.0, 48.0));

        // view_offset should have increased
        let view_offset = state.active_selector.as_ref().unwrap().view_offset();
        assert_eq!(view_offset, 3);
    }

    #[test]
    fn test_scroll_when_buffer_focused_scrolls_buffer() {
        // Create a buffer with many lines
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_dimensions(800.0, 160.0); // 10 visible lines

        // Initial scroll offset should be 0
        assert_eq!(state.viewport().scroll_offset(), 0);

        // Ensure we're in buffer focus (default)
        assert_eq!(state.focus, EditorFocus::Buffer);

        // Scroll down by 5 lines (80 pixels with line_height 16)
        state.handle_scroll(ScrollDelta::new(0.0, 80.0));

        // Buffer viewport should have scrolled
        assert_eq!(state.viewport().first_visible_line(), 5);
    }

    #[test]
    fn test_tick_picker_returns_none_when_buffer_focused() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Focus is Buffer, tick_picker should return None
        let dirty = state.tick_picker();
        assert!(!dirty.is_dirty());
    }

    #[test]
    fn test_tick_picker_returns_none_when_no_version_change() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open selector
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);
        assert_eq!(state.focus, EditorFocus::Selector);

        // Clear dirty region from opening
        let _ = state.take_dirty_region();

        // First tick - might update if cache changed
        let _first = state.tick_picker();

        // Second tick immediately - should return None (no change)
        let dirty = state.tick_picker();
        assert!(!dirty.is_dirty());
    }

    // =========================================================================
    // File Association Tests (Chunk: docs/chunks/file_save)
    // =========================================================================

    #[test]
    fn test_initial_associated_file_is_none() {
        let state = EditorState::empty(test_font_metrics());
        assert!(state.associated_file().is_none());
    }

    #[test]
    fn test_associate_file_with_existing_file_loads_content() {
        use std::io::Write;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file with content
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_associate_file.txt");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            f.write_all(b"Hello, world!\nLine two\n").unwrap();
        }

        state.associate_file(temp_file.clone());

        // Buffer should contain the file content
        assert_eq!(state.buffer().content(), "Hello, world!\nLine two\n");

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_associate_file_with_existing_file_sets_cursor_to_origin() {
        use std::io::Write;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content and move cursor
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        assert_eq!(state.buffer().cursor_position().col, 2);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_associate_cursor.txt");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            f.write_all(b"Content here").unwrap();
        }

        state.associate_file(temp_file.clone());

        // Cursor should be at (0, 0)
        assert_eq!(state.buffer().cursor_position().line, 0);
        assert_eq!(state.buffer().cursor_position().col, 0);

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_associate_file_with_existing_file_sets_associated_file() {
        use std::io::Write;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_associate_path.txt");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            f.write_all(b"Content").unwrap();
        }

        state.associate_file(temp_file.clone());

        // associated_file should be Some(path)
        assert_eq!(state.associated_file(), Some(&temp_file));

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_associate_file_with_nonexistent_path_keeps_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        assert_eq!(state.buffer().content(), "ab");

        // Associate with a non-existent file
        let nonexistent_path = PathBuf::from("/nonexistent/path/to/file.txt");
        state.associate_file(nonexistent_path.clone());

        // Buffer should be unchanged
        assert_eq!(state.buffer().content(), "ab");
    }

    #[test]
    fn test_associate_file_with_nonexistent_path_sets_associated_file() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        let nonexistent_path = PathBuf::from("/nonexistent/path/to/file.txt");
        state.associate_file(nonexistent_path.clone());

        // associated_file should be Some(path)
        assert_eq!(state.associated_file(), Some(&nonexistent_path));
    }

    #[test]
    fn test_associate_file_resets_scroll_offset() {
        use std::io::Write;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0); // 10 visible lines

        // Manually set scroll offset
        state.viewport_mut().scroll_to(10, 100);
        assert_eq!(state.viewport().scroll_offset(), 10);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_scroll_reset.txt");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            f.write_all(b"Line 1\n").unwrap();
        }

        state.associate_file(temp_file.clone());

        // Scroll offset should be reset to 0
        assert_eq!(state.viewport().scroll_offset(), 0);

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_associate_file_marks_full_viewport_dirty() {
        use std::io::Write;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Clear any existing dirty region
        let _ = state.take_dirty_region();
        assert!(!state.is_dirty());

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_dirty_viewport.txt");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            f.write_all(b"Content").unwrap();
        }

        state.associate_file(temp_file.clone());

        // Should be dirty after association
        assert!(state.is_dirty());

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    // =========================================================================
    // Window Title Tests (Chunk: docs/chunks/file_save)
    // =========================================================================

    #[test]
    fn test_window_title_returns_untitled_when_no_file() {
        let state = EditorState::empty(test_font_metrics());
        assert_eq!(state.window_title(), "Untitled");
    }

    #[test]
    fn test_window_title_returns_filename_when_file_associated() {
        let mut state = EditorState::empty(test_font_metrics());

        let path = PathBuf::from("/some/path/to/myfile.rs");
        state.set_associated_file(Some(path));

        assert_eq!(state.window_title(), "myfile.rs");
    }

    #[test]
    fn test_window_title_returns_filename_for_nested_path() {
        let mut state = EditorState::empty(test_font_metrics());

        let path = PathBuf::from("/Users/btaylor/Projects/lite-edit/src/main.rs");
        state.set_associated_file(Some(path));

        assert_eq!(state.window_title(), "main.rs");
    }

    // =========================================================================
    // Cmd+S Save Tests (Chunk: docs/chunks/file_save)
    // =========================================================================

    #[test]
    fn test_cmd_s_with_no_associated_file_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));

        // Clear dirty region
        let _ = state.take_dirty_region();

        // Press Cmd+S
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // Buffer should be unchanged
        assert_eq!(state.buffer().content(), "ab");
    }

    #[test]
    fn test_cmd_s_writes_to_file() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_cmd_s_save.txt");

        // Set up the associated file
        state.set_associated_file(Some(temp_file.clone()));

        // Type some content
        state.handle_key(KeyEvent::char('H'));
        state.handle_key(KeyEvent::char('i'));
        state.handle_key(KeyEvent::char('!'));

        // Press Cmd+S
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // File should contain the buffer content
        let file_content = std::fs::read_to_string(&temp_file).unwrap();
        assert_eq!(file_content, "Hi!");

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_cmd_s_does_not_modify_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_cmd_s_no_modify.txt");

        state.set_associated_file(Some(temp_file.clone()));

        // Type content
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));

        let content_before = state.buffer().content();

        // Press Cmd+S
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // Buffer content should be unchanged
        assert_eq!(state.buffer().content(), content_before);

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_cmd_s_does_not_move_cursor() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_cmd_s_cursor.txt");

        state.set_associated_file(Some(temp_file.clone()));

        // Type content
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        state.handle_key(KeyEvent::char('c'));

        let cursor_before = state.buffer().cursor_position();

        // Press Cmd+S
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // Cursor should be unchanged
        assert_eq!(state.buffer().cursor_position(), cursor_before);

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_cmd_s_does_not_mark_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_cmd_s_dirty.txt");

        state.set_associated_file(Some(temp_file.clone()));

        // Type content
        state.handle_key(KeyEvent::char('a'));

        // Clear dirty region
        let _ = state.take_dirty_region();
        assert!(!state.is_dirty());

        // Press Cmd+S
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // Should NOT be dirty after Cmd+S
        assert!(!state.is_dirty());

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_cmd_s_does_not_insert_s() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Buffer should be empty
        assert!(state.buffer().is_empty());

        // Press Cmd+S
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // Buffer should still be empty (no 's' inserted)
        assert!(state.buffer().is_empty());
    }

    #[test]
    fn test_ctrl_s_does_not_save() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Ctrl+S should NOT trigger save (different binding)
        // It should pass through to buffer and potentially insert
        let ctrl_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        state.handle_key(ctrl_s);

        // No file associated, so nothing should crash
        // (we just verify it doesn't trigger save behavior)
        assert!(state.associated_file().is_none());
    }

    // =========================================================================
    // Workspace command tests (Chunk: docs/chunks/workspace_model)
    // =========================================================================

    #[test]
    fn test_cmd_n_creates_new_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.workspace_count(), 1);

        let cmd_n = KeyEvent::new(
            Key::Char('n'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_n);

        assert_eq!(state.editor.workspace_count(), 2);
        assert_eq!(state.editor.active_workspace, 1); // Switched to new workspace
        assert!(state.is_dirty()); // Should mark dirty for UI update
    }

    #[test]
    fn test_cmd_shift_w_closes_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a second workspace
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 2);

        let _ = state.take_dirty_region(); // Clear dirty

        // Close the active workspace
        let cmd_shift_w = KeyEvent::new(
            Key::Char('w'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_w);

        assert_eq!(state.editor.workspace_count(), 1);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_cmd_shift_w_does_not_close_last_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.workspace_count(), 1);

        let cmd_shift_w = KeyEvent::new(
            Key::Char('w'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_w);

        // Should still have one workspace
        assert_eq!(state.editor.workspace_count(), 1);
    }

    #[test]
    fn test_cmd_1_switches_to_first_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a second workspace (switches to it)
        state.new_workspace();
        assert_eq!(state.editor.active_workspace, 1);

        let _ = state.take_dirty_region(); // Clear dirty

        // Press Cmd+1 to switch to first workspace
        let cmd_1 = KeyEvent::new(
            Key::Char('1'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_1);

        assert_eq!(state.editor.active_workspace, 0);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_cmd_2_switches_to_second_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a second workspace
        state.new_workspace();
        // Switch back to first
        state.switch_workspace(0);
        assert_eq!(state.editor.active_workspace, 0);

        let _ = state.take_dirty_region();

        // Press Cmd+2 to switch to second workspace
        let cmd_2 = KeyEvent::new(
            Key::Char('2'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_2);

        assert_eq!(state.editor.active_workspace, 1);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_cmd_digit_out_of_range_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only one workspace exists
        assert_eq!(state.editor.workspace_count(), 1);
        assert_eq!(state.editor.active_workspace, 0);

        // Press Cmd+3 (no third workspace)
        let cmd_3 = KeyEvent::new(
            Key::Char('3'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_3);

        // Should remain unchanged
        assert_eq!(state.editor.active_workspace, 0);
    }

    #[test]
    fn test_window_title_includes_workspace_label_when_multiple() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // With one workspace, title should just be "Untitled"
        assert_eq!(state.window_title(), "Untitled");

        // Create a second workspace
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 2);

        // Now title should include workspace label
        let title = state.window_title();
        assert!(title.contains("—")); // em-dash separator
        assert!(title.contains("untitled")); // workspace label
    }

    // =========================================================================
    // Find-in-File Tests (Chunk: docs/chunks/find_in_file)
    // =========================================================================

    #[test]
    fn test_cmd_f_transitions_to_find_focus() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.focus, EditorFocus::Buffer);

        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        assert_eq!(state.focus, EditorFocus::FindInFile);
    }

    #[test]
    fn test_cmd_f_creates_mini_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert!(state.find_mini_buffer.is_none());

        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        assert!(state.find_mini_buffer.is_some());
    }

    #[test]
    fn test_cmd_f_records_search_origin() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content and move cursor
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        state.handle_key(KeyEvent::char('c'));

        let cursor_pos = state.buffer().cursor_position();

        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // search_origin should equal cursor position at time Cmd+F was pressed
        assert_eq!(state.search_origin, cursor_pos);
    }

    #[test]
    fn test_escape_closes_find_strip() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);
        assert_eq!(state.focus, EditorFocus::FindInFile);

        // Press Escape
        let escape = KeyEvent::new(Key::Escape, Modifiers::default());
        state.handle_key(escape);

        // Should be back to Buffer focus
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.find_mini_buffer.is_none());
    }

    #[test]
    fn test_cmd_f_while_open_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f.clone());
        assert_eq!(state.focus, EditorFocus::FindInFile);

        // Get the mini buffer content
        let original_content = state.find_mini_buffer.as_ref().unwrap().content();

        // Press Cmd+F again
        state.handle_key(cmd_f);

        // Focus should still be FindInFile, mini buffer unchanged
        assert_eq!(state.focus, EditorFocus::FindInFile);
        assert_eq!(
            state.find_mini_buffer.as_ref().unwrap().content(),
            original_content
        );
    }

    #[test]
    fn test_typing_in_find_selects_match() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with known content
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("hello world hello");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type "world"
        for c in "world".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // Buffer selection should cover "world" (positions 6-11)
        let selection = state.buffer().selection_range();
        assert!(selection.is_some(), "Expected a selection after typing in find");
        let (start, end) = selection.unwrap();
        assert_eq!(start.col, 6);
        assert_eq!(end.col, 11);
    }

    #[test]
    fn test_no_match_clears_selection() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with known content
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("hello world");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type something that doesn't exist
        for c in "xyz".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // Buffer selection should be cleared
        let selection = state.buffer().selection_range();
        assert!(selection.is_none(), "Expected no selection when no match");
    }

    #[test]
    fn test_enter_advances_to_next_match() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with multiple occurrences
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("foo bar foo baz foo");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type "foo"
        for c in "foo".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // First match should be at position 0-3
        let selection1 = state.buffer().selection_range();
        assert!(selection1.is_some());
        let (start1, _) = selection1.unwrap();
        assert_eq!(start1.col, 0);

        // Press Enter to advance
        let enter = KeyEvent::new(Key::Return, Modifiers::default());
        state.handle_key(enter);

        // Second match should be at position 8-11
        let selection2 = state.buffer().selection_range();
        assert!(selection2.is_some());
        let (start2, _) = selection2.unwrap();
        assert_eq!(start2.col, 8);

        // Press Enter again
        let enter = KeyEvent::new(Key::Return, Modifiers::default());
        state.handle_key(enter);

        // Third match should be at position 16-19
        let selection3 = state.buffer().selection_range();
        assert!(selection3.is_some());
        let (start3, _) = selection3.unwrap();
        assert_eq!(start3.col, 16);
    }

    #[test]
    fn test_search_wraps_around() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with content and cursor near the end
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("hello world");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 8)); // After "world"

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type "hello" - should wrap around to find it at the beginning
        for c in "hello".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // Should find "hello" at position 0-5 (wrapped around)
        let selection = state.buffer().selection_range();
        assert!(selection.is_some(), "Expected to find 'hello' via wrap-around");
        let (start, end) = selection.unwrap();
        assert_eq!(start.col, 0);
        assert_eq!(end.col, 5);
    }

    #[test]
    fn test_case_insensitive_match() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with mixed case
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("Hello World HELLO");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type "hello" in lowercase
        for c in "hello".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // Should find "Hello" at position 0-5 (case-insensitive)
        let selection = state.buffer().selection_range();
        assert!(selection.is_some(), "Expected case-insensitive match");
        let (start, end) = selection.unwrap();
        assert_eq!(start.col, 0);
        assert_eq!(end.col, 5);
    }

    #[test]
    fn test_find_in_empty_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Buffer is empty

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type query - should not crash
        for c in "test".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // No match expected
        let selection = state.buffer().selection_range();
        assert!(selection.is_none());
    }

    #[test]
    fn test_empty_query_no_selection() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with content
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("hello world");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Empty query - no search should happen
        let selection = state.buffer().selection_range();
        assert!(selection.is_none());
    }

    #[test]
    fn test_cmd_f_does_not_insert_f() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));

        // Press Cmd+F
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Buffer should not have 'f' inserted
        assert_eq!(state.buffer().content(), "ab");
    }

    #[test]
    fn test_multiple_enter_advances_cycles_through_matches() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Set up buffer with two occurrences
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("ab ab");
        state.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));

        // Open find
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Type "ab"
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));

        // Debug: check the mini buffer content
        let mb_content = state.find_mini_buffer.as_ref().map(|mb| mb.content()).unwrap_or_default();
        eprintln!("Mini buffer content: {:?}", mb_content);
        eprintln!("Buffer content: {:?}", state.buffer().content());
        eprintln!("Focus: {:?}", state.focus);
        eprintln!("Selection: {:?}", state.buffer().selection_range());

        // First match at 0-2
        let s1 = state.buffer().selection_range().unwrap();
        assert_eq!(s1.0.col, 0);

        // Press Enter - second match at 3-5
        state.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));
        let s2 = state.buffer().selection_range().unwrap();
        assert_eq!(s2.0.col, 3);

        // Press Enter again - should wrap back to first match at 0-2
        state.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));
        let s3 = state.buffer().selection_range().unwrap();
        assert_eq!(s3.0.col, 0);
    }

    // =========================================================================
    // Rail offset mouse click tests
    // =========================================================================

    #[test]
    fn test_mouse_click_accounts_for_rail_offset() {
        // This test verifies that clicking in the content area positions the
        // cursor correctly, accounting for the left rail offset (RAIL_WIDTH).
        //
        // The bug: handle_mouse_buffer forwards raw window coordinates to the
        // buffer handler, but the buffer expects content-area-relative coords.
        // Without subtracting RAIL_WIDTH, clicks land ~7-8 columns to the right.
        use crate::left_rail::RAIL_WIDTH;
        use lite_edit_buffer::Position;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Set up buffer with known content
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("hello world");

        // Click at column 3 in the content area
        // Window x coordinate = RAIL_WIDTH + (column * glyph_width)
        // With RAIL_WIDTH = 56, glyph_width = 8, column = 3:
        // x = 56 + (3 * 8) = 56 + 24 = 80
        let target_column = 3;
        let glyph_width = test_font_metrics().advance_width; // 8.0
        let window_x = RAIL_WIDTH as f64 + (target_column as f64 * glyph_width);

        // y coordinate: we use flipped coordinates (origin at bottom)
        // Tab bar occupies top 32px (NSView y in [288, 320]).
        // Content area is NSView y in [0, 288).
        // Line 0 center: y = (view_height - TAB_BAR_HEIGHT) - line_height/2 = 320 - 32 - 8 = 280
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let content_top = 320.0 - TAB_BAR_HEIGHT as f64;
        let window_y = content_top - 8.0; // Center of line 0 in content area

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (window_x, window_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        // The cursor should be at column 3, not column 3 + (56/8) = column 10
        assert_eq!(
            state.buffer().cursor_position(),
            Position::new(0, target_column),
            "Cursor should be at column {} after clicking at window x={}, \
             but got column {}. This indicates RAIL_WIDTH ({}) is not being \
             subtracted from the x coordinate.",
            target_column,
            window_x,
            state.buffer().cursor_position().col,
            RAIL_WIDTH
        );
    }

    #[test]
    fn test_mouse_click_at_content_edge() {
        // Test clicking at the very left edge of content area (immediately
        // right of the rail) positions cursor at column 0
        use crate::left_rail::RAIL_WIDTH;
        use lite_edit_buffer::Position;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("hello world");

        // Click just to the right of the rail (at the content area edge)
        // Should position cursor at column 0
        // Tab bar occupies NSView y in [288, 320]; content area is [0, 288).
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let window_x = RAIL_WIDTH as f64 + 1.0; // Just past the rail
        let window_y = (320.0 - TAB_BAR_HEIGHT as f64) - 8.0; // Line 0 center in content area

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (window_x, window_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        assert_eq!(
            state.buffer().cursor_position(),
            Position::new(0, 0),
            "Clicking at the left edge of content area should place cursor at column 0"
        );
    }

    #[test]
    fn test_mouse_click_accounts_for_tab_bar_offset() {
        // Chunk: docs/chunks/tab_bar_layout_fixes - Test Y coordinate click targeting
        // This test verifies that clicking in the content area positions the
        // cursor at the correct LINE, accounting for the tab bar height.
        //
        // The content area starts below the tab bar. Clicks in the content area
        // should correctly map to buffer lines without being off by one.
        use crate::left_rail::RAIL_WIDTH;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Set up buffer with multiple lines
        *state.buffer_mut() = lite_edit_buffer::TextBuffer::from_str("line0\nline1\nline2\nline3");

        // Coordinate system explanation:
        // NSView uses bottom-left origin: y=0 is BOTTOM, y=view_height is TOP
        // Tab bar occupies the TOP 32px: NSView y in [288, 320)
        // Content area is NSView y in [0, 288)
        //
        // Within the content area:
        // - Line 0 is at the TOP of content (NSView y ≈ 288 - line_height)
        // - Line 1 is below line 0 (NSView y ≈ 288 - 2*line_height)
        //
        // To click on line 0:
        // - content_height = view_height - TAB_BAR_HEIGHT = 320 - 32 = 288
        // - Line 0 spans flipped_y ∈ [0, line_height) in content coords
        // - In NSView coords: y = content_height - flipped_y = 288 - (line_height/2) = 280

        let line_height = test_font_metrics().line_height; // 16.0
        let content_height = 320.0 - TAB_BAR_HEIGHT as f64;

        // Click on line 0 (center of line 0 in content area)
        let target_line = 0;
        let flipped_y_line0 = target_line as f64 * line_height + line_height / 2.0;
        let window_y_line0 = content_height - flipped_y_line0;
        let window_x = RAIL_WIDTH as f64 + 8.0; // Column 1

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (window_x, window_y_line0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        assert_eq!(
            state.buffer().cursor_position().line,
            target_line,
            "Clicking at center of line {} should place cursor on line {}, but got line {}. \
             This indicates TAB_BAR_HEIGHT ({}) is not being correctly accounted for.",
            target_line,
            target_line,
            state.buffer().cursor_position().line,
            TAB_BAR_HEIGHT
        );

        // Click on line 2
        let target_line = 2;
        let flipped_y_line2 = target_line as f64 * line_height + line_height / 2.0;
        let window_y_line2 = content_height - flipped_y_line2;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (window_x, window_y_line2),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        assert_eq!(
            state.buffer().cursor_position().line,
            target_line,
            "Clicking at center of line {} should place cursor on line {}, but got line {}.",
            target_line,
            target_line,
            state.buffer().cursor_position().line
        );
    }

    // Tab Command Tests (Chunk: docs/chunks/content_tab_bar)
    // =========================================================================

    #[test]
    fn test_switch_tab_changes_active_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }

        // Should have 2 tabs, active_tab is 1 (switched to new tab on add)
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);

        // Switch to first tab
        state.switch_tab(0);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_switch_tab_invalid_index_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only 1 tab exists
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);

        // Try to switch to invalid index
        let _ = state.take_dirty_region();
        state.switch_tab(5);

        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
        assert!(!state.is_dirty()); // No change, no dirty
    }

    #[test]
    fn test_close_tab_removes_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 2);

        // Close the first tab
        state.close_tab(0);

        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_close_last_tab_creates_new_empty_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only 1 tab exists
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);

        // Close the only tab - should create a new empty one
        state.close_tab(0);

        // Should still have 1 tab (new empty one)
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
    }

    #[test]
    fn test_next_tab_cycles_forward() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add two more tabs
        for _ in 0..2 {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        // Now have 3 tabs, active is 2 (last added)
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 3);

        // Switch to first tab
        state.switch_tab(0);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);

        // Next tab
        state.next_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);

        state.next_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 2);

        // Wrap around
        state.next_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
    }

    #[test]
    fn test_prev_tab_cycles_backward() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add two more tabs
        for _ in 0..2 {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        // Now have 3 tabs, active is 2 (last added)
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 3);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 2);

        // Previous tab
        state.prev_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);

        state.prev_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);

        // Wrap around
        state.prev_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 2);
    }

    #[test]
    fn test_next_tab_single_tab_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only 1 tab
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);

        let _ = state.take_dirty_region();
        state.next_tab();

        // Should remain unchanged
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_cmd_w_closes_active_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 2);

        // Cmd+W closes the active tab
        let cmd_w = KeyEvent::new(
            Key::Char('w'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_w);

        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
    }

    #[test]
    fn test_cmd_shift_right_bracket_next_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        // Switch to first tab
        state.switch_tab(0);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);

        // Cmd+Shift+] cycles to next tab
        let cmd_shift_bracket = KeyEvent::new(
            Key::Char(']'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_bracket);

        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);
    }

    #[test]
    fn test_cmd_shift_left_bracket_prev_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        // Active tab is 1 (new tab)
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);

        // Cmd+Shift+[ cycles to previous tab
        let cmd_shift_bracket = KeyEvent::new(
            Key::Char('['),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_bracket);

        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
    }

    #[test]
    fn test_close_active_tab_method() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);

        // Close active tab
        state.close_active_tab();

        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);
    }

    // =========================================================================
    // Cmd+T New Tab Tests (Chunk: docs/chunks/content_tab_bar)
    // =========================================================================

    #[test]
    fn test_cmd_t_creates_new_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Initially one tab
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 0);

        // Cmd+T creates a new tab
        let cmd_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_t);

        // Should have 2 tabs, active tab is 1 (switched to new tab)
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);
    }

    #[test]
    fn test_cmd_t_does_not_insert_t() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Cmd+T should NOT insert 't' into buffer
        let cmd_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_t);

        // Buffer should be empty
        assert!(state.buffer().is_empty());
    }

    #[test]
    fn test_cmd_shift_t_does_not_create_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Initially one tab
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);

        // Cmd+Shift+T should NOT create a new tab (reserved for future use)
        let cmd_shift_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_t);

        // Should still have 1 tab (Cmd+Shift+T is not handled)
        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);
    }

    #[test]
    fn test_new_tab_method() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 1);

        state.new_tab();

        assert_eq!(state.editor.active_workspace().unwrap().tabs.len(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab, 1);
    }

    #[test]
    fn test_new_tab_marks_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Clear any existing dirty state
        let _ = state.take_dirty_region();

        state.new_tab();

        assert!(state.is_dirty());
    }
}
