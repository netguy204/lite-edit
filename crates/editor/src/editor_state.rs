// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/viewport_scrolling - Scroll event handling
// Chunk: docs/chunks/quit_command - Cmd+Q quit flag and key interception
// Chunk: docs/chunks/file_picker - File picker (Cmd+P) integration
// Chunk: docs/chunks/file_save - File-buffer association and Cmd+S save
// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
// Chunk: docs/chunks/tab_bar_interaction - Click coordinate transformation for tab switching
// Chunk: docs/chunks/workspace_dir_picker - Directory picker for new workspaces
// Chunk: docs/chunks/pty_wakeup_reentrant - EventSender for PTY wakeup
// Chunk: docs/chunks/split_tab_click - Multi-pane tab bar click routing
//!
//! Editor state container.
//!
//! This module consolidates all mutable editor state into a single struct
//! that the main loop can work with. It provides the EditorContext for
//! focus target event handling.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::buffer_target::BufferFocusTarget;
// Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog import
// Chunk: docs/chunks/generic_yes_no_modal - ConfirmDialogContext and geometry import
use crate::confirm_dialog::{
    calculate_confirm_dialog_geometry, ConfirmDialog, ConfirmDialogContext,
};
use crate::context::EditorContext;
use crate::dir_picker;
// Chunk: docs/chunks/file_open_picker - File picker for opening files via Cmd+O
use crate::file_picker;
use crate::dirty_region::{DirtyRegion, InvalidationKind};
// Chunk: docs/chunks/pty_wakeup_reentrant - EventSender for PTY wakeup
use crate::event_channel::EventSender;
// Chunk: docs/chunks/file_change_events - Self-write suppression
use crate::file_change_suppression::FileChangeSuppression;
// Chunk: docs/chunks/buffer_file_watching - Per-buffer file watching
use crate::buffer_file_watcher::BufferFileWatcher;
// Chunk: docs/chunks/focus_stack - FocusLayer import for focus state bridge
// Chunk: docs/chunks/focus_stack - FocusStack import for stack-based focus management
use crate::focus::{FocusLayer, FocusStack, FocusTarget};
// Chunk: docs/chunks/focus_stack - Focus target imports for stack integration
use crate::global_shortcuts::GlobalShortcutTarget;
use crate::selector_target::SelectorFocusTarget;
use crate::find_target::FindFocusTarget;
use crate::confirm_dialog_target::ConfirmDialogFocusTarget;
use crate::font::FontMetrics;
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};
use crate::left_rail::{calculate_left_rail_geometry, RAIL_WIDTH};
use crate::mini_buffer::MiniBuffer;
use crate::pane_layout::PaneId;
// Chunk: docs/chunks/content_tab_bar - Tab bar click handling
// Chunk: docs/chunks/split_tab_click - Multi-pane tab bar click routing
use crate::tab_bar::{
    calculate_pane_tab_bar_geometry, calculate_tab_bar_geometry, tabs_from_pane,
    tabs_from_workspace, TAB_BAR_HEIGHT,
};
use crate::selector::{SelectorOutcome, SelectorWidget};
use crate::selector_overlay::calculate_overlay_geometry;
use crate::viewport::Viewport;
use crate::workspace::Editor;
// Chunk: docs/chunks/styled_line_cache - DirtyLines for cache invalidation tracking
use lite_edit_buffer::{DirtyLines, Position, TextBuffer};
// Chunk: docs/chunks/syntax_highlighting - Syntax highlighting support
use lite_edit_syntax::{LanguageRegistry, SyntaxTheme};
// Chunk: docs/chunks/dragdrop_file_paste - Shell escaping for dropped file paths
use lite_edit::shell_escape::shell_escape_paths;
// Chunk: docs/chunks/terminal_active_tab_safety - Terminal input encoding
// Chunk: docs/chunks/terminal_scrollback_viewport - Terminal scroll action result
// Chunk: docs/chunks/terminal_pty_wakeup - Run-loop wakeup for PTY output
// Chunk: docs/chunks/pty_wakeup_reentrant - WakeupSignal trait for cross-crate PTY wakeup
use lite_edit_terminal::{BufferView, InputEncoder, PtyWakeup, TermMode};

/// Duration in milliseconds for cursor blink interval
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// Which UI element currently owns keyboard/mouse focus.
/// Chunk: docs/chunks/file_picker - Focus mode enum distinguishing Buffer vs Selector editing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorFocus {
    /// Normal buffer editing mode
    #[default]
    Buffer,
    /// Selector overlay is active (file picker, command palette, etc.)
    Selector,
    // Chunk: docs/chunks/find_in_file - Find-in-file focus variant
    /// Find-in-file strip is active
    FindInFile,
    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog focus variant
    /// Confirm dialog is active (e.g., abandon unsaved changes?)
    ConfirmDialog,
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
/// Chunk: docs/chunks/file_picker - File picker state fields (focus, active_selector, resolved_path)
// Chunk: docs/chunks/workspace_dir_picker - file_index and last_cache_version moved to Workspace
pub struct EditorState {
    /// The workspace/tab model containing all buffers and viewports
    pub editor: Editor,
    // Chunk: docs/chunks/invalidation_separation - Invalidation kind tracking
    /// Accumulated invalidation for the current event batch.
    ///
    /// This replaces the previous `dirty_region` field to support distinct
    /// Content, Layout, and Overlay invalidation kinds. The renderer uses
    /// this to skip pane rect recalculation for Content-only frames.
    pub invalidation: InvalidationKind,
    // Chunk: docs/chunks/styled_line_cache - Buffer-level dirty tracking for cache invalidation
    /// Accumulated buffer-level dirty lines for styled line cache invalidation.
    /// This tracks which buffer lines have changed since the last render, allowing
    /// fine-grained cache invalidation instead of clearing the entire cache.
    pub dirty_lines: DirtyLines,
    // Chunk: docs/chunks/styled_line_cache - Clear cache flag for tab switch
    /// When true, the styled line cache should be fully cleared on next render.
    /// Set to true on tab switch to prevent stale cache entries from a previous
    /// buffer causing visual artifacts.
    pub clear_styled_line_cache: bool,
    /// The active focus target (currently always the buffer target)
    pub focus_target: BufferFocusTarget,
    // Chunk: docs/chunks/focus_stack - Focus stack for composable focus targets
    /// The focus stack for event propagation.
    ///
    /// Stack structure (bottom to top):
    /// - Index 0: GlobalShortcutTarget (handles Cmd+Q, Cmd+S, etc.)
    /// - Index 1: BufferFocusTarget (handles buffer editing)
    /// - Index 2+: [optional overlays] - selector, find bar, confirm dialog
    pub focus_stack: FocusStack,
    /// Whether the cursor is currently visible (for blink animation)
    pub cursor_visible: bool,
    /// Time of the last keystroke (for cursor blink reset)
    pub last_keystroke: Instant,
    /// Whether the overlay cursor is currently visible (for blink animation)
    /// Chunk: docs/chunks/cursor_blink_focus - Separate blink state for overlay mini-buffers
    pub overlay_cursor_visible: bool,
    /// Time of the last overlay keystroke (for overlay cursor blink reset)
    /// Chunk: docs/chunks/cursor_blink_focus - Separate keystroke tracking for overlays
    pub last_overlay_keystroke: Instant,
    /// Font metrics for pixel-to-position conversion
    font_metrics: FontMetrics,
    /// View height in pixels (for y-coordinate flipping in mouse events)
    view_height: f32,
    /// View width in pixels (for selector overlay geometry)
    view_width: f32,
    /// Whether the app should quit (set by Cmd+Q)
    // Chunk: docs/chunks/quit_command - Quit flag field set by Cmd+Q
    pub should_quit: bool,
    /// Which UI element currently owns focus
    pub focus: EditorFocus,
    /// The active selector widget (when focus == Selector)
    pub active_selector: Option<SelectorWidget>,
    /// The resolved path from the last selector confirmation
    /// (consumed by file_save chunk for buffer association)
    pub resolved_path: Option<PathBuf>,
    // Chunk: docs/chunks/find_in_file - Find-in-file mode state
    /// The MiniBuffer for the find query (when focus == FindInFile)
    pub find_mini_buffer: Option<MiniBuffer>,
    /// The buffer position from which the current search started
    /// (used as the search origin; only advances when Enter is pressed)
    pub search_origin: Position,
    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog state
    // Chunk: docs/chunks/generic_yes_no_modal - Replaced pending_close with confirm_context
    /// The active confirm dialog (when focus == ConfirmDialog)
    pub confirm_dialog: Option<ConfirmDialog>,
    /// Context for what triggered the confirm dialog and what action to take on confirmation.
    /// Replaces the previous `pending_close` field to support multiple dialog use cases.
    pub confirm_context: Option<ConfirmDialogContext>,
    // Chunk: docs/chunks/pty_wakeup_reentrant - EventSender for PTY wakeup
    /// Event sender for creating PTY wakeup handles.
    /// Set by main.rs during setup. PtyWakeup handles signal through this sender.
    event_sender: Option<EventSender>,
    // Chunk: docs/chunks/syntax_highlighting - Language registry for extension lookup
    /// Language registry for syntax highlighting.
    language_registry: LanguageRegistry,
    // Chunk: docs/chunks/file_change_events - Self-write suppression
    /// Registry of paths whose file change events should be suppressed.
    /// Prevents our own file saves from triggering reload/merge flows.
    file_change_suppression: FileChangeSuppression,
    // Chunk: docs/chunks/buffer_file_watching - Per-buffer file watching
    /// Per-buffer file watcher for files outside the workspace.
    /// Manages watchers for files opened via Cmd+O from external directories.
    buffer_file_watcher: BufferFileWatcher,
    /// Flag set by Ctrl+Shift+P to trigger an on-demand perf stats dump.
    #[cfg(feature = "perf-instrumentation")]
    pub dump_perf_stats: bool,
}

// =============================================================================
// Helper functions
// =============================================================================

/// Clamp a cursor position to be valid within the given buffer.
///
/// The line is clamped to `[0, line_count - 1]` (or 0 for empty buffers).
/// The column is clamped to `[0, line_length]` for the clamped line.
// Chunk: docs/chunks/base_snapshot_reload - Cursor clamping after reload
pub fn clamp_position_to_buffer(pos: Position, buffer: &TextBuffer) -> Position {
    let line_count = buffer.line_count();
    if line_count == 0 {
        return Position::new(0, 0);
    }

    let line = pos.line.min(line_count - 1);
    let line_len = buffer.line_len(line);
    let col = pos.col.min(line_len);

    Position::new(line, col)
}

// =============================================================================
// Delegate accessors for backward compatibility
// =============================================================================

// Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
impl EditorState {
    /// Returns a reference to the active tab's buffer.
    ///
    /// The resolution chain is: active_workspace → active_pane → active_tab → buffer.
    ///
    /// # Panics
    /// Panics if there is no active workspace, active pane, or active tab (should never happen
    /// as EditorState always creates at least one workspace with one pane and one tab).
    pub fn buffer(&self) -> &TextBuffer {
        self.editor
            .active_workspace()
            .expect("no active workspace")
            .active_pane()
            .expect("no active pane")
            .active_tab()
            .expect("no active tab")
            .as_text_buffer()
            .expect("active tab is not a file tab")
    }

    /// Returns a mutable reference to the active tab's buffer.
    ///
    /// # Panics
    /// Panics if there is no active workspace, active pane, or active tab.
    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        self.editor
            .active_workspace_mut()
            .expect("no active workspace")
            .active_pane_mut()
            .expect("no active pane")
            .active_tab_mut()
            .expect("no active tab")
            .as_text_buffer_mut()
            .expect("active tab is not a file tab")
    }

    // Chunk: docs/chunks/terminal_active_tab_safety - Safe Option-returning accessors
    /// Returns a reference to the active tab's TextBuffer, if it's a file tab.
    ///
    /// Returns `None` if the active tab is a terminal or other non-file tab type.
    /// Use this method in code paths that need to gracefully handle terminal tabs.
    pub fn try_buffer(&self) -> Option<&TextBuffer> {
        self.editor
            .active_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.as_text_buffer())
    }

    /// Returns a mutable reference to the active tab's TextBuffer, if it's a file tab.
    ///
    /// Returns `None` if the active tab is a terminal or other non-file tab type.
    /// Use this method in code paths that need to gracefully handle terminal tabs.
    pub fn try_buffer_mut(&mut self) -> Option<&mut TextBuffer> {
        self.editor
            .active_workspace_mut()
            .and_then(|ws| ws.active_pane_mut())
            .and_then(|pane| pane.active_tab_mut())
            .and_then(|tab| tab.as_text_buffer_mut())
    }

    // Chunk: docs/chunks/terminal_active_tab_safety - Cheap check for non-file tabs
    /// Returns true if the active tab is a file tab (has a TextBuffer).
    ///
    /// This is a cheap check for code paths that need to early-return when
    /// the active tab is not a file tab (e.g., terminal, agent output).
    pub fn active_tab_is_file(&self) -> bool {
        self.try_buffer().is_some()
    }

    /// Returns a reference to the active tab's viewport.
    ///
    /// # Panics
    /// Panics if there is no active workspace, active pane, or active tab.
    pub fn viewport(&self) -> &Viewport {
        &self.editor
            .active_workspace()
            .expect("no active workspace")
            .active_pane()
            .expect("no active pane")
            .active_tab()
            .expect("no active tab")
            .viewport
    }

    /// Returns a mutable reference to the active tab's viewport.
    ///
    /// # Panics
    /// Panics if there is no active workspace, active pane, or active tab.
    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.editor
            .active_workspace_mut()
            .expect("no active workspace")
            .active_pane_mut()
            .expect("no active pane")
            .active_tab_mut()
            .expect("no active tab")
            .viewport
    }

    /// Returns a reference to the active tab's associated file path.
    // Chunk: docs/chunks/file_save - Getter for active tab's associated file path
    pub fn associated_file(&self) -> Option<&PathBuf> {
        self.editor
            .active_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.associated_file.as_ref())
    }

    /// Sets the associated file for the active tab.
    fn set_associated_file(&mut self, path: Option<PathBuf>) {
        if let Some(ws) = self.editor.active_workspace_mut() {
            if let Some(pane) = ws.active_pane_mut() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.associated_file = path;
                }
            }
        }
    }

    // Chunk: docs/chunks/file_change_events - Self-write suppression check
    /// Checks if a file change event should be suppressed (was our own write).
    ///
    /// Call this when receiving a FileChanged event. If it returns true, the
    /// event should be ignored (it was triggered by our own save operation).
    /// The suppression entry is consumed (one-shot behavior).
    ///
    /// # Arguments
    ///
    /// * `path` - The path from the FileChanged event (absolute)
    pub fn is_file_change_suppressed(&mut self, path: &Path) -> bool {
        self.file_change_suppression.check(path)
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

        // FileIndex is now initialized per-workspace in Workspace::new() and Workspace::with_empty_tab()
        // Chunk: docs/chunks/workspace_dir_picker - Per-workspace file index
        // Chunk: docs/chunks/focus_stack - Initialize focus stack with global shortcuts and buffer target
        // The focus stack provides composable focus handling with event propagation.
        // Stack structure (bottom to top):
        // - GlobalShortcutTarget: handles Cmd+Q, Cmd+S, etc. (always at bottom)
        // - BufferFocusTarget: handles buffer editing (always present)
        // - [overlays]: selector, find bar, confirm dialog (pushed/popped as needed)
        let mut focus_stack = FocusStack::new();
        focus_stack.push(Box::new(GlobalShortcutTarget::new()));
        focus_stack.push(Box::new(BufferFocusTarget::new()));

        Self {
            editor,
            // Chunk: docs/chunks/invalidation_separation - Initialize invalidation
            invalidation: InvalidationKind::None,
            dirty_lines: DirtyLines::None,
            // Chunk: docs/chunks/styled_line_cache - Initialize cache clear flag
            clear_styled_line_cache: false,
            focus_target: BufferFocusTarget::new(),
            focus_stack,
            cursor_visible: true,
            last_keystroke: Instant::now(),
            // Chunk: docs/docs/cursor_blink_focus - Initialize overlay cursor state
            overlay_cursor_visible: true,
            last_overlay_keystroke: Instant::now(),
            font_metrics,
            view_height: 0.0,
            // Default to a large width to prevent unintended wrapping in tests
            // Chunk: docs/chunks/line_wrap_rendering - Large default to avoid test breakage
            view_width: 10000.0,
            should_quit: false,
            focus: EditorFocus::Buffer,
            active_selector: None,
            resolved_path: None,
            find_mini_buffer: None,
            search_origin: Position::new(0, 0),
            // Chunk: docs/chunks/dirty_tab_close_confirm - Initialize confirm dialog state
            // Chunk: docs/chunks/generic_yes_no_modal - Use confirm_context instead of pending_close
            confirm_dialog: None,
            confirm_context: None,
            // Chunk: docs/chunks/terminal_pty_wakeup - Initialize wakeup factory as None
            event_sender: None,
            // Chunk: docs/chunks/syntax_highlighting - Initialize language registry
            language_registry: LanguageRegistry::new(),
            // Chunk: docs/chunks/file_change_events - Initialize self-write suppression
            file_change_suppression: FileChangeSuppression::new(),
            // Chunk: docs/chunks/buffer_file_watching - Initialize per-buffer file watcher
            buffer_file_watcher: BufferFileWatcher::new(),
            #[cfg(feature = "perf-instrumentation")]
            dump_perf_stats: false,
        }
    }

    /// Creates an EditorState with an empty buffer.
    pub fn empty(font_metrics: FontMetrics) -> Self {
        Self::new(TextBuffer::new(), font_metrics)
    }

    // Chunk: docs/chunks/startup_workspace_dialog - Deferred initialization for startup dialog
    /// Creates an EditorState with no workspaces.
    ///
    /// This constructor is used during application startup when the workspace
    /// directory needs to be selected via a dialog before creating any workspaces.
    /// The editor will have no active workspace, buffer, or viewport until
    /// `add_startup_workspace()` is called.
    ///
    /// # Arguments
    ///
    /// * `font_metrics` - Font metrics for pixel-to-position conversion
    pub fn new_deferred(font_metrics: FontMetrics) -> Self {
        let line_height = font_metrics.line_height as f32;

        // Create editor with no workspaces
        let editor = Editor::new_deferred(line_height);

        // Chunk: docs/chunks/focus_stack - Initialize focus stack with global shortcuts and buffer target
        let mut focus_stack = FocusStack::new();
        focus_stack.push(Box::new(GlobalShortcutTarget::new()));
        focus_stack.push(Box::new(BufferFocusTarget::new()));

        Self {
            editor,
            // Chunk: docs/chunks/invalidation_separation - Initialize invalidation
            invalidation: InvalidationKind::None,
            dirty_lines: DirtyLines::None,
            // Chunk: docs/chunks/styled_line_cache - Initialize cache clear flag
            clear_styled_line_cache: false,
            focus_target: BufferFocusTarget::new(),
            focus_stack,
            cursor_visible: true,
            last_keystroke: Instant::now(),
            overlay_cursor_visible: true,
            last_overlay_keystroke: Instant::now(),
            font_metrics,
            view_height: 0.0,
            view_width: 10000.0,
            should_quit: false,
            focus: EditorFocus::Buffer,
            active_selector: None,
            resolved_path: None,
            find_mini_buffer: None,
            search_origin: Position::new(0, 0),
            // Chunk: docs/chunks/dirty_tab_close_confirm - Initialize confirm dialog state
            // Chunk: docs/chunks/generic_yes_no_modal - Use confirm_context instead of pending_close
            confirm_dialog: None,
            confirm_context: None,
            event_sender: None,
            language_registry: LanguageRegistry::new(),
            // Chunk: docs/chunks/file_change_events - Initialize self-write suppression
            file_change_suppression: FileChangeSuppression::new(),
            // Chunk: docs/chunks/buffer_file_watching - Initialize per-buffer file watcher
            buffer_file_watcher: BufferFileWatcher::new(),
            #[cfg(feature = "perf-instrumentation")]
            dump_perf_stats: false,
        }
    }

    // Chunk: docs/chunks/startup_workspace_dialog - Add initial workspace after directory selection
    /// Adds the initial workspace during application startup.
    ///
    /// This method is called after `new_deferred()` once the user has selected
    /// a directory via the startup dialog. It creates the first workspace and
    /// adds an empty tab for the welcome screen.
    ///
    /// # Arguments
    ///
    /// * `root_path` - The root directory for the workspace's file index
    pub fn add_startup_workspace(&mut self, root_path: PathBuf) {
        // Derive workspace label from directory name
        let label = root_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".to_string());

        // Create the workspace with the selected directory
        self.editor.new_workspace(label, root_path.clone());

        // Chunk: docs/chunks/buffer_file_watching - Set initial workspace root
        // Set the buffer file watcher's workspace root for the initial workspace.
        self.buffer_file_watcher.set_workspace_root(root_path);

        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Returns the font metrics.
    pub fn font_metrics(&self) -> &FontMetrics {
        &self.font_metrics
    }

    // Chunk: docs/chunks/focus_stack - Bridge from EditorFocus enum to FocusLayer
    /// Returns the current focus layer.
    ///
    /// This method bridges the existing `EditorFocus` enum to the new `FocusLayer`
    /// type used by the focus stack architecture. The renderer uses this to
    /// determine which overlay to render.
    pub fn focus_layer(&self) -> FocusLayer {
        // Chunk: docs/chunks/focus_stack - Use focus_stack.top_layer() for rendering decisions
        // The focus stack's top layer determines what overlay (if any) should be rendered.
        // This replaces the previous EditorFocus enum match.
        self.focus_stack.top_layer()
    }

    // Chunk: docs/chunks/pty_wakeup_reentrant - EventSender for PTY wakeup
    // Chunk: docs/chunks/buffer_file_watching - Wire up buffer file watcher callback
    /// Sets the event sender for creating PTY wakeup handles.
    ///
    /// The sender is cloned when spawning new terminals to create PtyWakeup
    /// handles that signal through the unified event queue.
    ///
    /// Also wires up the buffer file watcher callback to send FileChanged events
    /// through the event channel.
    pub fn set_event_sender(&mut self, sender: EventSender) {
        // Chunk: docs/chunks/buffer_file_watching - Wire buffer file watcher callback
        // Clone sender for the buffer file watcher callback
        let event_sender_for_buffer_watcher = sender.clone();
        self.buffer_file_watcher.set_callback(Box::new(move |path| {
            let _ = event_sender_for_buffer_watcher.send_file_changed(path);
        }));

        // Set workspace root for buffer file watcher (if workspace exists)
        if let Some(ws) = self.editor.active_workspace() {
            self.buffer_file_watcher.set_workspace_root(ws.root_path.clone());
        }

        self.event_sender = Some(sender);
    }

    // Chunk: docs/chunks/pty_wakeup_reentrant - Creates PtyWakeup with WakeupSignal trait
    /// Creates a PTY wakeup handle using the stored event sender.
    ///
    /// Returns `None` if no event sender has been set.
    ///
    /// The returned `PtyWakeup` will signal through the event channel when
    /// PTY data arrives, avoiding the reentrant borrow issues of the old
    /// global callback approach.
    pub fn create_pty_wakeup(&self) -> Option<PtyWakeup> {
        self.event_sender.as_ref().map(|sender| {
            PtyWakeup::with_signal(Box::new(sender.clone()))
        })
    }

    /// Updates the viewport size based on window dimensions in pixels.
    ///
    /// This also updates the stored view_height and view_width for
    /// mouse event coordinate flipping and selector overlay geometry.
    // Chunk: docs/chunks/resize_click_alignment - Pass line count for scroll clamping
    // Chunk: docs/chunks/scroll_max_last_line - Pass content_height to viewport
    // Chunk: docs/chunks/scroll_max_last_line - Pass content_height to viewport
    // Chunk: docs/chunks/find_strip_scroll_clearance - Subtract TAB_BAR_HEIGHT for correct visible_lines
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    pub fn update_viewport_size(&mut self, window_height: f32) {
        // Terminal tabs don't have a TextBuffer line count; use 0 which is harmless
        // since terminals don't use the Viewport in the same way as file tabs.
        let line_count = self.try_buffer().map(|b| b.line_count()).unwrap_or(0);
        // Pass content_height (window_height minus tab bar) to viewport so visible_lines
        // is computed correctly. The tab bar occupies the top TAB_BAR_HEIGHT pixels.
        let content_height = window_height - TAB_BAR_HEIGHT;
        self.viewport_mut().update_size(content_height, line_count);
        self.view_height = window_height; // Keep full height for coordinate flipping

        // Chunk: docs/chunks/split_scroll_viewport - Sync all pane viewports on resize
        // When window height changes, all panes change geometry. Update each
        // tab's viewport to reflect the new pane content heights.
        // Note: This may be called before view_width is set, so sync_pane_viewports
        // will early-return if dimensions are incomplete.
        self.sync_pane_viewports();
    }

    /// Updates the viewport size with both width and height.
    ///
    /// This is the preferred method when both dimensions are available.
    // Chunk: docs/chunks/resize_click_alignment - Pass line count for scroll clamping
    // Chunk: docs/chunks/scroll_max_last_line - Pass content_height to viewport
    // Chunk: docs/chunks/find_strip_scroll_clearance - Subtract TAB_BAR_HEIGHT for correct visible_lines
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    pub fn update_viewport_dimensions(&mut self, window_width: f32, window_height: f32) {
        // Terminal tabs don't have a TextBuffer line count; use 0 which is harmless
        // since terminals don't use the Viewport in the same way as file tabs.
        let line_count = self.try_buffer().map(|b| b.line_count()).unwrap_or(0);
        // Pass content_height (window_height minus tab bar) to viewport so visible_lines
        // is computed correctly. The tab bar occupies the top TAB_BAR_HEIGHT pixels.
        let content_height = window_height - TAB_BAR_HEIGHT;
        self.viewport_mut().update_size(content_height, line_count);
        self.view_height = window_height; // Keep full height for coordinate flipping
        self.view_width = window_width;

        // Chunk: docs/chunks/split_scroll_viewport - Sync all pane viewports on resize
        // When window dimensions change, all panes change geometry. Update each
        // tab's viewport to reflect the new pane content heights.
        self.sync_pane_viewports();
    }

    /// Syncs the active tab's viewport to the current window dimensions.
    ///
    /// This must be called whenever a tab becomes active (via `new_tab`,
    /// `switch_tab`, or file picker confirmation) to ensure its viewport has
    /// the correct `visible_lines` value for dirty region calculations.
    ///
    /// Skips syncing for non-file tabs (e.g., terminals) which don't have
    /// a `TextBuffer` and use a different rendering path.
    // Chunk: docs/chunks/tab_click_cursor_placement - Viewport sync on tab activation
    // Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
    fn sync_active_tab_viewport(&mut self) {
        // Skip if view_height hasn't been set yet (initial state before first resize)
        let view_height = self.view_height;
        if view_height == 0.0 {
            return;
        }

        // Get the line count from the active tab's text buffer, if it exists.
        // Terminal tabs don't have a TextBuffer, so we skip viewport sync for them.
        let line_count = match self.editor.active_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.as_text_buffer())
        {
            Some(buf) => buf.line_count(),
            None => return, // Non-file tab, skip viewport sync
        };

        // Sync the viewport to the content height (window height minus tab bar).
        // This matches update_viewport_size/update_viewport_dimensions which also
        // subtract TAB_BAR_HEIGHT to compute visible_lines correctly.
        let content_height = view_height - TAB_BAR_HEIGHT;
        self.viewport_mut().update_size(content_height, line_count);
    }

    // Chunk: docs/chunks/split_scroll_viewport - Per-pane viewport synchronization
    /// Syncs all tabs' viewports to their respective pane content heights.
    ///
    /// This must be called whenever the pane layout changes (e.g., after a split
    /// or window resize) to ensure each tab's viewport has the correct
    /// `visible_lines` value for scroll clamping and dirty region calculations.
    ///
    /// Unlike `sync_active_tab_viewport()` which only updates the active tab,
    /// this method updates all tabs in all panes to handle multi-pane layouts.
    ///
    /// The method computes pane rectangles from the current window dimensions,
    /// then updates each tab's viewport with the correct pane content height.
    // Chunk: docs/chunks/terminal_resize_sync - Propagate resize to terminal grid
    fn sync_pane_viewports(&mut self) {
        use crate::pane_layout::calculate_pane_rects;

        // Skip if view dimensions haven't been set yet
        let view_width = self.view_width;
        let view_height = self.view_height;
        if view_width == 0.0 || view_height == 0.0 {
            return;
        }

        // Calculate the content area (excludes left rail in single-pane, full width in multi-pane)
        // Note: In multi-pane mode, the content bounds are relative to the content area
        // which starts after RAIL_WIDTH horizontally, at top of window vertically.
        let content_width = view_width - RAIL_WIDTH;
        let content_height = view_height;

        // Chunk: docs/chunks/terminal_resize_sync - Cache font metrics for terminal resize calculations
        let line_height = self.font_metrics.line_height;
        let advance_width = self.font_metrics.advance_width;

        // Early return if no workspace
        let workspace = match self.editor.active_workspace_mut() {
            Some(ws) => ws,
            None => return,
        };

        // Calculate pane rects for the current layout
        let bounds = (0.0, 0.0, content_width, content_height);
        let pane_rects = calculate_pane_rects(bounds, &workspace.pane_root);

        // Update each pane's tabs with the correct viewport dimensions
        for pane_rect in &pane_rects {
            // Get the pane by ID
            let pane = match workspace.pane_root.get_pane_mut(pane_rect.pane_id) {
                Some(p) => p,
                None => continue,
            };

            // Calculate the pane's content height and width (pane height minus tab bar)
            let pane_content_height = pane_rect.height - TAB_BAR_HEIGHT;
            let pane_width = pane_rect.width;

            // Update each tab's viewport in this pane
            for tab in &mut pane.tabs {
                // Chunk: docs/chunks/terminal_resize_sync - Resize terminal grid on layout change
                // For terminal tabs, resize the alacritty grid to match the new pane dimensions.
                // This ensures hosted programs (Claude Code, vim, htop) see the correct terminal
                // size via TIOCGWINSZ and position their cursors accurately.
                if let Some(terminal) = tab.as_terminal_buffer_mut() {
                    let rows = (pane_content_height as f64 / line_height).floor() as usize;
                    let cols = (pane_width as f64 / advance_width).floor() as usize;

                    // Only resize if dimensions actually changed (avoid PTY thrashing)
                    let (current_cols, current_rows) = terminal.size();
                    if (cols != current_cols || rows != current_rows) && cols > 0 && rows > 0 {
                        terminal.resize(cols, rows);
                    }
                }

                // Get the line count for this tab's content
                // File tabs use TextBuffer line count
                // Terminal tabs use their terminal's line count
                let line_count = if let Some(text_buffer) = tab.as_text_buffer() {
                    text_buffer.line_count()
                } else if let Some((terminal, _)) = tab.terminal_and_viewport_mut() {
                    terminal.line_count()
                } else {
                    // Unknown tab type, skip
                    continue;
                };

                // Update the tab's viewport with the pane's content height
                tab.viewport.update_size(pane_content_height, line_count);
            }
        }
    }

    // Chunk: docs/chunks/vsplit_scroll - Pane content dimensions helper
    /// Returns the content dimensions (height, width) for a specific pane.
    ///
    /// This is used by scroll handlers to create an `EditorContext` with the
    /// correct pane-local dimensions rather than full-window dimensions. Without
    /// this, scroll clamping in split panes would use incorrect wrap calculations.
    ///
    /// # Arguments
    ///
    /// * `pane_id` - The ID of the pane to look up
    ///
    /// # Returns
    ///
    /// `Some((content_height, content_width))` if the pane is found, `None` otherwise.
    /// The content height is the pane height minus the tab bar height.
    fn get_pane_content_dimensions(&self, pane_id: PaneId) -> Option<(f32, f32)> {
        use crate::pane_layout::calculate_pane_rects;

        // Skip if view dimensions haven't been set yet
        if self.view_width == 0.0 || self.view_height == 0.0 {
            return None;
        }

        let workspace = self.editor.active_workspace()?;

        // Calculate content area (excludes left rail)
        let content_width = self.view_width - RAIL_WIDTH;
        let content_height = self.view_height;
        let bounds = (0.0, 0.0, content_width, content_height);

        // Calculate pane rects and find the target pane
        let pane_rects = calculate_pane_rects(bounds, &workspace.pane_root);

        for pane_rect in pane_rects {
            if pane_rect.pane_id == pane_id {
                // Pane content height excludes the tab bar
                let pane_content_height = pane_rect.height - TAB_BAR_HEIGHT;
                let pane_content_width = pane_rect.width;
                return Some((pane_content_height, pane_content_width));
            }
        }

        None
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
    // Chunk: docs/chunks/quit_command - Intercepts Cmd+Q before delegating to focus target
    // Chunk: docs/chunks/file_picker - Cmd+P interception and focus-based key routing
    // Chunk: docs/chunks/terminal_paste_render - Paste handler without premature dirty marking
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

            // Cmd+O (without Ctrl) opens system file picker
            // Chunk: docs/chunks/file_open_picker
            if let Key::Char('o') = event.key {
                self.handle_cmd_o();
                return;
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

            // Chunk: docs/chunks/workspace_switching - Workspace cycling shortcuts
            // Cmd+] (without Shift) cycles to next workspace
            if let Key::Char(']') = event.key {
                if !event.modifiers.shift {
                    self.next_workspace();
                    return;
                }
            }

            // Cmd+[ (without Shift) cycles to previous workspace
            if let Key::Char('[') = event.key {
                if !event.modifiers.shift {
                    self.prev_workspace();
                    return;
                }
            }

            // Chunk: docs/chunks/content_tab_bar - Create new tab
            // Cmd+T creates a new empty tab in the current workspace
            // Chunk: docs/chunks/terminal_tab_spawn - Cmd+Shift+T creates a new terminal tab
            if let Key::Char('t') = event.key {
                if event.modifiers.shift {
                    self.new_terminal_tab();
                    return;
                } else {
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

            // Chunk: docs/chunks/tiling_focus_keybindings - Directional tab movement
            // Cmd+Shift+Arrow moves the active tab in that direction
            if event.modifiers.shift {
                use crate::pane_layout::{Direction, MoveResult};

                let direction = match event.key {
                    Key::Right => Some(Direction::Right),
                    Key::Left => Some(Direction::Left),
                    Key::Down => Some(Direction::Down),
                    Key::Up => Some(Direction::Up),
                    _ => None,
                };

                if let Some(dir) = direction {
                    if let Some(workspace) = self.editor.active_workspace_mut() {
                        let result = workspace.move_active_tab(dir);
                        match result {
                            MoveResult::MovedToExisting { .. } | MoveResult::MovedToNew { .. } => {
                                self.invalidation.merge(InvalidationKind::Layout);
                            }
                            MoveResult::Rejected | MoveResult::SourceNotFound => {
                                // No-op, no visual change
                            }
                        }
                        // Chunk: docs/chunks/split_scroll_viewport - Sync viewports after split
                        // After any tab movement (including splits), sync all pane viewports
                        // to ensure tabs have correct visible_lines for their new pane geometry.
                        self.sync_pane_viewports();
                    }
                    return;
                }
            }

            // Chunk: docs/chunks/tiling_focus_keybindings - Focus switching between panes
            // Cmd+Option+Arrow switches focus to the pane in that direction
            if event.modifiers.option && !event.modifiers.shift {
                use crate::pane_layout::Direction;

                let direction = match event.key {
                    Key::Right => Some(Direction::Right),
                    Key::Left => Some(Direction::Left),
                    Key::Down => Some(Direction::Down),
                    Key::Up => Some(Direction::Up),
                    _ => None,
                };

                if let Some(dir) = direction {
                    if let Some(workspace) = self.editor.active_workspace_mut() {
                        if workspace.switch_focus(dir) {
                            self.invalidation.merge(InvalidationKind::Layout);
                        }
                    }
                    return;
                }
            }
        }

        // Ctrl+Shift+P: dump perf stats on demand (perf-instrumentation feature only)
        #[cfg(feature = "perf-instrumentation")]
        if event.modifiers.control && event.modifiers.shift && !event.modifiers.command {
            if let Key::Char('p') | Key::Char('P') = event.key {
                self.dump_perf_stats = true;
                return;
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
            // Chunk: docs/chunks/dirty_tab_close_confirm - Key handling for confirm dialog
            EditorFocus::ConfirmDialog => {
                self.handle_key_confirm_dialog(event);
            }
        }
    }

    /// Handles Cmd+P to toggle the file picker.
    /// Chunk: docs/chunks/file_picker - Toggle behavior for Cmd+P (open/close file picker)
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
            // Chunk: docs/chunks/dirty_tab_close_confirm - Block file picker during confirm dialog
            EditorFocus::ConfirmDialog => {
                // Don't open file picker while confirm dialog is active
            }
        }
    }

    /// Handles Cmd+O to open a file via the native macOS file picker.
    /// Chunk: docs/chunks/file_open_picker - Open file via system file picker
    fn handle_cmd_o(&mut self) {
        // No-op for terminal tabs (associate_file also guards, but early return is cleaner)
        if !self.active_tab_is_file() {
            return;
        }

        if let Some(path) = file_picker::pick_file() {
            self.associate_file(path);
        }
    }

    /// Opens the file picker selector.
    /// Chunk: docs/chunks/file_picker - FileIndex initialization, initial query, SelectorWidget setup
    // Chunk: docs/chunks/selector_scroll_bottom - Call update_visible_size after set_items
    // Chunk: docs/chunks/workspace_dir_picker - Use workspace's file index
    fn open_file_picker(&mut self) {
        // Get the active workspace's file index
        // Chunk: docs/chunks/workspace_dir_picker - Per-workspace file index
        let workspace = match self.editor.active_workspace() {
            Some(ws) => ws,
            None => return,
        };

        // Query with empty string to get initial results
        let results = workspace.file_index.query("");
        let cache_version = workspace.file_index.cache_version();

        // Create a new selector widget
        let mut selector = SelectorWidget::new();

        // Map results to display strings
        let items: Vec<String> = results
            .iter()
            .map(|r| r.path.display().to_string())
            .collect();
        selector.set_items(items);

        // Calculate overlay geometry to set initial visible_rows (fixes Bug A:
        // without this, visible_item_range() returns 0..1 on first render because
        // RowScroller is initialized with visible_rows = 0).
        // Chunk: docs/chunks/selector_scroll_bottom
        let line_height = self.font_metrics.line_height as f32;
        let geometry = calculate_overlay_geometry(
            self.view_width,
            self.view_height,
            line_height,
            selector.items().len(),
        );
        // Chunk: docs/chunks/selector_scroll_end - Sync RowScroller row_height with geometry
        // The RowScroller is initialized with a default row_height (16.0), but the actual
        // item height comes from font_metrics.line_height. Without this sync, the scroller
        // computes an incorrect visible_rows count, causing the selection to be placed
        // outside the rendered viewport when scrolling to the bottom of a long list.
        selector.set_item_height(geometry.item_height);
        selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);

        // Store the selector and update focus
        self.active_selector = Some(selector);
        self.focus = EditorFocus::Selector;
        // Chunk: docs/chunks/focus_stack - Push selector focus target onto stack
        // This keeps the focus_stack in sync for focus_layer() rendering decisions.
        // We use new_empty() because the actual widget is in self.active_selector.
        // TODO(focus_stack): Full integration would store widget only in focus_stack.
        self.focus_stack.push(Box::new(SelectorFocusTarget::new_empty()));

        // Store cache version in workspace (for streaming refresh)
        // Chunk: docs/chunks/workspace_dir_picker - Per-workspace cache version tracking
        if let Some(ws) = self.editor.active_workspace_mut() {
            ws.last_cache_version = cache_version;
        }

        // Chunk: docs/chunks/cursor_blink_focus - Reset cursor states on focus transition
        // Main buffer cursor stays visible (static) while overlay is active
        self.cursor_visible = true;
        // Overlay cursor starts visible and ready to blink
        self.overlay_cursor_visible = true;
        self.last_overlay_keystroke = Instant::now();

        // Mark full viewport dirty for overlay rendering
        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Closes the active selector.
    /// Chunk: docs/chunks/file_picker - Selector dismissal and focus return to Buffer
    fn close_selector(&mut self) {
        self.active_selector = None;
        self.focus = EditorFocus::Buffer;
        // Chunk: docs/chunks/focus_stack - Pop selector focus target from stack
        self.focus_stack.pop();

        // Chunk: docs/chunks/cursor_blink_focus - Reset cursor states on focus transition
        // Buffer cursor resumes blinking (start visible, record keystroke to prevent immediate blink-off)
        self.cursor_visible = true;
        self.last_keystroke = Instant::now();

        self.invalidation.merge(InvalidationKind::Layout);
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
    // Chunk: docs/chunks/terminal_active_tab_safety - Skip for terminal tabs
    fn handle_cmd_f(&mut self) {
        // Find-in-file only makes sense for file tabs. Terminal tabs use the shell's search.
        if !self.active_tab_is_file() {
            return;
        }

        match self.focus {
            EditorFocus::Buffer => {
                // Record cursor position as search origin
                self.search_origin = self.buffer().cursor_position();

                // Create a new MiniBuffer for the find query
                self.find_mini_buffer = Some(MiniBuffer::new(self.font_metrics));

                // Transition focus
                self.focus = EditorFocus::FindInFile;
                // Chunk: docs/chunks/focus_stack - Push find focus target onto stack
                // Use new_empty() since the actual state is in self.find_mini_buffer.
                // TODO(focus_stack): Full integration would store mini_buffer only in focus_stack.
                self.focus_stack.push(Box::new(FindFocusTarget::new_empty(self.font_metrics)));

                // Chunk: docs/chunks/cursor_blink_focus - Reset cursor states on focus transition
                // Main buffer cursor stays visible (static) while overlay is active
                self.cursor_visible = true;
                // Overlay cursor starts visible and ready to blink
                self.overlay_cursor_visible = true;
                self.last_overlay_keystroke = Instant::now();

                // Mark full viewport dirty for overlay rendering
                self.invalidation.merge(InvalidationKind::Layout);
            }
            EditorFocus::FindInFile => {
                // No-op: Cmd+F while open does nothing
            }
            EditorFocus::Selector => {
                // No-op: don't open find while file picker is open
            }
            // Chunk: docs/chunks/dirty_tab_close_confirm - Block find during confirm dialog
            EditorFocus::ConfirmDialog => {
                // No-op: don't open find while confirm dialog is active
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
        // Chunk: docs/chunks/focus_stack - Pop find focus target from stack
        self.focus_stack.pop();

        // Chunk: docs/chunks/cursor_blink_focus - Reset cursor states on focus transition
        // Buffer cursor resumes blinking (start visible, record keystroke to prevent immediate blink-off)
        self.cursor_visible = true;
        self.last_keystroke = Instant::now();

        self.invalidation.merge(InvalidationKind::Layout);
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

        // Chunk: docs/chunks/cursor_blink_focus - Record overlay keystroke time for blink reset
        self.last_overlay_keystroke = Instant::now();

        // Ensure overlay cursor is visible when typing
        if !self.overlay_cursor_visible {
            self.overlay_cursor_visible = true;
        }

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
                    self.invalidation.merge(InvalidationKind::Layout);
                }
            }
        }
    }

    /// Runs the live search and updates the buffer selection.
    ///
    /// Called after every key event that changes the minibuffer's content.
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    fn run_live_search(&mut self) {
        // Early return if not a file tab (should not happen since find mode
        // is guarded, but defensive)
        if !self.active_tab_is_file() {
            return;
        }

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

                // Scroll viewport to make match visible.
                // Chunk: docs/chunks/find_strip_scroll_clearance - Use margin when find strip is active
                // Use margin=1 because the find strip occludes the last visible row.
                // This ensures matches land at visible_lines - 2 (one row above the strip).
                let line_count = self.buffer().line_count();
                let match_line = start.line;
                if self.viewport_mut().ensure_visible_with_margin(match_line, line_count, 1) {
                    self.invalidation.merge(InvalidationKind::Layout);
                }
            }
            None => {
                // No match: clear the selection
                self.buffer_mut().clear_selection();
            }
        }

        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Advances the search to the next match (Enter in find mode).
    ///
    /// Moves search_origin past the end of the current match and re-runs search.
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    fn advance_to_next_match(&mut self) {
        // Early return if not a file tab
        if !self.active_tab_is_file() {
            return;
        }

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

    // =========================================================================
    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog key handling
    // =========================================================================

    /// Handles a key event when the confirm dialog is focused.
    ///
    /// Delegates to `ConfirmDialog::handle_key()` and processes the outcome:
    /// - `Cancelled`: Close the dialog, keep the tab open
    /// - `Confirmed`: Dispatch to the appropriate handler based on context
    /// - `Pending`: Just mark dirty for visual update
    // Chunk: docs/chunks/generic_yes_no_modal - Context-based outcome routing
    fn handle_key_confirm_dialog(&mut self, event: KeyEvent) {
        use crate::confirm_dialog::ConfirmOutcome;

        let dialog = match self.confirm_dialog.as_mut() {
            Some(d) => d,
            None => return,
        };

        let outcome = dialog.handle_key(&event);

        match outcome {
            ConfirmOutcome::Cancelled => {
                // User chose Cancel or pressed Escape - handle based on context
                self.handle_confirm_dialog_cancelled();
            }
            ConfirmOutcome::Confirmed => {
                // User confirmed - handle based on context
                self.handle_confirm_dialog_confirmed();
            }
            ConfirmOutcome::Pending => {
                // Dialog still open - just mark dirty for visual update
                self.invalidation.merge(InvalidationKind::Layout);
            }
        }
    }

    /// Handles the confirmed outcome of the confirm dialog.
    ///
    /// Dispatches to the appropriate handler based on the `confirm_context`:
    /// - `CloseDirtyTab`: Force-close the tab without saving
    /// - `QuitWithDirtyTabs`: Set the quit flag
    /// - `CloseActiveTerminal`: Kill the process and close the terminal tab
    /// - `FileDeletedFromDisk`: Save the buffer to recreate the file
    // Chunk: docs/chunks/generic_yes_no_modal - Context-based outcome routing
    // Chunk: docs/chunks/deletion_rename_handling - FileDeletedFromDisk handling
    fn handle_confirm_dialog_confirmed(&mut self) {
        if let Some(ctx) = self.confirm_context.take() {
            match ctx {
                ConfirmDialogContext::CloseDirtyTab { pane_id, tab_idx } => {
                    self.force_close_tab(pane_id, tab_idx);
                }
                ConfirmDialogContext::QuitWithDirtyTabs { .. } => {
                    // Set the quit flag - the main loop will handle termination
                    self.should_quit = true;
                }
                // Chunk: docs/chunks/terminal_close_guard - Kill process and close terminal
                ConfirmDialogContext::CloseActiveTerminal { pane_id, tab_idx } => {
                    self.kill_terminal_and_close_tab(pane_id, tab_idx);
                }
                // Chunk: docs/chunks/deletion_rename_handling - Save to recreate deleted file
                ConfirmDialogContext::FileDeletedFromDisk { pane_id: _, tab_idx: _, deleted_path } => {
                    // User chose "Save" - recreate the file from buffer contents
                    self.save_buffer_to_path(&deleted_path);
                }
            }
        }
        self.close_confirm_dialog();
    }

    // Chunk: docs/chunks/deletion_rename_handling - Context-aware cancelled handling
    /// Handles the cancelled outcome of the confirm dialog.
    ///
    /// For most dialogs, cancelling just closes the dialog. For `FileDeletedFromDisk`,
    /// cancelling means "Abandon" which closes the tab (since the file no longer exists).
    fn handle_confirm_dialog_cancelled(&mut self) {
        // Take context to examine it (we'll need to close the dialog afterward)
        if let Some(ctx) = self.confirm_context.take() {
            match ctx {
                // Chunk: docs/chunks/deletion_rename_handling - Abandon closes the tab
                ConfirmDialogContext::FileDeletedFromDisk { pane_id, tab_idx, .. } => {
                    // "Abandon" was selected - close the tab
                    self.force_close_tab(pane_id, tab_idx);
                }
                // For all other contexts, cancelling just closes the dialog
                _ => {}
            }
        }
        self.close_confirm_dialog();
    }

    /// Closes the confirm dialog and returns focus to the buffer.
    // Chunk: docs/chunks/generic_yes_no_modal - Use confirm_context instead of pending_close
    fn close_confirm_dialog(&mut self) {
        self.confirm_dialog = None;
        self.confirm_context = None;
        self.focus = EditorFocus::Buffer;
        // Chunk: docs/chunks/focus_stack - Pop confirm dialog focus target from stack
        self.focus_stack.pop();
        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Shows a confirmation dialog for closing a dirty tab.
    ///
    /// This stores the context so we can close the correct tab
    /// if the user confirms, then transitions focus to the dialog.
    // Chunk: docs/chunks/generic_yes_no_modal - Use ConfirmDialogContext
    fn show_confirm_dialog(&mut self, pane_id: PaneId, tab_idx: usize) {
        let dialog = ConfirmDialog::new("Abandon unsaved changes?");
        self.confirm_dialog = Some(dialog.clone());
        self.confirm_context = Some(ConfirmDialogContext::CloseDirtyTab { pane_id, tab_idx });
        self.focus = EditorFocus::ConfirmDialog;
        // Chunk: docs/chunks/focus_stack - Push confirm dialog focus target onto stack
        self.focus_stack.push(Box::new(ConfirmDialogFocusTarget::new(dialog)));
        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Shows a confirmation dialog for closing a terminal with an active process.
    ///
    /// Uses terminal-specific wording ("Kill running process?") and the
    /// `CloseActiveTerminal` context variant.
    // Chunk: docs/chunks/terminal_close_guard - Terminal close confirmation
    fn show_terminal_close_confirm(&mut self, pane_id: PaneId, tab_idx: usize) {
        let dialog = ConfirmDialog::with_labels(
            "Kill running process?",
            "Cancel",
            "Kill",
        );
        self.confirm_dialog = Some(dialog.clone());
        self.confirm_context = Some(ConfirmDialogContext::CloseActiveTerminal { pane_id, tab_idx });
        self.focus = EditorFocus::ConfirmDialog;
        // Chunk: docs/chunks/focus_stack - Push confirm dialog focus target onto stack
        self.focus_stack.push(Box::new(ConfirmDialogFocusTarget::new(dialog)));
        self.invalidation.merge(InvalidationKind::Layout);
    }

    // Chunk: docs/chunks/deletion_rename_handling - File deleted event handler
    /// Handles external file deletion events.
    ///
    /// Finds any open tabs associated with the deleted file and shows a confirm
    /// dialog asking the user whether to "Save" (recreate the file from the
    /// buffer's contents) or "Abandon" (close the tab).
    ///
    /// The dialog uses the `FileDeletedFromDisk` context variant.
    pub fn handle_file_deleted(&mut self, path: std::path::PathBuf) {
        // Find if any tab in the active workspace has this file open
        if let Some(workspace) = self.editor.active_workspace() {
            let pane_id = workspace.active_pane_id;
            for (tab_idx, tab) in workspace.tabs().iter().enumerate() {
                if let Some(ref associated) = tab.associated_file {
                    if associated == &path {
                        // Found a tab with this file - show confirm dialog
                        self.show_file_deleted_confirm(pane_id, tab_idx, path);
                        return;
                    }
                }
            }
        }
        // No tab found for this file - ignore the event
    }

    /// Shows a confirmation dialog for a deleted file.
    ///
    /// Uses file-deleted-specific wording ("File deleted from disk") and offers
    /// "Save" (recreate) as the confirm action and "Abandon" as the cancel action.
    fn show_file_deleted_confirm(&mut self, pane_id: PaneId, tab_idx: usize, deleted_path: std::path::PathBuf) {
        let dialog = ConfirmDialog::with_labels(
            "File deleted from disk",
            "Abandon",
            "Save",
        );
        self.confirm_dialog = Some(dialog.clone());
        self.confirm_context = Some(ConfirmDialogContext::FileDeletedFromDisk {
            pane_id,
            tab_idx,
            deleted_path,
        });
        self.focus = EditorFocus::ConfirmDialog;
        // Chunk: docs/chunks/focus_stack - Push confirm dialog focus target onto stack
        self.focus_stack.push(Box::new(ConfirmDialogFocusTarget::new(dialog)));
        self.invalidation.merge(InvalidationKind::Layout);
    }

    // Chunk: docs/chunks/deletion_rename_handling - File renamed event handler
    /// Handles external file rename events.
    ///
    /// Updates the `associated_file` of any matching tab to the new path and
    /// updates the tab label to reflect the new filename. If the file extension
    /// changed, re-evaluates syntax highlighting for the new file type.
    /// This is a silent operation - no dialog is shown.
    pub fn handle_file_renamed(&mut self, from: std::path::PathBuf, to: std::path::PathBuf) {
        // Check if extension changed for syntax highlighting re-evaluation
        let extension_changed = from.extension() != to.extension();

        if let Some(workspace) = self.editor.active_workspace_mut() {
            // Check all panes for tabs with this file
            for pane in workspace.all_panes_mut() {
                for tab in &mut pane.tabs {
                    if let Some(ref associated) = tab.associated_file {
                        if associated == &from {
                            // Update the associated file path
                            tab.associated_file = Some(to.clone());

                            // Update the tab label to the new filename
                            if let Some(new_name) = to.file_name() {
                                tab.label = new_name.to_string_lossy().to_string();
                            }

                            // Re-evaluate syntax highlighting if extension changed
                            if extension_changed {
                                let theme = SyntaxTheme::catppuccin_mocha();
                                tab.setup_highlighting(&self.language_registry, theme);
                            }

                            // Mark dirty to refresh the UI
                            self.invalidation.merge(InvalidationKind::Layout);
                            return;
                        }
                    }
                }
            }
        }
        // No tab found for this file - ignore the event
    }

    /// Checks if the tab at `index` in `pane_id` is a terminal with an active process.
    ///
    /// Returns `true` if the tab is a terminal and `try_wait()` returns `None` (process running).
    /// Returns `false` for file tabs, exited terminals, or tabs without a PTY.
    ///
    /// Note: This requires mutable access because `try_wait()` may reap a zombie process
    /// (standard POSIX behavior).
    // Chunk: docs/chunks/terminal_close_guard - Process liveness detection
    fn is_terminal_with_active_process(&mut self, pane_id: PaneId, index: usize) -> bool {
        use crate::workspace::TabKind;

        if let Some(workspace) = self.editor.active_workspace_mut() {
            if let Some(pane) = workspace.pane_root.get_pane_mut(pane_id) {
                if let Some(tab) = pane.tabs.get_mut(index) {
                    // Only check terminal tabs
                    if tab.kind != TabKind::Terminal {
                        return false;
                    }
                    // Check if process is still running
                    if let Some(term) = tab.as_terminal_buffer_mut() {
                        // try_wait returns None if process is still running
                        return term.try_wait().is_none();
                    }
                }
            }
        }
        false
    }

    /// Kills the terminal process and closes the tab.
    ///
    /// This is called after the user confirms closing a terminal with an active process.
    // Chunk: docs/chunks/terminal_close_guard - Terminal process termination
    fn kill_terminal_and_close_tab(&mut self, pane_id: PaneId, tab_idx: usize) {
        // Kill the process first
        if let Some(workspace) = self.editor.active_workspace_mut() {
            if let Some(pane) = workspace.pane_root.get_pane_mut(pane_id) {
                if let Some(tab) = pane.tabs.get_mut(tab_idx) {
                    if let Some(term) = tab.as_terminal_buffer_mut() {
                        let _ = term.kill(); // Ignore errors - we're closing anyway
                    }
                }
            }
        }
        // Then close the tab using existing force_close logic
        self.force_close_tab(pane_id, tab_idx);
    }

    /// Closes a tab without checking the dirty flag.
    ///
    /// This is used after the user confirms abandoning unsaved changes.
    /// The `_pane_id` parameter is currently unused because we always operate
    /// on the active pane, but it's kept for future multi-pane confirmation dialogs.
    fn force_close_tab(&mut self, _pane_id: PaneId, tab_idx: usize) {
        // Pre-compute values needed for fallback before borrowing workspace mutably
        let tab_id = self.editor.gen_tab_id();
        let line_height = self.editor.line_height();

        if let Some(workspace) = self.editor.active_workspace_mut() {
            let pane_count = workspace.pane_root.pane_count();

            if pane_count > 1 {
                // Multi-pane layout: check if pane will become empty
                let pane_will_be_empty = workspace.active_pane()
                    .map(|p| p.tabs.len() == 1)
                    .unwrap_or(false);

                // Find fallback focus BEFORE mutating (to avoid borrow conflicts)
                let fallback_focus = if pane_will_be_empty {
                    workspace.find_fallback_focus()
                } else {
                    None
                };

                // Close the tab
                if let Some(pane) = workspace.active_pane_mut() {
                    pane.close_tab(tab_idx);
                }

                // If pane is now empty, cleanup the tree and update focus
                if pane_will_be_empty {
                    if let Some(fallback_pane_id) = fallback_focus {
                        // Update focus BEFORE cleanup (cleanup removes the empty pane)
                        workspace.active_pane_id = fallback_pane_id;
                    }
                    // Cleanup empty panes (collapses the tree)
                    crate::pane_layout::cleanup_empty_panes(&mut workspace.pane_root);
                }
            } else {
                // Single pane layout
                if let Some(pane) = workspace.active_pane_mut() {
                    if pane.tabs.len() > 1 {
                        // Multiple tabs: just close the tab
                        pane.close_tab(tab_idx);
                    } else {
                        // Single tab in single pane: replace with empty tab
                        let new_tab = crate::workspace::Tab::empty_file(tab_id, line_height);
                        pane.tabs[0] = new_tab;
                        pane.active_tab = 0;
                    }
                }
            }
        }

        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Handles a key event when the selector is focused.
    /// Chunk: docs/chunks/file_picker - Key forwarding to SelectorWidget and SelectorOutcome handling
    fn handle_key_selector(&mut self, event: KeyEvent) {
        // Chunk: docs/chunks/cursor_blink_focus - Record overlay keystroke time for blink reset
        self.last_overlay_keystroke = Instant::now();

        // Ensure overlay cursor is visible when typing
        if !self.overlay_cursor_visible {
            self.overlay_cursor_visible = true;
        }

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

        // Chunk: docs/chunks/selector_scroll_end - Sync RowScroller row_height with geometry
        selector.set_item_height(geometry.item_height);
        // Update visible size on the selector (for arrow key navigation scroll)
        selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);

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
                    // Chunk: docs/chunks/workspace_dir_picker - Use workspace's file index
                    if let Some(workspace) = self.editor.active_workspace() {
                        let results = workspace.file_index.query(&current_query);
                        let cache_version = workspace.file_index.cache_version();
                        let items: Vec<String> = results
                            .iter()
                            .map(|r| r.path.display().to_string())
                            .collect();
                        // Need to reborrow selector mutably
                        if let Some(ref mut sel) = self.active_selector {
                            sel.set_items(items);
                            // Fix Bug B: Recalculate visible_rows after set_items.
                            // The update_visible_size at the start of the handler used
                            // the old item count. With a new item list (potentially
                            // different size), max_visible_items may change, so we need
                            // to update visible_rows to match the new geometry.
                            // Chunk: docs/chunks/selector_scroll_bottom
                            let new_geometry = calculate_overlay_geometry(
                                self.view_width,
                                self.view_height,
                                line_height,
                                sel.items().len(),
                            );
                            // Chunk: docs/chunks/selector_scroll_end - Sync row_height
                            sel.set_item_height(new_geometry.item_height);
                            sel.update_visible_size(
                                new_geometry.visible_items as f32 * new_geometry.item_height,
                            );
                        }
                        // Update workspace's cache version
                        if let Some(ws) = self.editor.active_workspace_mut() {
                            ws.last_cache_version = cache_version;
                        }
                    }
                }
                // Mark dirty for any visual update (selection, query, etc.)
                self.invalidation.merge(InvalidationKind::Layout);
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
    /// Chunk: docs/chunks/file_picker - Path resolution, recency recording, and resolved_path storage on Enter
    // Chunk: docs/chunks/file_save - Integrates file picker confirmation with associate_file
    // Chunk: docs/chunks/workspace_dir_picker - Use workspace's file index and root_path
    fn handle_selector_confirm(&mut self, idx: usize) {
        // Get the workspace root_path as the base directory for path resolution
        let base_dir = self.editor.active_workspace()
            .map(|ws| ws.root_path.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        // Get items and query from selector
        let (items, query) = if let Some(ref selector) = self.active_selector {
            (selector.items().to_vec(), selector.query())
        } else {
            return;
        };

        // Resolve the path
        let resolved = self.resolve_picker_path(idx, &items, &query, &base_dir);

        // Record the selection for recency in the workspace's file index
        // Chunk: docs/chunks/workspace_dir_picker - Per-workspace recency tracking
        if let Some(ws) = self.editor.active_workspace() {
            ws.file_index.record_selection(&resolved);
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
    /// Chunk: docs/chunks/file_picker - Path resolution logic (existing file vs new file creation)
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
    // Chunk: docs/chunks/terminal_active_tab_safety - Route terminal tabs to InputEncoder
    fn handle_key_buffer(&mut self, event: KeyEvent) {
        // Record keystroke time for cursor blink reset
        self.last_keystroke = Instant::now();

        // Chunk: docs/chunks/syntax_highlighting - Track whether we need to sync highlighter
        let needs_highlighter_sync;
        // Chunk: docs/chunks/unsaved_tab_tint - Track whether we processed a file tab
        let mut is_file_tab = false;
        // Chunk: docs/chunks/dirty_bit_navigation - Track whether content was mutated
        let mut content_mutated = false;

        // Check if the active tab is a file tab or terminal tab
        // Use a block to limit the borrow scope
        {
            let ws = self.editor.active_workspace_mut().expect("no active workspace");
            let tab = ws.active_tab_mut().expect("no active tab");

            // Check for highlighter before getting mutable borrow
            needs_highlighter_sync = tab.highlighter().is_some();

            // Try to get the text buffer and viewport for file tabs
            if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            // File tab: use the existing BufferFocusTarget path
            // Chunk: docs/chunks/unsaved_tab_tint - Mark this as a file tab for dirty tracking
            is_file_tab = true;

            // Ensure cursor blink visibility is on when typing
            if !self.cursor_visible {
                self.cursor_visible = true;
                // Mark cursor line dirty to show it
                let cursor_line = buffer.cursor_position().line;
                let dirty = viewport.dirty_lines_to_region(
                    &lite_edit_buffer::DirtyLines::Single(cursor_line),
                    buffer.line_count(),
                );
                // Chunk: docs/chunks/invalidation_separation - Content invalidation for cursor
                self.invalidation.merge(InvalidationKind::Content(dirty));
            }

            // Chunk: docs/chunks/viewport_scrolling - Snap-back viewport when cursor off-screen
            // If the cursor is off-screen (scrolled away), snap the viewport back
            // to make the cursor visible BEFORE processing the keystroke.
            // This ensures typing after scrolling doesn't edit at a position
            // the user can't see.
            let cursor_line = buffer.cursor_position().line;
            if viewport.buffer_line_to_screen_line(cursor_line).is_none() {
                // Cursor is off-screen - scroll to make it visible
                let line_count = buffer.line_count();
                if viewport.ensure_visible(cursor_line, line_count) {
                    // Viewport scrolled - mark full viewport dirty
                    self.invalidation.merge(InvalidationKind::Layout);
                }
            }

            // Create context and forward to focus target
            let font_metrics = self.font_metrics;
            // Chunk: docs/chunks/content_tab_bar - Use content area dimensions
            // Adjust dimensions to account for left rail and tab bar
            let content_height = self.view_height - TAB_BAR_HEIGHT;
            let content_width = self.view_width - RAIL_WIDTH;

            // Chunk: docs/chunks/invalidation_separation - Use temporary DirtyRegion for EditorContext
            // EditorContext accumulates buffer-level dirty regions. We convert to
            // InvalidationKind::Content after handling.
            let mut ctx_dirty_region = DirtyRegion::None;

            // Chunk: docs/chunks/styled_line_cache - Pass dirty_lines for cache invalidation
            let mut ctx = EditorContext::new(
                buffer,
                viewport,
                &mut ctx_dirty_region,
                &mut self.dirty_lines,
                font_metrics,
                content_height,
                content_width,
            );
            self.focus_target.handle_key(event, &mut ctx);
            // Chunk: docs/chunks/dirty_bit_navigation - Capture content_mutated before ctx goes out of scope
            content_mutated = ctx.content_mutated;

            // Chunk: docs/chunks/invalidation_separation - Convert to Content invalidation
            if ctx_dirty_region.is_dirty() {
                self.invalidation.merge(InvalidationKind::Content(ctx_dirty_region));
            }
        } else if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
            // Chunk: docs/chunks/terminal_clipboard_selection - Terminal clipboard operations
            // Check for Cmd+C (copy) and Cmd+V (paste) first
            use crate::input::Key;

            if event.modifiers.command && !event.modifiers.control {
                match event.key {
                    Key::Char('c') | Key::Char('C') => {
                        // Cmd+C: copy selected text to clipboard
                        if let Some(text) = terminal.selected_text() {
                            crate::clipboard::copy_to_clipboard(&text);
                            terminal.clear_selection();
                        }
                        // No-op if no selection (don't send interrupt)
                        self.invalidation.merge(InvalidationKind::Layout);
                        return;
                    }
                    Key::Char('v') | Key::Char('V') => {
                        // Cmd+V: paste from clipboard
                        // Chunk: docs/chunks/terminal_paste_render - Don't mark dirty before PTY echo
                        if let Some(text) = crate::clipboard::paste_from_clipboard() {
                            // Use bracketed paste encoding
                            let modes = terminal.term_mode();
                            let bytes = InputEncoder::encode_paste(&text, modes);
                            if !bytes.is_empty() {
                                let _ = terminal.write_input(&bytes);
                            }
                        }
                        // No dirty marking here - let poll_agents() detect the PTY echo
                        // and update_damage() mark the correct lines dirty.
                        return;
                    }
                    _ => {}
                }
            }

            // Chunk: docs/chunks/terminal_scrollback_viewport - Snap to bottom on keypress
            // Terminal tab: encode key and send to PTY
            // First, snap to bottom if scrolled up in primary screen mode
            if !terminal.is_alt_screen() {
                let line_count = terminal.line_count();
                if !viewport.is_at_bottom(line_count) {
                    viewport.scroll_to_bottom(line_count);
                }
            }

            let modes = terminal.term_mode();
            let bytes = InputEncoder::encode_key(&event, modes);

            if !bytes.is_empty() {
                // Write to the terminal's PTY (ignore errors for now)
                let _ = terminal.write_input(&bytes);
            }

            // Mark full viewport dirty since terminal output may change
            self.invalidation.merge(InvalidationKind::Layout);
        }
        // Other tab types (AgentOutput, Diff): no-op
        } // End of borrow scope

        // Chunk: docs/chunks/syntax_highlighting - Sync highlighter after buffer mutation
        if needs_highlighter_sync {
            self.sync_active_tab_highlighter();
        }

        // Chunk: docs/chunks/dirty_bit_navigation - Mark file tab dirty only for content mutations
        // The EditorContext tracks whether a content-mutating command was executed.
        // This correctly distinguishes mutations (insert, delete, paste, cut) from
        // non-mutating operations (cursor movement, selection, scrolling) that also
        // set dirty_region for rendering purposes.
        if is_file_tab && content_mutated {
            if let Some(ws) = self.editor.active_workspace_mut() {
                if let Some(tab) = ws.active_tab_mut() {
                    tab.dirty = true;
                }
            }
        }
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
    // Chunk: docs/chunks/mouse_click_cursor - Mouse event routing from controller to focus target via EditorContext
    /// Chunk: docs/chunks/file_picker - Focus-based mouse routing (selector vs buffer)
    // Chunk: docs/chunks/tiling_workspace_integration - Coordinate handling: flip y once at entry
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        use crate::input::MouseEventKind;

        // Step 1: Flip y-coordinate ONCE at entry
        // NSView uses bottom-left origin (y=0 at bottom)
        // We convert to screen space (y=0 at top) for all downstream code
        let (nsview_x, nsview_y) = event.position;
        let screen_x = nsview_x;
        let screen_y = (self.view_height as f64) - nsview_y;

        // Create screen-space event for downstream handlers
        let screen_event = MouseEvent {
            kind: event.kind,
            position: (screen_x, screen_y),
            modifiers: event.modifiers,
            click_count: event.click_count,
        };

        // Step 2: Hit-test against UI regions in screen space (y=0 at top)

        // Check if click is in left rail region (x < RAIL_WIDTH)
        if screen_x < RAIL_WIDTH as f64 {
            if let MouseEventKind::Down = screen_event.kind {
                // Calculate which workspace was clicked
                let geometry = calculate_left_rail_geometry(self.view_height, self.editor.workspace_count());
                // geometry.tile_rects are already in screen space (y=0 at top)
                for (idx, tile_rect) in geometry.tile_rects.iter().enumerate() {
                    if tile_rect.contains(screen_x as f32, screen_y as f32) {
                        self.switch_workspace(idx);
                        return;
                    }
                }
            }
            // Don't forward rail clicks to buffer
            return;
        }

        // Chunk: docs/chunks/pane_cursor_click_offset - Unified pane hit resolution
        // In multi-pane layouts, each pane has its own tab bar at its top edge.
        // We use resolve_pane_hit to consistently detect tab bar clicks.
        {
            use crate::pane_layout::{resolve_pane_hit, HitZone};

            let is_tab_bar_click = if let Some(workspace) = self.editor.active_workspace() {
                // Renderer-consistent bounds
                let bounds = (
                    RAIL_WIDTH,
                    0.0,
                    self.view_width - RAIL_WIDTH,
                    self.view_height,
                );

                if let Some(hit) = resolve_pane_hit(
                    screen_x as f32,
                    screen_y as f32,
                    bounds,
                    &workspace.pane_root,
                    TAB_BAR_HEIGHT,
                ) {
                    hit.zone == HitZone::TabBar
                } else {
                    false
                }
            } else {
                false
            };

            if is_tab_bar_click {
                if let MouseEventKind::Down = screen_event.kind {
                    self.handle_tab_bar_click(screen_x as f32, screen_y as f32);
                }
                // Don't forward tab bar clicks to buffer
                return;
            }
        }

        // Step 3: Route to appropriate handler with screen-space coordinates
        match self.focus {
            EditorFocus::Selector => {
                self.handle_mouse_selector(screen_event);
            }
            EditorFocus::Buffer | EditorFocus::FindInFile => {
                // In FindInFile mode, mouse events still go to the buffer
                // so the user can scroll/click while searching
                self.handle_mouse_buffer(screen_event);
            }
            // Chunk: docs/chunks/dirty_tab_close_confirm - Block mouse during confirm dialog
            // Chunk: docs/chunks/generic_yes_no_modal - Add mouse click support for confirm dialog
            EditorFocus::ConfirmDialog => {
                if let MouseEventKind::Down = screen_event.kind {
                    self.handle_mouse_confirm_dialog(screen_x as f32, screen_y as f32);
                }
            }
        }
    }

    /// Handles a mouse click on the confirm dialog.
    ///
    /// Hit-tests the cancel and confirm buttons and dispatches accordingly:
    /// - Click on cancel button: closes the dialog
    /// - Click on confirm button: handles confirmation based on context
    /// - Click elsewhere: no-op (dialog stays open)
    // Chunk: docs/chunks/generic_yes_no_modal - Mouse click handling for confirm dialog
    fn handle_mouse_confirm_dialog(&mut self, x: f32, y: f32) {
        let dialog = match self.confirm_dialog.as_ref() {
            Some(d) => d,
            None => return,
        };

        // Calculate geometry to get button positions
        let line_height = self.font_metrics.line_height as f32;
        let glyph_width = self.font_metrics.advance_width as f32;
        let geometry = calculate_confirm_dialog_geometry(
            self.view_width,
            self.view_height,
            line_height,
            glyph_width,
            dialog,
        );

        // Hit test the buttons
        if geometry.is_cancel_button(x, y) {
            // Update selection for visual feedback before closing
            if let Some(d) = self.confirm_dialog.as_mut() {
                d.selected = crate::confirm_dialog::ConfirmButton::Cancel;
            }
            self.close_confirm_dialog();
        } else if geometry.is_confirm_button(x, y) {
            // Update selection for visual feedback before handling
            if let Some(d) = self.confirm_dialog.as_mut() {
                d.selected = crate::confirm_dialog::ConfirmButton::Abandon;
            }
            self.handle_confirm_dialog_confirmed();
        }
        // Clicks outside buttons are ignored - dialog stays open
    }

    /// Handles a mouse event when the selector is focused.
    /// Chunk: docs/chunks/file_picker - Mouse forwarding to SelectorWidget with overlay geometry
    // Chunk: docs/chunks/tiling_workspace_integration - Receives screen-space coordinates (y=0 at top)
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

        // Chunk: docs/chunks/selector_scroll_end - Sync RowScroller row_height with geometry
        selector.set_item_height(geometry.item_height);
        // Update visible size on the selector (for consistency with scroll/key handling)
        selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);

        // event.position is already in screen space (y=0 at top), no flip needed
        // Overlay geometry also uses screen space (y=0 at top)
        let outcome = selector.handle_mouse(
            event.position,
            event.kind,
            geometry.item_height as f64,
            geometry.list_origin_y as f64,
        );

        match outcome {
            SelectorOutcome::Pending => {
                // Mark dirty for visual update
                self.invalidation.merge(InvalidationKind::Layout);
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
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    // Chunk: docs/chunks/tiling_workspace_integration - Receives screen-space coordinates (y=0 at top)
    // Chunk: docs/chunks/tiling_focus_keybindings - Click-to-focus pane switching
    // Chunk: docs/chunks/pane_cursor_click_offset - Fixed coordinate transformation for non-primary panes
    fn handle_mouse_buffer(&mut self, event: MouseEvent) {
        use crate::input::MouseEventKind;
        use crate::pane_layout::{resolve_pane_hit, HitZone};

        // Record event time for cursor blink reset (same as keystroke)
        self.last_keystroke = Instant::now();

        // event.position is in screen space (y=0 at top of window)
        let (screen_x, screen_y) = event.position;

        // Chunk: docs/chunks/pane_cursor_click_offset - Unified pane hit resolution
        // Use renderer-consistent bounds for pane layout
        let bounds = (
            RAIL_WIDTH,
            0.0,
            self.view_width - RAIL_WIDTH,
            self.view_height,
        );

        // Resolve which pane was hit and get pane-local coordinates
        let hit = if let Some(workspace) = self.editor.active_workspace() {
            resolve_pane_hit(
                screen_x as f32,
                screen_y as f32,
                bounds,
                &workspace.pane_root,
                TAB_BAR_HEIGHT,
            )
        } else {
            None
        };

        // Chunk: docs/chunks/tiling_focus_keybindings - Click-to-focus pane switching
        // Check which pane was clicked and update focus if different (on MouseDown in Content zone)
        if let MouseEventKind::Down = event.kind {
            if let Some(ref hit) = hit {
                if hit.zone == HitZone::Content {
                    if let Some(ws) = self.editor.active_workspace_mut() {
                        if hit.pane_id != ws.active_pane_id {
                            ws.active_pane_id = hit.pane_id;
                            self.invalidation.merge(InvalidationKind::Layout);
                        }
                    }
                }
            }
        }

        // Now get the (potentially updated) active tab
        let ws = self.editor.active_workspace_mut().expect("no active workspace");
        let tab = ws.active_tab_mut().expect("no active tab");

        // Chunk: docs/chunks/pane_cursor_click_offset - Use pane-local coordinates from hit resolution
        // These coordinates are already relative to the pane's content origin (after tab bar)
        let (content_x, content_y) = if let Some(ref hit) = hit {
            (hit.local_x as f64, hit.local_y as f64)
        } else {
            // Fallback for clicks outside panes (shouldn't happen in normal use)
            let fallback_x = (screen_x - RAIL_WIDTH as f64).max(0.0);
            let fallback_y = (screen_y - TAB_BAR_HEIGHT as f64).max(0.0);
            (fallback_x, fallback_y)
        };

        // Try to get the text buffer and viewport for file tabs
        if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            // File tab: use the existing BufferFocusTarget path

            // Ensure cursor is visible when clicking
            if !self.cursor_visible {
                self.cursor_visible = true;
                // Mark cursor line dirty to show it
                let cursor_line = buffer.cursor_position().line;
                let dirty = viewport.dirty_lines_to_region(
                    &lite_edit_buffer::DirtyLines::Single(cursor_line),
                    buffer.line_count(),
                );
                // Chunk: docs/chunks/invalidation_separation - Content invalidation for cursor
                self.invalidation.merge(InvalidationKind::Content(dirty));
            }

            // Create event with pane-local content coordinates
            // content_x and content_y are already relative to the pane's content origin
            let content_event = MouseEvent {
                kind: event.kind,
                position: (content_x, content_y),
                modifiers: event.modifiers,
                click_count: event.click_count,
            };

            // Chunk: docs/chunks/pane_cursor_click_offset - Use pane dimensions for EditorContext
            // When we have a hit result, use the pane's content dimensions for accuracy
            let (pane_content_height, pane_content_width) = if let Some(ref hit) = hit {
                let pane_rect = &hit.pane_rect;
                (
                    pane_rect.height - TAB_BAR_HEIGHT,
                    pane_rect.width,
                )
            } else {
                // Fallback to global content area dimensions
                (
                    self.view_height - TAB_BAR_HEIGHT,
                    self.view_width - RAIL_WIDTH,
                )
            };

            // Create context and forward to focus target
            let font_metrics = self.font_metrics;

            // Chunk: docs/chunks/invalidation_separation - Use temporary DirtyRegion for EditorContext
            let mut ctx_dirty_region = DirtyRegion::None;

            // Chunk: docs/chunks/styled_line_cache - Pass dirty_lines for cache invalidation
            let mut ctx = EditorContext::new(
                buffer,
                viewport,
                &mut ctx_dirty_region,
                &mut self.dirty_lines,
                font_metrics,
                pane_content_height,
                pane_content_width,
            );
            self.focus_target.handle_mouse(content_event, &mut ctx);

            // Chunk: docs/chunks/invalidation_separation - Convert to Content invalidation
            if ctx_dirty_region.is_dirty() {
                self.invalidation.merge(InvalidationKind::Content(ctx_dirty_region));
            }
        } else if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
            // Chunk: docs/chunks/terminal_mouse_offset - Fixed terminal mouse Y coordinate calculation
            // Chunk: docs/chunks/terminal_clipboard_selection - Terminal mouse selection
            // Terminal tab: handle mouse events for selection or forward to PTY
            let modes = terminal.term_mode();

            // Calculate cell position from pixel coordinates
            // content_x and content_y are already in content-local space (y=0 at top of content)
            let cell_width = self.font_metrics.advance_width;
            let cell_height = self.font_metrics.line_height as f32;

            // Account for scroll_fraction_px
            // The renderer translates content by -scroll_fraction_px, so we add it back
            let scroll_fraction_px = viewport.scroll_fraction_px() as f64;
            let adjusted_y = (content_y + scroll_fraction_px).max(0.0);

            let col = (content_x / cell_width as f64) as usize;
            let row = (adjusted_y / cell_height as f64) as usize;

            // Check if any mouse mode is active - forward to PTY
            if modes.intersects(TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) {
                let bytes = InputEncoder::encode_mouse(&event, col, row, modes);
                if !bytes.is_empty() {
                    let _ = terminal.write_input(&bytes);
                }
            } else {
                // No mouse mode active - handle selection
                // Convert screen row to document line (accounting for viewport scroll)
                let doc_line = viewport.first_visible_line() + row;
                let pos = Position::new(doc_line, col);

                match event.kind {
                    MouseEventKind::Down => {
                        if event.click_count >= 2 {
                            // Double-click: select word at position
                            // Chunk: docs/chunks/terminal_clipboard_selection - Word selection
                            if let Some(styled_line) = terminal.styled_line(pos.line) {
                                let line_text: String = styled_line.spans.iter()
                                    .map(|span| span.text.as_str())
                                    .collect();
                                let chars: Vec<char> = line_text.chars().collect();
                                if !chars.is_empty() && pos.col < chars.len() {
                                    let click_char = chars[pos.col];
                                    let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
                                    let (start, end) = if is_word_char(click_char) {
                                        let mut s = pos.col;
                                        while s > 0 && is_word_char(chars[s - 1]) { s -= 1; }
                                        let mut e = pos.col;
                                        while e < chars.len() && is_word_char(chars[e]) { e += 1; }
                                        (s, e)
                                    } else if click_char.is_whitespace() {
                                        let mut s = pos.col;
                                        while s > 0 && chars[s - 1].is_whitespace() { s -= 1; }
                                        let mut e = pos.col;
                                        while e < chars.len() && chars[e].is_whitespace() { e += 1; }
                                        (s, e)
                                    } else {
                                        (pos.col, pos.col + 1)
                                    };
                                    terminal.set_selection_anchor(Position::new(pos.line, start));
                                    terminal.set_selection_head(Position::new(pos.line, end));
                                }
                            }
                        } else {
                            // Single click: start new selection
                            terminal.set_selection_anchor(pos);
                            terminal.set_selection_head(pos);
                        }
                    }
                    MouseEventKind::Moved => {
                        // Only extend selection if we have an anchor (dragging)
                        if terminal.selection_anchor().is_some() {
                            terminal.set_selection_head(pos);
                        }
                    }
                    MouseEventKind::Up => {
                        // Finalize selection - if anchor == head, clear selection
                        if terminal.selection_anchor() == terminal.selection_head() {
                            terminal.clear_selection();
                        }
                    }
                }
            }

            // Mark dirty since terminal may need redraw (e.g., selection)
            self.invalidation.merge(InvalidationKind::Layout);
        }
        // Other tab types (AgentOutput, Diff): no-op
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
    // Chunk: docs/chunks/viewport_scrolling - Editor-level scroll event routing
    /// Chunk: docs/chunks/file_picker - Scroll event routing to selector widget when selector is open
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    // Chunk: docs/chunks/pane_hover_scroll - Hover-targeted pane scrolling
    pub fn handle_scroll(&mut self, delta: ScrollDelta) {
        // When selector is open, forward scroll to selector
        if self.focus == EditorFocus::Selector {
            self.handle_scroll_selector(delta);
            return;
        }

        // Chunk: docs/chunks/content_tab_bar - Tab bar horizontal scrolling
        // Note: horizontal scroll in tab bar region is handled via handle_scroll_tab_bar
        // which is called from handle_mouse when scroll events occur in tab bar area

        // Chunk: docs/chunks/pane_hover_scroll - Determine target pane from mouse position
        // If the scroll event has a mouse position, use hit-testing to find the pane
        // under the cursor. Otherwise, fall back to the focused pane.
        let target_pane_id = self.find_pane_at_scroll_position(&delta);

        // Scroll the target pane without changing focus
        self.scroll_pane(target_pane_id, delta);
    }

    /// Finds the pane under the mouse cursor for hover-scroll routing.
    ///
    /// Returns the pane ID under the cursor if the scroll event includes mouse position
    /// and the position is within the content area. Falls back to the focused pane
    /// if no position is provided or if the cursor is outside the content area.
    // Chunk: docs/chunks/pane_hover_scroll - Pane hit-testing for hover-scroll
    fn find_pane_at_scroll_position(&self, delta: &ScrollDelta) -> crate::pane_layout::PaneId {
        use crate::pane_layout::calculate_pane_rects;

        // Get the focused pane as the default target
        let default_pane_id = self
            .editor
            .active_workspace()
            .map(|ws| ws.active_pane_id)
            .unwrap_or(0);

        // If no mouse position, use the focused pane
        let (mouse_x, mouse_y) = match delta.mouse_position {
            Some(pos) => pos,
            None => return default_pane_id,
        };

        // Check if we have a workspace with panes
        let workspace = match self.editor.active_workspace() {
            Some(ws) => ws,
            None => return default_pane_id,
        };

        // Calculate content area bounds
        let content_height = self.view_height - TAB_BAR_HEIGHT;
        let content_width = self.view_width - RAIL_WIDTH;

        // Check if mouse is in the content area (below tab bar, right of rail)
        // mouse_x, mouse_y are in screen coordinates (origin at top-left of view)
        if mouse_x < RAIL_WIDTH as f64
            || mouse_y < TAB_BAR_HEIGHT as f64
            || mouse_x >= self.view_width as f64
            || mouse_y >= self.view_height as f64
        {
            // Mouse is outside content area, use focused pane
            return default_pane_id;
        }

        // Convert screen coordinates to content-local coordinates
        let content_x = (mouse_x - RAIL_WIDTH as f64) as f32;
        let content_y = (mouse_y - TAB_BAR_HEIGHT as f64) as f32;

        // Calculate pane rects in content-local coordinates
        let bounds = (0.0, 0.0, content_width, content_height);
        let pane_rects = calculate_pane_rects(bounds, &workspace.pane_root);

        // Find the pane containing the mouse position
        for pane_rect in &pane_rects {
            if pane_rect.contains(content_x, content_y) {
                return pane_rect.pane_id;
            }
        }

        // No pane found at position (shouldn't happen if bounds are correct)
        default_pane_id
    }

    /// Scrolls the tab in the specified pane without changing focus.
    // Chunk: docs/chunks/pane_hover_scroll - Pane-targeted scroll execution
    // Chunk: docs/chunks/vsplit_scroll - Use pane-specific dimensions for scroll clamping
    fn scroll_pane(&mut self, target_pane_id: crate::pane_layout::PaneId, delta: ScrollDelta) {
        // Chunk: docs/chunks/vsplit_scroll - Get pane-specific dimensions before borrowing workspace.
        // Using full-window dimensions here causes scroll clamping to use incorrect wrap
        // calculations in split panes, preventing scrolling to the end of long files.
        let (content_height, content_width) = self
            .get_pane_content_dimensions(target_pane_id)
            .unwrap_or((self.view_height - TAB_BAR_HEIGHT, self.view_width - RAIL_WIDTH));

        // Get the target pane's active tab
        let ws = match self.editor.active_workspace_mut() {
            Some(ws) => ws,
            None => return,
        };

        let pane = match ws.pane_root.get_pane_mut(target_pane_id) {
            Some(p) => p,
            None => return,
        };

        let tab = match pane.active_tab_mut() {
            Some(t) => t,
            None => return,
        };

        // Chunk: docs/chunks/welcome_scroll - Welcome screen vertical scrolling
        // If this is an empty file tab (showing the welcome screen), route scroll
        // to the welcome screen offset rather than the buffer viewport.
        {
            use crate::workspace::TabKind;
            let is_welcome = tab.kind == TabKind::File
                && tab.as_text_buffer().map(|b| b.is_empty()).unwrap_or(false);

            if is_welcome {
                let current = tab.welcome_scroll_offset_px();
                let new_offset = (current + delta.dy as f32).max(0.0);
                tab.set_welcome_scroll_offset_px(new_offset);
                if (new_offset - current).abs() > 0.001 {
                    self.invalidation.merge(InvalidationKind::Layout);
                }
                return;
            }
        }

        // Try to get the text buffer and viewport for file tabs
        if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            // In Buffer or FindInFile mode, scroll the buffer
            // Create context and forward to focus target
            let font_metrics = self.font_metrics;

            // Chunk: docs/chunks/invalidation_separation - Use temporary DirtyRegion for EditorContext
            let mut ctx_dirty_region = DirtyRegion::None;

            // Chunk: docs/chunks/styled_line_cache - Pass dirty_lines for cache invalidation
            let mut ctx = EditorContext::new(
                buffer,
                viewport,
                &mut ctx_dirty_region,
                &mut self.dirty_lines,
                font_metrics,
                content_height,
                content_width,
            );
            self.focus_target.handle_scroll(delta, &mut ctx);

            // Chunk: docs/chunks/invalidation_separation - Convert to Content invalidation
            if ctx_dirty_region.is_dirty() {
                self.invalidation.merge(InvalidationKind::Content(ctx_dirty_region));
            }
        } else if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
            // Chunk: docs/chunks/terminal_scrollback_viewport - Terminal scrollback viewport handling
            // Terminal tab: handle scrolling based on terminal mode
            let is_alt_screen = terminal.is_alt_screen();
            let line_count = terminal.line_count();
            let line_height = self.font_metrics.line_height;

            if is_alt_screen {
                // Alternate screen mode (vim, htop, less): send scroll to PTY
                // Convert pixel delta to line count
                let line_height_f32 = line_height as f32;
                if line_height_f32 > 0.0 {
                    let lines = (delta.dy as f32 / line_height_f32).round() as i32;
                    if lines != 0 {
                        let modes = terminal.term_mode();
                        let bytes = InputEncoder::encode_scroll(
                            lines,
                            0, // col - use 0 for scroll events
                            0, // row - use 0 for scroll events
                            &lite_edit_input::Modifiers::default(),
                            modes,
                        );
                        if !bytes.is_empty() {
                            let _ = terminal.write_input(&bytes);
                        }
                    }
                }
            } else {
                // Primary screen: scroll the viewport through scrollback
                let current_px = viewport.scroll_offset_px();
                let new_px = current_px + delta.dy as f32;
                viewport.set_scroll_offset_px(new_px, line_count);

                // Mark dirty if scroll position changed
                if (viewport.scroll_offset_px() - current_px).abs() > 0.001 {
                    self.invalidation.merge(InvalidationKind::Layout);
                }
            }
        }
        // Other tab types (AgentOutput, Diff): no-op
    }

    /// Handles a scroll event when the selector is focused.
    /// Chunk: docs/chunks/file_picker - Scroll event routing to selector widget when selector is open
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

        // Chunk: docs/chunks/selector_scroll_end - Sync RowScroller row_height with geometry
        selector.set_item_height(geometry.item_height);
        // Update visible size on the selector (for arrow key navigation scroll)
        selector.update_visible_size(geometry.visible_items as f32 * geometry.item_height);

        // Forward scroll to selector (raw pixel delta, no rounding)
        selector.handle_scroll(delta.dy as f64);

        // Mark full viewport dirty for redraw
        self.invalidation.merge(InvalidationKind::Layout);
    }

    // Chunk: docs/chunks/dragdrop_file_paste - File drop handling
    /// Handles file drop events by inserting shell-escaped file paths.
    ///
    /// When files are dropped onto the view, this method:
    /// 1. Shell-escapes each path (single-quote escaping for POSIX shells)
    /// 2. Joins multiple paths with spaces
    /// 3. Inserts the result as text based on the current focus:
    ///    - Terminal tab: Uses bracketed paste encoding
    ///    - File tab: Inserts directly into the buffer
    ///    - Other modes (Selector, FindInFile, ConfirmDialog): Ignored
    ///
    /// This mirrors how macOS Terminal.app and Alacritty handle file drops.
    pub fn handle_file_drop(&mut self, paths: Vec<String>) {
        // Only handle drops when in Buffer focus mode
        // (Selector/FindInFile/ConfirmDialog don't accept file drops)
        if self.focus != EditorFocus::Buffer {
            return;
        }

        if paths.is_empty() {
            return;
        }

        // Shell-escape and join the paths
        let escaped_text = shell_escape_paths(&paths);

        // Get the active tab and route based on type
        let ws = match self.editor.active_workspace_mut() {
            Some(ws) => ws,
            None => return,
        };

        let tab = match ws.active_tab_mut() {
            Some(tab) => tab,
            None => return,
        };

        // Check for terminal tab first
        if let Some((terminal, _viewport)) = tab.terminal_and_viewport_mut() {
            // Terminal tab: use bracketed paste encoding (same as Cmd+V)
            let modes = terminal.term_mode();
            let bytes = InputEncoder::encode_paste(&escaped_text, modes);
            if !bytes.is_empty() {
                let _ = terminal.write_input(&bytes);
            }
            // Don't mark dirty - let poll_agents() detect the PTY echo
            return;
        }

        // File tab: insert text directly into buffer
        if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            let dirty_lines = buffer.insert_str(&escaped_text);
            let dirty = viewport.dirty_lines_to_region(&dirty_lines, buffer.line_count());
            // Chunk: docs/chunks/invalidation_separation - Content invalidation for text insertion
            self.invalidation.merge(InvalidationKind::Content(dirty));
            // Chunk: docs/chunks/styled_line_cache - Track dirty lines for cache invalidation
            self.dirty_lines.merge(dirty_lines);

            // Ensure cursor is visible after insertion
            let cursor_line = buffer.cursor_position().line;
            if viewport.ensure_visible(cursor_line, buffer.line_count()) {
                self.invalidation.merge(InvalidationKind::Layout);
            }

            // Mark the tab as dirty (unsaved changes)
            tab.dirty = true;

            // Chunk: docs/chunks/highlight_text_source - Sync highlighter after file drop insertion
            self.sync_active_tab_highlighter();
        }

        // Other tab types (AgentOutput, Diff): no-op
    }

    // Chunk: docs/chunks/unicode_ime_input - Text input event handlers

    /// Handles text insertion from IME, keyboard, paste, or dictation.
    ///
    /// This is the final text to insert after any IME composition is complete.
    /// The text is inserted at the cursor position (or replaces the specified range).
    pub fn handle_insert_text(&mut self, event: lite_edit_input::TextInputEvent) {
        // Only handle text input in Buffer focus mode
        if self.focus != EditorFocus::Buffer {
            return;
        }

        let text = &event.text;
        if text.is_empty() {
            return;
        }

        let ws = match self.editor.active_workspace_mut() {
            Some(ws) => ws,
            None => return,
        };

        let tab = match ws.active_tab_mut() {
            Some(tab) => tab,
            None => return,
        };

        // Check for terminal tab
        if let Some((terminal, _viewport)) = tab.terminal_and_viewport_mut() {
            // Terminal tab: write text as raw UTF-8 (not paste-bracketed)
            let bytes = text.as_bytes();
            if !bytes.is_empty() {
                let _ = terminal.write_input(bytes);
            }
            return;
        }

        // File tab: insert text into buffer
        if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            // Clear any marked text first (IME commit replaces marked text)
            buffer.clear_marked_text();

            let dirty_lines = buffer.insert_str(text);
            self.dirty_lines.merge(dirty_lines.clone());
            let dirty = viewport.dirty_lines_to_region(&dirty_lines, buffer.line_count());
            // Chunk: docs/chunks/invalidation_separation - Content invalidation for text insertion
            self.invalidation.merge(InvalidationKind::Content(dirty));

            // Ensure cursor is visible
            let cursor_line = buffer.cursor_position().line;
            if viewport.ensure_visible(cursor_line, buffer.line_count()) {
                self.invalidation.merge(InvalidationKind::Layout);
            }

            tab.dirty = true;
        }

        // Chunk: docs/chunks/highlight_text_source - Sync highlighter after text insertion
        self.sync_active_tab_highlighter();
    }

    /// Handles IME marked text (composition in progress).
    ///
    /// The marked text is displayed with an underline to indicate it's uncommitted.
    pub fn handle_set_marked_text(&mut self, event: lite_edit_input::MarkedTextEvent) {
        // Only handle in Buffer focus mode
        if self.focus != EditorFocus::Buffer {
            return;
        }

        let ws = match self.editor.active_workspace_mut() {
            Some(ws) => ws,
            None => return,
        };

        let tab = match ws.active_tab_mut() {
            Some(tab) => tab,
            None => return,
        };

        // File tab: set marked text on buffer
        if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            let dirty_lines = buffer.set_marked_text(&event.text, event.selected_range);
            self.dirty_lines.merge(dirty_lines.clone());
            let dirty = viewport.dirty_lines_to_region(&dirty_lines, buffer.line_count());
            // Chunk: docs/chunks/invalidation_separation - Content invalidation for marked text
            self.invalidation.merge(InvalidationKind::Content(dirty));

            // Ensure cursor is visible (cursor moves to end of marked text)
            let cursor_line = buffer.cursor_position().line;
            if viewport.ensure_visible(cursor_line, buffer.line_count()) {
                self.invalidation.merge(InvalidationKind::Layout);
            }
        }

        // Terminal tabs don't support marked text - IME sends final text directly

        // Chunk: docs/chunks/highlight_text_source - Sync highlighter after setting marked text
        self.sync_active_tab_highlighter();
    }

    /// Handles IME composition cancellation.
    ///
    /// Clears any marked text without inserting it.
    pub fn handle_unmark_text(&mut self) {
        // Only handle in Buffer focus mode
        if self.focus != EditorFocus::Buffer {
            return;
        }

        let ws = match self.editor.active_workspace_mut() {
            Some(ws) => ws,
            None => return,
        };

        let tab = match ws.active_tab_mut() {
            Some(tab) => tab,
            None => return,
        };

        // File tab: clear marked text
        if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
            let dirty_lines = buffer.cancel_marked_text();
            self.dirty_lines.merge(dirty_lines.clone());
            let dirty = viewport.dirty_lines_to_region(&dirty_lines, buffer.line_count());
            // Chunk: docs/chunks/invalidation_separation - Content invalidation for text clearing
            self.invalidation.merge(InvalidationKind::Content(dirty));
        }

        // Chunk: docs/chunks/highlight_text_source - Sync highlighter after clearing marked text
        self.sync_active_tab_highlighter();
    }

    // Chunk: docs/chunks/invalidation_separation - Updated to use InvalidationKind
    /// Returns true if any invalidation is pending (screen needs re-rendering).
    pub fn is_dirty(&self) -> bool {
        self.invalidation.is_dirty()
    }

    /// Called periodically to check for streaming file index updates.
    ///
    /// When the selector is open and the file index has discovered new paths,
    /// this re-queries the index with the current query and updates the selector's
    /// item list. This is the mechanism by which results stream in during the
    /// initial directory walk.
    ///
    /// Returns `DirtyRegion::FullViewport` if items were updated, `None` otherwise.
    /// Chunk: docs/chunks/file_picker - Streaming refresh mechanism for background file index updates
    // Chunk: docs/chunks/workspace_dir_picker - Use workspace's file index
    pub fn tick_picker(&mut self) -> DirtyRegion {
        // Only relevant when selector is active
        if self.focus != EditorFocus::Selector {
            return DirtyRegion::None;
        }

        // Get the workspace's file index and last_cache_version
        let workspace = match self.editor.active_workspace() {
            Some(ws) => ws,
            None => return DirtyRegion::None,
        };

        // Check if cache version has changed
        let current_version = workspace.file_index.cache_version();
        if current_version <= workspace.last_cache_version {
            return DirtyRegion::None;
        }

        // Re-query with current query
        let query = self
            .active_selector
            .as_ref()
            .map(|s| s.query())
            .unwrap_or_default();

        let results = workspace.file_index.query(&query);
        let items: Vec<String> = results
            .iter()
            .map(|r| r.path.display().to_string())
            .collect();

        // Update the selector items
        if let Some(ref mut widget) = self.active_selector {
            widget.set_items(items);
        }

        // Update workspace's cache version
        if let Some(ws) = self.editor.active_workspace_mut() {
            ws.last_cache_version = current_version;
        }

        DirtyRegion::FullViewport
    }

    // =========================================================================
    // Agent Polling (Chunk: docs/chunks/agent_lifecycle)
    // =========================================================================

    /// Polls all agents and standalone terminals in all workspaces for PTY events.
    ///
    /// Call this each frame to:
    /// 1. Process PTY output from agent processes
    /// 2. Process PTY output from standalone terminal tabs
    /// 3. Update agent state machines (Running → NeedsInput → Stale)
    /// 4. Update workspace status indicators
    ///
    /// Returns `(DirtyRegion, needs_rewakeup)`:
    /// - `DirtyRegion::FullViewport` if any agent or terminal had activity
    /// - `needs_rewakeup` is true if any terminal hit its byte budget and has more
    ///   data pending (caller should schedule a follow-up wakeup)
    // Chunk: docs/chunks/terminal_tab_spawn - Poll standalone terminals
    // Chunk: docs/chunks/terminal_flood_starvation - Propagate needs_rewakeup
    pub fn poll_agents(&mut self) -> (DirtyRegion, bool) {
        let mut any_activity = false;
        let mut any_needs_rewakeup = false;

        for workspace in &mut self.editor.workspaces {
            if workspace.poll_agent() {
                any_activity = true;
            }
            // Chunk: docs/chunks/terminal_tab_spawn - Poll standalone terminals
            let (had_events, needs_rewakeup) = workspace.poll_standalone_terminals();
            if had_events {
                any_activity = true;
            }
            if needs_rewakeup {
                any_needs_rewakeup = true;
            }
        }

        let dirty = if any_activity {
            DirtyRegion::FullViewport
        } else {
            DirtyRegion::None
        };

        (dirty, any_needs_rewakeup)
    }

    // Chunk: docs/chunks/invalidation_separation - Updated to use InvalidationKind
    /// Takes the invalidation kind, leaving `InvalidationKind::None` in its place.
    ///
    /// Call this after rendering to reset the dirty state.
    pub fn take_invalidation(&mut self) -> InvalidationKind {
        std::mem::take(&mut self.invalidation)
    }

    /// Takes the dirty region, leaving `DirtyRegion::None` in its place.
    ///
    /// **DEPRECATED**: Use `take_invalidation()` instead. This method exists
    /// for backward compatibility with drain_loop rendering code.
    pub fn take_dirty_region(&mut self) -> DirtyRegion {
        match std::mem::take(&mut self.invalidation) {
            InvalidationKind::None => DirtyRegion::None,
            InvalidationKind::Content(region) => region,
            InvalidationKind::Layout | InvalidationKind::Overlay => DirtyRegion::FullViewport,
        }
    }

    // Chunk: docs/chunks/styled_line_cache - Take dirty lines for cache invalidation
    /// Takes the dirty lines, leaving `DirtyLines::None` in its place.
    ///
    /// Call this after rendering to reset the dirty state. The returned value
    /// should be passed to `Renderer::invalidate_styled_lines()` to invalidate
    /// cached styled lines for the changed buffer lines.
    pub fn take_dirty_lines(&mut self) -> DirtyLines {
        std::mem::take(&mut self.dirty_lines)
    }

    // Chunk: docs/chunks/styled_line_cache - Take clear cache flag for tab switch
    /// Takes the clear_styled_line_cache flag, leaving `false` in its place.
    ///
    /// Call this at the start of each render pass. If true, call
    /// `Renderer::clear_styled_line_cache()` to fully clear the cache.
    /// This is set on tab switch to prevent stale cache entries from a
    /// previous buffer causing visual artifacts.
    pub fn take_clear_styled_line_cache(&mut self) -> bool {
        std::mem::take(&mut self.clear_styled_line_cache)
    }

    /// Toggles cursor visibility for blink animation.
    ///
    /// Focus-aware: only the cursor in the currently focused area (buffer or overlay)
    /// blinks. When an overlay (Selector or FindInFile) is focused, the main buffer
    /// cursor remains static (visible), and the overlay cursor blinks.
    ///
    /// Returns the dirty region for the cursor line if visibility changed.
    /// If the user recently typed, this keeps the cursor solid instead of toggling.
    ///
    /// Chunk: docs/chunks/cursor_blink_focus - Focus-aware cursor blink toggle
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    pub fn toggle_cursor_blink(&mut self) -> DirtyRegion {
        // Terminal tabs don't have a text buffer cursor to blink.
        // The terminal has its own cursor managed by the PTY.
        // Return FullViewport for terminal tabs to ensure the cursor is rendered.
        if !self.active_tab_is_file() {
            // For terminal tabs, just toggle the visibility state
            // and return FullViewport since the cursor is part of the terminal grid.
            let now = Instant::now();
            let since_keystroke = now.duration_since(self.last_keystroke);

            if since_keystroke.as_millis() < CURSOR_BLINK_INTERVAL_MS as u128 {
                if !self.cursor_visible {
                    self.cursor_visible = true;
                    return DirtyRegion::FullViewport;
                }
                return DirtyRegion::None;
            }

            self.cursor_visible = !self.cursor_visible;
            return DirtyRegion::FullViewport;
        }

        let now = Instant::now();

        match self.focus {
            EditorFocus::Buffer => {
                // Buffer has focus - toggle the main buffer cursor
                let since_keystroke = now.duration_since(self.last_keystroke);

                // If user typed recently, keep cursor solid
                if since_keystroke.as_millis() < CURSOR_BLINK_INTERVAL_MS as u128 {
                    if !self.cursor_visible {
                        self.cursor_visible = true;
                        return self.cursor_dirty_region();
                    }
                    return DirtyRegion::None;
                }

                // Toggle buffer cursor visibility
                self.cursor_visible = !self.cursor_visible;
                self.cursor_dirty_region()
            }
            EditorFocus::Selector | EditorFocus::FindInFile => {
                // Overlay has focus - toggle the overlay cursor, not the buffer cursor
                let since_keystroke = now.duration_since(self.last_overlay_keystroke);

                // If user typed recently, keep cursor solid
                if since_keystroke.as_millis() < CURSOR_BLINK_INTERVAL_MS as u128 {
                    if !self.overlay_cursor_visible {
                        self.overlay_cursor_visible = true;
                        // Return FullViewport since overlay cursors aren't on a specific buffer line
                        return DirtyRegion::FullViewport;
                    }
                    return DirtyRegion::None;
                }

                // Toggle overlay cursor visibility
                self.overlay_cursor_visible = !self.overlay_cursor_visible;
                // Return FullViewport since overlay cursors aren't on a specific buffer line
                DirtyRegion::FullViewport
            }
            // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog has no cursor to blink
            EditorFocus::ConfirmDialog => {
                // The confirm dialog doesn't have a text input cursor, so no blink needed.
                // Return None to avoid unnecessary redraws.
                DirtyRegion::None
            }
        }
    }

    // Chunk: docs/chunks/dirty_region_wrap_aware - Wrap-aware dirty region conversion
    /// Returns the dirty region for just the cursor line.
    ///
    /// This uses wrap-aware conversion to correctly handle soft line wrapping,
    /// where buffer line indices can be much smaller than screen row indices.
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    // Chunk: docs/chunks/cursor_blink_stall - Defense-in-depth for uninitialized viewport
    fn cursor_dirty_region(&self) -> DirtyRegion {
        // For terminal tabs, return FullViewport since the cursor is part of the grid.
        if let Some(buffer) = self.try_buffer() {
            // Defense-in-depth: if viewport not properly sized, force full repaint.
            // This guards against the cursor blink stall bug even if
            // dirty_lines_to_region_wrapped's guard is somehow bypassed.
            if self.viewport().visible_lines() == 0 {
                return DirtyRegion::FullViewport;
            }

            let cursor_line = buffer.cursor_position().line;
            let line_count = buffer.line_count();

            // Create WrapLayout for the current viewport width
            let wrap_layout = crate::wrap_layout::WrapLayout::new(self.view_width, &self.font_metrics);

            // Capture line lengths for the closure
            let line_lens: Vec<usize> = (0..line_count)
                .map(|line| buffer.line_len(line))
                .collect();

            self.viewport().dirty_lines_to_region_wrapped(
                &lite_edit_buffer::DirtyLines::Single(cursor_line),
                line_count,
                &wrap_layout,
                |line| line_lens.get(line).copied().unwrap_or(0),
            )
        } else {
            DirtyRegion::FullViewport
        }
    }

    // Chunk: docs/chunks/invalidation_separation - Layout invalidation for full rerender
    /// Marks a full layout invalidation (e.g., after buffer replacement, resize).
    ///
    /// This signals Layout invalidation, which:
    /// - Triggers pane rect recalculation
    /// - Forces full content re-render
    pub fn mark_full_dirty(&mut self) {
        self.invalidation = InvalidationKind::Layout;
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
    // Chunk: docs/chunks/file_save - File loading with UTF-8 lossy conversion, cursor/scroll reset
    // Chunk: docs/chunks/tab_click_cursor_placement - Sync viewport on file association
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    // Chunk: docs/chunks/syntax_highlighting - Setup syntax highlighting on file open
    pub fn associate_file(&mut self, path: PathBuf) {
        // File association only makes sense for file tabs.
        // Terminal tabs don't have a TextBuffer to load into.
        if !self.active_tab_is_file() {
            return;
        }

        if path.exists() {
            // Read file contents with UTF-8 lossy conversion
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let contents = String::from_utf8_lossy(&bytes);
                    *self.buffer_mut() = TextBuffer::from_str(&contents);
                    self.buffer_mut().set_cursor(lite_edit_buffer::Position::new(0, 0));
                    let line_count = self.buffer().line_count();
                    self.viewport_mut().scroll_to(0, line_count);

                    // Chunk: docs/chunks/base_snapshot_reload - Populate base on load
                    // Store base content snapshot for three-way merge
                    if let Some(ws) = self.editor.active_workspace_mut() {
                        if let Some(tab) = ws.active_tab_mut() {
                            tab.base_content = Some(contents.to_string());
                        }
                    }
                }
                Err(_) => {
                    // Silently ignore read errors (out of scope for this chunk)
                }
            }
        }
        // For non-existent files, leave buffer as-is (file picker already created empty file)

        self.set_associated_file(Some(path.clone()));

        // Chunk: docs/chunks/buffer_file_watching - Register external file watch
        // Register a watch for files outside the workspace. This is safe to call
        // for workspace-internal files because register() checks is_external() first.
        if let Err(e) = self.buffer_file_watcher.register(&path) {
            // Log but don't fail - watching is a nice-to-have, not critical
            eprintln!("Failed to watch external file {:?}: {}", path, e);
        }

        // Chunk: docs/chunks/syntax_highlighting - Set up syntax highlighting
        // Try to set up syntax highlighting based on file extension
        self.setup_active_tab_highlighting();

        // Sync viewport to ensure dirty region calculations work correctly
        // (handles case of file picker confirming into a newly created tab)
        self.sync_active_tab_viewport();
        self.invalidation.merge(InvalidationKind::Layout);
    }

    // Chunk: docs/chunks/syntax_highlighting - Setup syntax highlighting helper
    /// Sets up syntax highlighting for the active tab based on its file extension.
    ///
    /// This is called after loading file content to enable syntax highlighting
    /// for recognized file types. If the extension is not recognized, the tab
    /// remains without a highlighter (plain text).
    fn setup_active_tab_highlighting(&mut self) {
        // Extract what we need before the mutable borrow
        let theme = SyntaxTheme::catppuccin_mocha();

        // Get the active tab and set up highlighting
        if let Some(ws) = self.editor.active_workspace_mut() {
            if let Some(tab) = ws.active_tab_mut() {
                tab.setup_highlighting(&self.language_registry, theme);
            }
        }
    }

    // Chunk: docs/chunks/syntax_highlighting - Sync highlighter after buffer edit
    /// Syncs the active tab's highlighter with the current buffer content.
    ///
    /// Call this after any buffer mutation to keep syntax highlighting in sync.
    /// This performs a full re-parse rather than incremental update.
    fn sync_active_tab_highlighter(&mut self) {
        if let Some(ws) = self.editor.active_workspace_mut() {
            if let Some(tab) = ws.active_tab_mut() {
                tab.sync_highlighter();
            }
        }
    }

    /// Returns the window title based on the current file association.
    ///
    /// Returns the filename if a file is associated, or "Untitled" otherwise.
    /// When multiple workspaces exist, includes the workspace label.
    // Chunk: docs/chunks/file_save - Derives window title from associated filename or 'Untitled'
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
    /// On successful save, clears the tab's dirty flag and conflict mode.
    ///
    /// When a tab was in conflict mode, saving signals that the user has finished
    /// resolving conflicts. After clearing conflict mode, we re-check the disk to
    /// see if external changes arrived during conflict resolution. If the disk
    /// differs from what we just saved, a new merge cycle is triggered.
    // Chunk: docs/chunks/file_save - Writes buffer content to associated file path
    // Chunk: docs/chunks/terminal_active_tab_safety - Guard for terminal tabs
    // Chunk: docs/chunks/unsaved_tab_tint - Clear dirty flag on successful save
    // Chunk: docs/chunks/conflict_mode_lifecycle - Clear conflict mode and re-check disk on save
    fn save_file(&mut self) {
        // Save only makes sense for file tabs with a TextBuffer
        if !self.active_tab_is_file() {
            return;
        }

        let path = match self.associated_file() {
            Some(p) => p.clone(),
            None => return, // No file associated - no-op
        };

        // Chunk: docs/chunks/file_change_events - Suppress before write
        // Mark this path for suppression before writing. This prevents the
        // filesystem watcher from triggering a reload/merge flow for our own save.
        self.file_change_suppression.suppress(path.clone());

        let content = self.buffer().content();
        if std::fs::write(&path, content.as_bytes()).is_ok() {
            // Track whether we were in conflict mode before clearing it
            let was_in_conflict_mode = self.editor.active_workspace()
                .and_then(|ws| ws.active_tab())
                .map(|t| t.conflict_mode)
                .unwrap_or(false);

            // Clear dirty flag and conflict mode on successful save
            if let Some(ws) = self.editor.active_workspace_mut() {
                if let Some(tab) = ws.active_tab_mut() {
                    tab.dirty = false;
                    // Chunk: docs/chunks/base_snapshot_reload - Populate base on save
                    // Update base content snapshot to match saved content
                    tab.base_content = Some(content.clone());
                    // Chunk: docs/chunks/conflict_mode_lifecycle - Clear conflict mode
                    tab.conflict_mode = false;
                }
            }

            // Chunk: docs/chunks/conflict_mode_lifecycle - Re-check disk after conflict resolution
            // If we were in conflict mode, check if the disk has changed since our save.
            // This catches the case where another process modified the file while we
            // were resolving conflicts. If the disk differs, trigger a new merge cycle.
            if was_in_conflict_mode {
                // Read disk content to compare with what we saved
                if let Ok(disk_bytes) = std::fs::read(&path) {
                    let disk_content = String::from_utf8_lossy(&disk_bytes).to_string();
                    // If disk differs from what we just wrote, an external change arrived
                    // during conflict resolution. Need to merge this new change.
                    if disk_content != content {
                        // Re-read to trigger merge - the buffer is now clean (dirty=false),
                        // but disk differs, so we need to merge the new external changes.
                        // Mark the buffer dirty first to allow merge to proceed.
                        if let Some(ws) = self.editor.active_workspace_mut() {
                            if let Some(tab) = ws.active_tab_mut() {
                                tab.dirty = true;
                            }
                        }
                        // Trigger merge for the new external changes
                        let _ = self.merge_file_tab(&path);
                    }
                }
            }
        }
        // Silently ignore write errors (out of scope for this chunk)
    }

// Chunk: docs/chunks/deletion_rename_handling - Save buffer to specific path
    /// Saves the active buffer to the specified path, recreating the file.
    ///
    /// This is used when the user chooses "Save" in response to a file deletion
    /// notification. It writes the buffer contents to the specified path,
    /// suppresses the resulting file change event, and clears the dirty flag.
    fn save_buffer_to_path(&mut self, path: &std::path::Path) {
        // Save only makes sense for file tabs with a TextBuffer
        if !self.active_tab_is_file() {
            return;
        }

        // Suppress the file change event for our own write
        self.file_change_suppression.suppress(path.to_path_buf());

        let content = self.buffer().content();
        if std::fs::write(path, content.as_bytes()).is_ok() {
            // Clear dirty flag on successful save
            if let Some(ws) = self.editor.active_workspace_mut() {
                if let Some(tab) = ws.active_tab_mut() {
                    tab.dirty = false;
                }
            }
        }
        // Silently ignore write errors (out of scope for this chunk)
    }

    // Chunk: docs/chunks/conflict_mode_lifecycle - Check if tab is in conflict mode
    /// Checks whether a tab at the given path is in conflict mode.
    ///
    /// Returns `true` if a tab exists for this path and has `conflict_mode == true`.
    /// Returns `false` if no matching tab exists or if the tab is not in conflict mode.
    ///
    /// This is used by `handle_file_changed` to skip processing FileChanged events
    /// for tabs that are actively resolving merge conflicts.
    pub fn is_tab_in_conflict_mode(&self, path: &Path) -> bool {
        for ws in &self.editor.workspaces {
            if let Some(tab) = ws.pane_root.all_panes()
                .iter()
                .flat_map(|p| p.tabs.iter())
                .find(|t| t.associated_file.as_ref() == Some(&path.to_path_buf()))
            {
                return tab.conflict_mode;
            }
        }
        false
    }

    /// Reload a file tab's buffer from disk.
    ///
    /// This is called when `FileChanged` arrives for a tab with `dirty == false`.
    /// It re-reads the file, replaces the buffer content, updates `base_content`,
    /// preserves cursor position (clamped to buffer bounds), and re-applies
    /// syntax highlighting.
    ///
    /// Returns `true` if the reload succeeded, `false` if the file couldn't be
    /// read or no matching tab was found, or if the tab has unsaved changes.
    // Chunk: docs/chunks/base_snapshot_reload - Clean buffer reload
    pub fn reload_file_tab(&mut self, path: &Path) -> bool {
        // Find the workspace and tab for this path
        // We need to search all workspaces since the file could be open in any of them
        let mut found_workspace_idx: Option<usize> = None;

        for (ws_idx, ws) in self.editor.workspaces.iter().enumerate() {
            if ws.find_tab_by_path(path).is_some() {
                found_workspace_idx = Some(ws_idx);
                break;
            }
        }

        let ws_idx = match found_workspace_idx {
            Some(idx) => idx,
            None => return false, // No tab has this path
        };

        // Get the workspace and tab mutably
        let ws = &mut self.editor.workspaces[ws_idx];
        let tab = match ws.find_tab_mut_by_path(path) {
            Some(t) => t,
            None => return false, // Should not happen, but be defensive
        };

        // Only reload if the tab is clean (no unsaved changes)
        if tab.dirty {
            // Defer to three_way_merge chunk - do nothing for dirty buffers
            return false;
        }

        // Read the file content
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return false, // File couldn't be read
        };
        let new_content = String::from_utf8_lossy(&bytes).to_string();

        // Store old cursor position before replacing buffer
        let old_cursor = tab.as_text_buffer()
            .map(|buf| buf.cursor_position())
            .unwrap_or(Position::new(0, 0));

        // Replace buffer content
        let buffer = match tab.as_text_buffer_mut() {
            Some(buf) => buf,
            None => return false, // Not a file tab
        };
        *buffer = TextBuffer::from_str(&new_content);

        // Clamp cursor position to new buffer bounds
        let new_cursor = clamp_position_to_buffer(old_cursor, buffer);
        buffer.set_cursor(new_cursor);

        // Update base_content
        tab.base_content = Some(new_content);

        // Re-apply syntax highlighting
        let theme = SyntaxTheme::catppuccin_mocha();
        tab.setup_highlighting(&self.language_registry, theme);

        // Mark full viewport dirty
        self.invalidation.merge(InvalidationKind::Layout);

        true
    }

    // Chunk: docs/chunks/three_way_merge - Merge dirty buffer with external changes
    /// Merges external file changes into a dirty buffer using three-way merge.
    ///
    /// This is called when a FileChanged event arrives for a tab with `dirty == true`.
    /// The merge uses the stored `base_content` as the common ancestor, the current
    /// buffer content as "ours", and the new disk content as "theirs".
    ///
    /// # Behavior
    ///
    /// - Reads the new disk content
    /// - Performs three-way merge: base_content → buffer, base_content → disk
    /// - On clean merge: replaces buffer content with the merged result
    /// - On conflict: replaces buffer content including conflict markers
    /// - Cursor position is clamped to new buffer bounds
    /// - Updates `base_content` to new disk content
    /// - Dirty flag remains true (user still has unsaved changes)
    /// - Re-applies syntax highlighting
    /// - Marks full viewport dirty
    ///
    /// # Returns
    ///
    /// `Some(MergeResult)` if merge was performed, `None` if:
    /// - No matching tab was found
    /// - Tab is not dirty (should use reload_file_tab instead)
    /// - Tab is not a file tab
    /// - File couldn't be read
    /// - base_content is missing (shouldn't happen for dirty buffers)
    pub fn merge_file_tab(&mut self, path: &Path) -> Option<lite_edit::merge::MergeResult> {
        use lite_edit::merge::three_way_merge;

        // Find the workspace and tab for this path
        let mut found_workspace_idx: Option<usize> = None;

        for (ws_idx, ws) in self.editor.workspaces.iter().enumerate() {
            if ws.find_tab_by_path(path).is_some() {
                found_workspace_idx = Some(ws_idx);
                break;
            }
        }

        let ws_idx = found_workspace_idx?;

        // Get the workspace and tab mutably
        let ws = &mut self.editor.workspaces[ws_idx];
        let tab = ws.find_tab_mut_by_path(path)?;

        // Only merge if the tab is dirty
        if !tab.dirty {
            // Clean tabs should use reload_file_tab instead
            return None;
        }

        // Get the base content (required for merge)
        let base_content = tab.base_content.clone()?;

        // Get current buffer content
        let buffer = tab.as_text_buffer()?;
        let ours_content = buffer.content();

        // Store old cursor position before replacing buffer
        let old_cursor = buffer.cursor_position();

        // Read the new disk content
        let bytes = std::fs::read(path).ok()?;
        let theirs_content = String::from_utf8_lossy(&bytes).to_string();

        // Perform three-way merge
        let merge_result = three_way_merge(&base_content, &ours_content, &theirs_content);
        let merged_content = merge_result.content().to_string();

        // Replace buffer content with merged result
        let buffer = tab.as_text_buffer_mut()?;
        *buffer = TextBuffer::from_str(&merged_content);

        // Clamp cursor position to new buffer bounds
        let new_cursor = clamp_position_to_buffer(old_cursor, buffer);
        buffer.set_cursor(new_cursor);

        // Update base_content to the new disk content
        // (so subsequent saves will correctly detect what changed)
        tab.base_content = Some(theirs_content);

        // Dirty flag remains true - user still has unsaved merged changes

        // Chunk: docs/chunks/conflict_mode_lifecycle - Set conflict_mode when merge produces conflicts
        // Set conflict_mode if the merge produced conflict markers
        if !merge_result.is_clean() {
            tab.conflict_mode = true;
        }

        // Re-apply syntax highlighting
        let theme = SyntaxTheme::catppuccin_mocha();
        tab.setup_highlighting(&self.language_registry, theme);

        // Mark full viewport dirty
        self.invalidation.merge(InvalidationKind::Layout);

        Some(merge_result)
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
    /// Opens a directory picker dialog (NSOpenPanel) for the user to select
    /// the workspace root directory. If the user selects a directory, a new
    /// workspace is created with that directory as the root_path. The workspace
    /// label is derived from the directory name.
    ///
    /// If the user cancels the dialog, no workspace is created.
    ///
    /// For the first workspace of a session (startup workspace via `add_startup_workspace`),
    /// an empty file tab is created to show the welcome screen. For subsequent workspaces
    /// created via this method, a terminal tab is spawned instead, giving experienced
    /// users immediate shell access in the project directory.
    // Chunk: docs/chunks/workspace_dir_picker - Directory picker for new workspaces
    // Chunk: docs/chunks/workspace_initial_terminal - Terminal tab for subsequent workspaces
    pub fn new_workspace(&mut self) {
        // Show directory picker dialog
        let selected_dir = match dir_picker::pick_directory() {
            Some(dir) => dir,
            None => return, // User cancelled, do nothing
        };

        // Derive workspace label from directory name
        let label = selected_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".to_string());

        // Check if this is a subsequent workspace (not the startup workspace).
        // If at least one workspace already exists, we create a terminal tab instead
        // of an empty file tab, giving experienced users immediate shell access.
        let is_subsequent = self.editor.workspace_count() >= 1;

        if is_subsequent {
            // Subsequent workspaces get a terminal tab instead of empty file tab
            self.editor.new_workspace_without_tab(label, selected_dir.clone());
            self.new_terminal_tab();
        } else {
            // First workspace gets empty file tab (for welcome screen)
            self.editor.new_workspace(label, selected_dir.clone());
        }

        // Chunk: docs/chunks/buffer_file_watching - Update buffer file watcher root
        // Update the buffer file watcher's workspace root for the new workspace.
        self.buffer_file_watcher.set_workspace_root(selected_dir);

        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Closes the active workspace.
    ///
    /// Does nothing if this is the last workspace.
    pub fn close_active_workspace(&mut self) {
        if self.editor.workspace_count() > 1 {
            self.editor.close_workspace(self.editor.active_workspace);
            // Chunk: docs/chunks/buffer_file_watching - Update buffer file watcher root
            // After closing a workspace, update the buffer file watcher's root to the
            // newly active workspace's root path.
            if let Some(ws) = self.editor.active_workspace() {
                self.buffer_file_watcher.set_workspace_root(ws.root_path.clone());
            }
            self.invalidation.merge(InvalidationKind::Layout);
        }
    }

    /// Switches to the workspace at the given index (0-based).
    ///
    /// Does nothing if the index is out of bounds.
    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.editor.workspace_count() && index != self.editor.active_workspace {
            self.editor.switch_workspace(index);
            // Chunk: docs/chunks/buffer_file_watching - Update buffer file watcher root
            // Update the buffer file watcher's workspace root when switching workspaces.
            // This ensures external file detection uses the new workspace's root path.
            if let Some(ws) = self.editor.active_workspace() {
                self.buffer_file_watcher.set_workspace_root(ws.root_path.clone());
            }
            self.invalidation.merge(InvalidationKind::Layout);
        }
    }

    /// Cycles to the next workspace (wraps from last to first).
    ///
    /// Does nothing if there's only one workspace.
    // Chunk: docs/chunks/workspace_switching - Cmd+] workspace cycling
    pub fn next_workspace(&mut self) {
        let count = self.editor.workspace_count();
        if count > 1 {
            let next = (self.editor.active_workspace + 1) % count;
            self.switch_workspace(next);
        }
    }

    /// Cycles to the previous workspace (wraps from first to last).
    ///
    /// Does nothing if there's only one workspace.
    // Chunk: docs/chunks/workspace_switching - Cmd+[ workspace cycling
    pub fn prev_workspace(&mut self) {
        let count = self.editor.workspace_count();
        if count > 1 {
            let prev = if self.editor.active_workspace == 0 {
                count - 1
            } else {
                self.editor.active_workspace - 1
            };
            self.switch_workspace(prev);
        }
    }

    // =========================================================================
    // Tab Management (Chunk: docs/chunks/content_tab_bar)
    // =========================================================================

    /// Switches to the tab at the given index in the active pane.
    ///
    /// Does nothing if the index is out of bounds or if it's the current tab.
    // Chunk: docs/chunks/content_tab_bar - Switch active tab; clears unread badge
    // Chunk: docs/chunks/tab_bar_interaction - Click-to-switch tab activation
    // Chunk: docs/chunks/tab_click_cursor_placement - Sync viewport on tab switch
    // Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
    pub fn switch_tab(&mut self, index: usize) {
        let switched = if let Some(workspace) = self.editor.active_workspace_mut() {
            if let Some(pane) = workspace.active_pane_mut() {
                if index < pane.tabs.len() && index != pane.active_tab {
                    pane.switch_tab(index);
                    // switch_tab already clears unread badge
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if switched {
            // Sync viewport to ensure dirty region calculations work correctly
            // (must be done after pane.switch_tab so active_tab is updated)
            self.sync_active_tab_viewport();
            self.invalidation.merge(InvalidationKind::Layout);
            // Chunk: docs/chunks/styled_line_cache - Clear cache on tab switch
            // Mark that the styled line cache should be cleared to prevent stale
            // entries from the previous buffer causing visual artifacts.
            self.clear_styled_line_cache = true;
        }
    }

    /// Closes the tab at the given index in the active pane.
    ///
    /// If this is the last tab in the last pane, creates a new empty tab instead of closing.
    /// If the tab is dirty (has unsaved changes), shows a confirm dialog asking the user
    /// whether to abandon the changes or cancel.
    // Chunk: docs/chunks/content_tab_bar - Close tab with dirty-buffer guard (Cmd+W)
    // Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
    // Chunk: docs/chunks/pane_close_last_tab - Cleanup empty panes on last tab close
    // Chunk: docs/chunks/dirty_tab_close_confirm - Confirm dialog for dirty tabs
    pub fn close_tab(&mut self, index: usize) {
        // Pre-compute values needed for fallback before borrowing workspace mutably
        let tab_id = self.editor.gen_tab_id();
        let line_height = self.editor.line_height();

        // Chunk: docs/chunks/buffer_file_watching - Extract associated file for watcher cleanup
        // Get the associated file path before closing (for watcher cleanup)
        let associated_file = self.editor
            .active_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|pane| pane.tabs.get(index))
            .and_then(|tab| tab.associated_file.clone());

        // Chunk: docs/chunks/dirty_tab_close_confirm - Show confirm dialog for dirty tabs
        // Check if the tab is dirty and show confirmation dialog if so.
        // We check this in a separate borrow scope so we can call show_confirm_dialog after.
        let dirty_pane_id = self.editor
            .active_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|pane| {
                pane.tabs.get(index).and_then(|tab| {
                    if tab.dirty { Some(pane.id) } else { None }
                })
            });

        if let Some(pane_id) = dirty_pane_id {
            self.show_confirm_dialog(pane_id, index);
            return;
        }

        // Chunk: docs/chunks/terminal_close_guard - Check terminal process liveness
        // Check if this is a terminal with an active process
        let active_terminal_pane_id = self.editor
            .active_workspace()
            .and_then(|ws| ws.active_pane())
            .and_then(|pane| {
                use crate::workspace::TabKind;
                pane.tabs.get(index).and_then(|tab| {
                    if tab.kind == TabKind::Terminal {
                        Some(pane.id)
                    } else {
                        None
                    }
                })
            });

        if let Some(pane_id) = active_terminal_pane_id {
            if self.is_terminal_with_active_process(pane_id, index) {
                self.show_terminal_close_confirm(pane_id, index);
                return;
            }
        }

        if let Some(workspace) = self.editor.active_workspace_mut() {
            let pane_count = workspace.pane_root.pane_count();

            if pane_count > 1 {
                // Multi-pane layout: check if pane will become empty
                let pane_will_be_empty = workspace.active_pane()
                    .map(|p| p.tabs.len() == 1)
                    .unwrap_or(false);

                // Find fallback focus BEFORE mutating (to avoid borrow conflicts)
                let fallback_focus = if pane_will_be_empty {
                    workspace.find_fallback_focus()
                } else {
                    None
                };

                // Close the tab
                if let Some(pane) = workspace.active_pane_mut() {
                    pane.close_tab(index);
                }

                // If pane is now empty, cleanup the tree and update focus
                if pane_will_be_empty {
                    if let Some(fallback_pane_id) = fallback_focus {
                        // Update focus BEFORE cleanup (cleanup removes the empty pane)
                        workspace.active_pane_id = fallback_pane_id;
                    }
                    // Cleanup empty panes (collapses the tree)
                    crate::pane_layout::cleanup_empty_panes(&mut workspace.pane_root);
                }
            } else {
                // Single pane layout
                if let Some(pane) = workspace.active_pane_mut() {
                    if pane.tabs.len() > 1 {
                        // Multiple tabs: just close the tab
                        pane.close_tab(index);
                    } else {
                        // Single tab in single pane: replace with empty tab
                        let new_tab = crate::workspace::Tab::empty_file(tab_id, line_height);
                        pane.tabs[0] = new_tab;
                        pane.active_tab = 0;
                    }
                }
            }
            self.invalidation.merge(InvalidationKind::Layout);
        }

        // Chunk: docs/chunks/buffer_file_watching - Unregister external file watch
        // Unregister the file watcher for the closed tab (if it had an associated file)
        if let Some(ref path) = associated_file {
            self.buffer_file_watcher.unregister(path);
        }
    }

    /// Closes the active tab in the active pane.
    // Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
    pub fn close_active_tab(&mut self) {
        let active_tab_index = self.editor
            .active_workspace()
            .and_then(|ws| ws.active_pane())
            .map(|pane| pane.active_tab)
            .unwrap_or(0);
        self.close_tab(active_tab_index);
    }

    /// Cycles to the next tab in the active pane.
    ///
    /// Wraps around from the last tab to the first.
    /// Does nothing if there's only one tab.
    // Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
    pub fn next_tab(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            if let Some(pane) = workspace.active_pane() {
                if pane.tabs.len() > 1 {
                    let next = (pane.active_tab + 1) % pane.tabs.len();
                    self.switch_tab(next);
                }
            }
        }
    }

    /// Cycles to the previous tab in the active pane.
    ///
    /// Wraps around from the first tab to the last.
    /// Does nothing if there's only one tab.
    // Chunk: docs/chunks/tiling_workspace_integration - Resolve through pane tree
    pub fn prev_tab(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            if let Some(pane) = workspace.active_pane() {
                if pane.tabs.len() > 1 {
                    let prev = if pane.active_tab == 0 {
                        pane.tabs.len() - 1
                    } else {
                        pane.active_tab - 1
                    };
                    self.switch_tab(prev);
                }
            }
        }
    }

    /// Creates a new empty tab in the active workspace and switches to it.
    ///
    /// This is triggered by Cmd+T. For now, this creates an empty file tab.
    /// Terminal tab creation will be added in the terminal_emulator chunk.
    // Chunk: docs/chunks/content_tab_bar - Create new empty file tab (Cmd+T)
    // Chunk: docs/chunks/tab_click_cursor_placement - Sync viewport on tab creation
    pub fn new_tab(&mut self) {
        let tab_id = self.editor.gen_tab_id();
        let line_height = self.editor.line_height();
        let new_tab = crate::workspace::Tab::empty_file(tab_id, line_height);

        if let Some(workspace) = self.editor.active_workspace_mut() {
            workspace.add_tab(new_tab);
        }

        // Sync viewport to ensure dirty region calculations work correctly
        self.sync_active_tab_viewport();

        // Ensure the new tab is visible in the tab bar
        self.ensure_active_tab_visible();
        self.invalidation.merge(InvalidationKind::Layout);
    }

    // Chunk: docs/chunks/terminal_tab_spawn - Cmd+Shift+T terminal spawning
    // Chunk: docs/chunks/tiling_workspace_integration - Count terminals across all panes

    /// Counts existing terminal tabs in the active workspace (across all panes).
    ///
    /// Returns 0 if no workspace is active.
    fn terminal_tab_count(&self) -> usize {
        use crate::workspace::TabKind;
        self.editor
            .active_workspace()
            .map(|ws| {
                ws.all_panes()
                    .iter()
                    .flat_map(|pane| pane.tabs.iter())
                    .filter(|t| t.kind == TabKind::Terminal)
                    .count()
            })
            .unwrap_or(0)
    }

    // Chunk: docs/chunks/terminal_tab_spawn - Cmd+Shift+T terminal spawning
    // Chunk: docs/chunks/terminal_shell_env - Login shell spawning for full environment
    /// Creates a new standalone terminal tab in the active workspace.
    ///
    /// The terminal runs the user's default shell from the passwd database,
    /// spawned as a login shell to ensure the full profile chain is sourced
    /// (`~/.zprofile`, `~/.zshrc`, etc.). This ensures the terminal has the
    /// user's complete environment including PATH entries from tools like
    /// pyenv, nvm, rbenv, etc.
    ///
    /// Terminal dimensions are computed from the current viewport size and
    /// font metrics.
    ///
    /// Terminal tabs are labeled "Terminal", "Terminal 2", etc. based on how
    /// many terminal tabs already exist in the workspace.
    pub fn new_terminal_tab(&mut self) {
        use crate::left_rail::RAIL_WIDTH;
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::workspace::Tab;
        use lite_edit_terminal::TerminalBuffer;

        // Chunk: docs/chunks/terminal_pane_initial_sizing - Use pane dimensions for terminal sizing
        // Get active pane ID to compute pane-specific dimensions. In multi-pane layouts, the
        // active pane is only a fraction of the window content area, so we use the actual pane
        // dimensions rather than the full window dimensions.
        let pane_dimensions = self.editor.active_workspace()
            .map(|ws| ws.active_pane_id)
            .and_then(|pane_id| self.get_pane_content_dimensions(pane_id));

        let (content_height, content_width) = match pane_dimensions {
            Some((height, width)) => (height, width),
            None => {
                // Fall back to full window dimensions (single-pane or dimensions not set)
                (self.view_height - TAB_BAR_HEIGHT, self.view_width - RAIL_WIDTH)
            }
        };

        // Guard against zero dimensions
        if content_height <= 0.0 || content_width <= 0.0 {
            return;
        }

        // Compute terminal dimensions (convert f32 content dimensions to f64 for font_metrics)
        let rows = (content_height as f64 / self.font_metrics.line_height).floor() as usize;
        let cols = (content_width as f64 / self.font_metrics.advance_width).floor() as usize;

        // Guard against zero-dimension terminal
        if rows == 0 || cols == 0 {
            return;
        }

        // Create terminal buffer with 5000 scrollback lines
        let mut terminal = TerminalBuffer::new(cols, rows, 5000);

        // Get working directory from workspace's root_path or current directory
        let cwd = self
            .editor
            .active_workspace()
            .map(|ws| ws.root_path.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        // Chunk: docs/chunks/terminal_pty_wakeup - Spawn shell with wakeup if available
        // Spawn login shell with wakeup support if a factory is registered (enables
        // low-latency PTY output rendering). Falls back to non-wakeup spawn if not
        // available. The shell is determined from the passwd database and spawned
        // as a login shell to get the user's full environment.
        let spawn_result = if let Some(wakeup) = self.create_pty_wakeup() {
            terminal.spawn_shell_with_wakeup(&cwd, wakeup)
        } else {
            terminal.spawn_shell(&cwd)
        };

        // Log error but don't fail
        if let Err(e) = spawn_result {
            eprintln!("Failed to spawn shell: {}", e);
        }

        // Generate label based on existing terminal count
        let existing_count = self.terminal_tab_count();
        let label = if existing_count == 0 {
            "Terminal".to_string()
        } else {
            format!("Terminal {}", existing_count + 1)
        };

        // Create and add the terminal tab
        let tab_id = self.editor.gen_tab_id();
        let line_height = self.editor.line_height();
        let new_tab = Tab::new_terminal(tab_id, terminal, label, line_height);

        if let Some(workspace) = self.editor.active_workspace_mut() {
            workspace.add_tab(new_tab);
        }

        // Chunk: docs/chunks/terminal_viewport_init - Initialize terminal viewport dimensions
        // Initialize the new terminal tab's viewport so scroll_to_bottom computes correct
        // offsets. Without this, visible_rows=0 causes scroll_to_bottom to scroll past
        // all content, producing a blank screen until a window resize.
        if let Some(workspace) = self.editor.active_workspace_mut() {
            if let Some(tab) = workspace.active_tab_mut() {
                let line_count = tab.buffer().line_count();
                tab.viewport.update_size(content_height, line_count);
            }
        }

        // Sync viewport to ensure dirty region calculations work correctly
        // (This is a no-op for terminal tabs but kept for consistency)
        self.sync_active_tab_viewport();

        // Chunk: docs/chunks/terminal_pane_initial_sizing - Sync viewports after terminal creation
        // Ensure the terminal's PTY is correctly sized for its pane. This is especially important
        // in split layouts where the pane is smaller than the window content area. This call
        // iterates all panes and syncs terminal sizes to match their actual pane geometry.
        self.sync_pane_viewports();

        // Ensure the new tab is visible in the tab bar
        self.ensure_active_tab_visible();
        self.invalidation.merge(InvalidationKind::Layout);
    }

    /// Scrolls the tab bar horizontally.
    ///
    /// Positive delta scrolls right (reveals more tabs to the right),
    /// negative delta scrolls left (reveals more tabs to the left).
    // Chunk: docs/chunks/content_tab_bar - Horizontal tab bar scroll and auto-scroll to active tab
    // Chunk: docs/chunks/tiling_workspace_integration - Use pane's tab_bar_view_offset
    pub fn scroll_tab_bar(&mut self, delta: f32) {
        if let Some(workspace) = self.editor.active_workspace_mut() {
            let current_offset = workspace.tab_bar_view_offset();
            let new_offset = (current_offset + delta).max(0.0);
            workspace.set_tab_bar_view_offset(new_offset);
            self.invalidation.merge(InvalidationKind::Layout);
        }
    }

    /// Ensures the active tab is visible in the tab bar.
    ///
    /// If the active tab is scrolled out of view, adjusts the scroll offset
    /// to bring it into view.
    // Chunk: docs/chunks/tiling_workspace_integration - Use pane's tab_bar_view_offset
    pub fn ensure_active_tab_visible(&mut self) {
        if let Some(workspace) = self.editor.active_workspace() {
            let tabs = tabs_from_workspace(workspace);
            let glyph_width = self.font_metrics.advance_width as f32;
            let tab_bar_offset = workspace.tab_bar_view_offset();
            let active_tab_index = workspace.active_tab_index();
            let geometry = calculate_tab_bar_geometry(
                self.view_width,
                &tabs,
                glyph_width,
                tab_bar_offset,
            );

            // Check if active tab is visible
            if let Some(active_rect) = geometry.tab_rects.get(active_tab_index) {
                let visible_start = RAIL_WIDTH;
                let visible_end = self.view_width;

                // If tab is to the left of visible area, scroll left
                if active_rect.x < visible_start {
                    let scroll_amount = visible_start - active_rect.x;
                    if let Some(workspace) = self.editor.active_workspace_mut() {
                        let new_offset = (workspace.tab_bar_view_offset() - scroll_amount).max(0.0);
                        workspace.set_tab_bar_view_offset(new_offset);
                    }
                }

                // If tab is to the right of visible area, scroll right
                let tab_right = active_rect.x + active_rect.width;
                if tab_right > visible_end {
                    let scroll_amount = tab_right - visible_end;
                    if let Some(workspace) = self.editor.active_workspace_mut() {
                        let new_offset = workspace.tab_bar_view_offset() + scroll_amount;
                        workspace.set_tab_bar_view_offset(new_offset);
                    }
                }
            }
        }
    }

    /// Handles a mouse click in the tab bar region.
    ///
    // Chunk: docs/chunks/content_tab_bar - Click-to-switch and close-button hit testing
    // Chunk: docs/chunks/tab_bar_interaction - Tab click coordinate transformation
    // Chunk: docs/chunks/tiling_workspace_integration - Receives screen-space coordinates (y=0 at top)
    // Chunk: docs/chunks/split_tab_click - Multi-pane tab bar click routing
    /// Determines which tab was clicked and switches to it, or handles
    /// close button clicks.
    ///
    /// In multi-pane layouts, each pane has its own tab bar at its top edge.
    /// This function determines which pane's tab bar was clicked, switches
    /// focus to that pane if necessary, and then activates the clicked tab.
    ///
    /// The mouse coordinates are in screen space (y=0 at top of window).
    // Chunk: docs/chunks/content_tab_bar - Click-to-switch and close-button hit testing
    fn handle_tab_bar_click(&mut self, screen_x: f32, screen_y: f32) {
        use crate::pane_layout::calculate_pane_rects;

        // Find which pane's tab bar was clicked and get the tab information
        let click_result = {
            let workspace = match self.editor.active_workspace() {
                Some(ws) => ws,
                None => return,
            };

            // Calculate pane rects in renderer space (starting at RAIL_WIDTH, 0)
            // This matches how the renderer calculates pane positions
            let bounds = (
                RAIL_WIDTH,
                0.0,
                self.view_width - RAIL_WIDTH,
                self.view_height,
            );
            let pane_rects = calculate_pane_rects(bounds, &workspace.pane_root);

            let glyph_width = self.font_metrics.advance_width as f32;

            // Find which pane's tab bar was clicked
            let mut result: Option<(PaneId, usize, bool)> = None; // (pane_id, tab_index, is_close_button)

            for pane_rect in &pane_rects {
                // Each pane's tab bar is at y ∈ [pane_rect.y, pane_rect.y + TAB_BAR_HEIGHT)
                let tab_bar_y_start = pane_rect.y;
                let tab_bar_y_end = pane_rect.y + TAB_BAR_HEIGHT;

                // Check if the click is within this pane's tab bar region
                if screen_x >= pane_rect.x
                    && screen_x < pane_rect.x + pane_rect.width
                    && screen_y >= tab_bar_y_start
                    && screen_y < tab_bar_y_end
                {
                    // Found the pane - get its tabs and calculate geometry
                    if let Some(pane) = workspace.pane_root.get_pane(pane_rect.pane_id) {
                        let tabs = tabs_from_pane(pane);
                        let geometry = calculate_pane_tab_bar_geometry(
                            pane_rect.x,
                            pane_rect.y,
                            pane_rect.width,
                            &tabs,
                            glyph_width,
                            pane.tab_bar_view_offset,
                        );

                        // Check each tab rect
                        for tab_rect in &geometry.tab_rects {
                            if tab_rect.contains(screen_x, screen_y) {
                                let is_close = tab_rect.is_close_button(screen_x, screen_y);
                                result = Some((pane_rect.pane_id, tab_rect.tab_index, is_close));
                                break;
                            }
                        }
                    }
                    break;
                }
            }

            result
        };

        // Apply the click result (mutable operations)
        if let Some((pane_id, tab_index, is_close_button)) = click_result {
            // Switch focus to the clicked pane if it's not already active
            let current_pane_id = self
                .editor
                .active_workspace()
                .map(|ws| ws.active_pane_id)
                .unwrap_or(0);

            if pane_id != current_pane_id {
                if let Some(ws) = self.editor.active_workspace_mut() {
                    ws.active_pane_id = pane_id;
                }
                self.invalidation.merge(InvalidationKind::Layout);
            }

            // Now handle the tab click (close or switch)
            if is_close_button {
                self.close_tab(tab_index);
            } else {
                self.switch_tab(tab_index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dir_picker;
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

    /// Regression test: cursor blink stall bug.
    /// When viewport has never been sized (visible_lines == 0), toggle_cursor_blink()
    /// must still return a dirty region that triggers repaint. Without this fix,
    /// cursor_dirty_region() would return None (via dirty_lines_to_region_wrapped's
    /// boundary check bug), causing the cursor to freeze.
    // Chunk: docs/chunks/cursor_blink_stall - Regression test for cursor blink stall
    #[test]
    fn test_toggle_cursor_blink_uninitialized_viewport_returns_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        // Deliberately do NOT call update_viewport_size() - viewport has visible_lines == 0
        assert_eq!(
            state.viewport().visible_lines(),
            0,
            "Test precondition: viewport should have 0 visible lines"
        );

        // Set last_keystroke to the past so blink toggle actually toggles
        state.last_keystroke = Instant::now() - Duration::from_secs(1);

        // Toggle cursor blink should return FullViewport, not None
        let dirty = state.toggle_cursor_blink();
        assert!(
            dirty.is_dirty(),
            "Cursor blink should return dirty region even with uninitialized viewport"
        );
        assert_eq!(
            dirty,
            DirtyRegion::FullViewport,
            "Uninitialized viewport should return FullViewport"
        );
    }

    #[test]
    fn test_viewport_size_update() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // visible_lines is computed from content_height = window_height - TAB_BAR_HEIGHT
        // With TAB_BAR_HEIGHT=32, content_height = 320 - 32 = 288
        // visible_lines = 288 / 16 = 18
        let expected_visible = ((320.0 - TAB_BAR_HEIGHT) / 16.0).floor() as usize;
        assert_eq!(state.viewport().visible_lines(), expected_visible);
        // view_height remains the full window height for coordinate flipping
        assert_eq!(state.view_height, 320.0);
    }

    /// Regression test: visible_lines must be computed from content area height,
    /// not full window height. The tab bar occupies TAB_BAR_HEIGHT pixels at the
    /// top, so the usable text area is (window_height - TAB_BAR_HEIGHT).
    ///
    /// Bug: When this calculation was wrong, the user couldn't scroll far enough
    /// to fully reveal the last line of the buffer.
    // Chunk: docs/chunks/scroll_max_last_line - Regression test for content_height fix
    #[test]
    fn test_visible_lines_accounts_for_tab_bar() {
        let mut state = EditorState::empty(test_font_metrics());
        // line_height = 16.0, TAB_BAR_HEIGHT = 32.0
        // window_height = 192 => content_height = 192 - 32 = 160
        // visible_lines = 160 / 16 = 10
        state.update_viewport_dimensions(800.0, 192.0);

        assert_eq!(
            state.viewport().visible_lines(),
            10,
            "visible_lines should be computed from content_height (192 - 32 = 160), \
             not window_height (192). With line_height=16, that's 10 lines, not 12."
        );
        // view_height must remain the full window height for mouse coordinate flipping
        assert_eq!(state.view_height, 192.0);
        assert_eq!(state.view_width, 800.0);
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

    // ======================================================================
    // Cmd+O System File Picker Tests (Chunk: docs/chunks/file_open_picker)
    // ======================================================================

    #[test]
    fn test_cmd_o_opens_file_into_active_tab() {
        use std::io::Write;
        use crate::file_picker;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file with content
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_cmd_o_file.txt");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            f.write_all(b"Hello from Cmd+O!\nSecond line\n").unwrap();
        }

        // Mock the file picker to return the temp file
        file_picker::mock_set_next_file(Some(temp_file.clone()));

        // Press Cmd+O
        let cmd_o = KeyEvent::new(
            Key::Char('o'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_o);

        // Buffer should contain the file content
        assert_eq!(state.buffer().content(), "Hello from Cmd+O!\nSecond line\n");

        // Associated file should be set
        assert_eq!(state.associated_file(), Some(&temp_file));

        // Cleanup
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_cmd_o_cancelled_picker_leaves_tab_unchanged() {
        use crate::file_picker;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some content into the buffer first
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        state.handle_key(KeyEvent::char('c'));
        let original_content = state.buffer().content().to_string();

        // Mock the file picker to return None (user cancelled)
        file_picker::mock_set_next_file(None);

        // Press Cmd+O
        let cmd_o = KeyEvent::new(
            Key::Char('o'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_o);

        // Buffer should be unchanged
        assert_eq!(state.buffer().content(), original_content);

        // No file should be associated (still None from initial state)
        assert!(state.associated_file().is_none());
    }

    #[test]
    fn test_cmd_o_no_op_on_terminal_tab() {
        use crate::file_picker;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab (making it the active tab)
        state.new_terminal_tab();

        // Verify we're on a terminal tab
        assert!(!state.active_tab_is_file());

        // Mock the file picker to return a path
        let temp_path = std::env::temp_dir().join("should_not_load.txt");
        file_picker::mock_set_next_file(Some(temp_path));

        // Press Cmd+O
        let cmd_o = KeyEvent::new(
            Key::Char('o'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_o);

        // The mock file picker should NOT have been called (early return)
        // We can't directly verify this, but we can verify nothing changed
        // and no panic occurred (terminal tabs don't have a buffer to load into)
        assert!(!state.active_tab_is_file());
    }

    #[test]
    fn test_cmd_o_does_not_insert_character() {
        use crate::file_picker;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Mock the file picker to return None (user cancels)
        file_picker::mock_set_next_file(None);

        let cmd_o = KeyEvent::new(
            Key::Char('o'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_o);

        // Buffer should remain empty - 'o' should not be inserted
        assert!(state.buffer().is_empty());
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
        let first_visible = state.active_selector.as_ref().unwrap().first_visible_item();
        assert!(first_visible > 0, "Selector should have scrolled");
    }

    #[test]
    fn test_scroll_when_selector_open_updates_first_visible_item() {
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

        // Initial first_visible_item should be 0
        assert_eq!(state.active_selector.as_ref().unwrap().first_visible_item(), 0);

        // Scroll down (positive delta = scroll down)
        // line_height is 16.0, so 48 pixels = 3 rows
        state.handle_scroll(ScrollDelta::new(0.0, 48.0));

        // first_visible_item should have increased
        let first_visible = state.active_selector.as_ref().unwrap().first_visible_item();
        assert_eq!(first_visible, 3);
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

        // Set up mock directory picker to return a test directory
        // Chunk: docs/chunks/workspace_dir_picker - Mock directory picker in tests
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/workspace")));

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
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
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
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
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
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
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

        // Create a second workspace named "my_project"
        // Chunk: docs/chunks/workspace_dir_picker - Workspace label is derived from directory name
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/my_project")));
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 2);

        // Now title should include workspace label (derived from directory name)
        let title = state.window_title();
        assert!(title.contains("—")); // em-dash separator
        assert!(title.contains("my_project"), "Title should contain workspace label from directory name, got: {}", title);
    }

    // =========================================================================
    // Workspace Switching Tests (Chunk: docs/chunks/workspace_switching)
    // =========================================================================

    #[test]
    fn test_left_rail_click_switches_workspace_with_y_flip() {
        use crate::left_rail::{calculate_left_rail_geometry, RAIL_WIDTH, TILE_HEIGHT};
        let mut state = EditorState::empty(test_font_metrics());

        // Set up view dimensions - use a realistic window height
        let view_height: f32 = 600.0;
        state.view_height = view_height;
        state.view_width = 800.0;

        // Create a second workspace so we have 2 total
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 2);
        // Switch back to workspace 0
        state.switch_workspace(0);
        assert_eq!(state.editor.active_workspace, 0);

        let _ = state.take_dirty_region();

        // Calculate geometry to find the y-position of workspace 1's tile
        // In top-down screen coords: workspace 0 is at y=TOP_MARGIN (8.0)
        //                            workspace 1 is at y=TOP_MARGIN+TILE_HEIGHT+TILE_SPACING (60.0)
        let geom = calculate_left_rail_geometry(view_height, 2);
        let tile_1_y_top_down = geom.tile_rects[1].y; // Should be ~60.0
        let tile_1_y_center = tile_1_y_top_down + TILE_HEIGHT / 2.0;

        // Convert to NSView coordinates (y=0 at bottom)
        // NSView y = view_height - screen_y
        let nsview_y = view_height - tile_1_y_center;

        // Create a click event at the center of workspace 1 tile
        let click_x = (RAIL_WIDTH / 2.0) as f64;
        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y as f64),
            modifiers: Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Should have switched to workspace 1
        assert_eq!(
            state.editor.active_workspace, 1,
            "Clicking on workspace 1 tile (NSView y={}, flipped to top-down y={}) should switch to workspace 1",
            nsview_y, tile_1_y_center
        );
        assert!(state.is_dirty());
    }

    #[test]
    fn test_next_workspace_cycles_forward() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create 3 workspaces total
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
        state.new_workspace();
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws3")));
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 3);

        // Switch to workspace 0
        state.switch_workspace(0);
        assert_eq!(state.editor.active_workspace, 0);

        // Cycle forward: 0 -> 1 -> 2 -> 0
        state.next_workspace();
        assert_eq!(state.editor.active_workspace, 1);

        state.next_workspace();
        assert_eq!(state.editor.active_workspace, 2);

        state.next_workspace();
        assert_eq!(state.editor.active_workspace, 0); // Wraps around
    }

    #[test]
    fn test_prev_workspace_cycles_backward() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create 3 workspaces total
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
        state.new_workspace();
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws3")));
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 3);

        // Switch to workspace 2
        state.switch_workspace(2);
        assert_eq!(state.editor.active_workspace, 2);

        // Cycle backward: 2 -> 1 -> 0 -> 2
        state.prev_workspace();
        assert_eq!(state.editor.active_workspace, 1);

        state.prev_workspace();
        assert_eq!(state.editor.active_workspace, 0);

        state.prev_workspace();
        assert_eq!(state.editor.active_workspace, 2); // Wraps around
    }

    #[test]
    fn test_next_workspace_single_workspace_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.workspace_count(), 1);
        assert_eq!(state.editor.active_workspace, 0);

        state.next_workspace();
        assert_eq!(state.editor.active_workspace, 0);
    }

    #[test]
    fn test_prev_workspace_single_workspace_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.workspace_count(), 1);
        assert_eq!(state.editor.active_workspace, 0);

        state.prev_workspace();
        assert_eq!(state.editor.active_workspace, 0);
    }

    #[test]
    fn test_cmd_right_bracket_next_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create second workspace
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
        state.new_workspace();
        state.switch_workspace(0);
        assert_eq!(state.editor.active_workspace, 0);

        let _ = state.take_dirty_region();

        // Cmd+] (without Shift) cycles to next workspace
        let cmd_bracket = KeyEvent::new(
            Key::Char(']'),
            Modifiers {
                command: true,
                shift: false,
                ..Default::default()
            },
        );
        state.handle_key(cmd_bracket);

        assert_eq!(state.editor.active_workspace, 1);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_cmd_left_bracket_prev_workspace() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create second workspace
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/ws2")));
        state.new_workspace();
        assert_eq!(state.editor.active_workspace, 1);

        let _ = state.take_dirty_region();

        // Cmd+[ (without Shift) cycles to previous workspace
        let cmd_bracket = KeyEvent::new(
            Key::Char('['),
            Modifiers {
                command: true,
                shift: false,
                ..Default::default()
            },
        );
        state.handle_key(cmd_bracket);

        assert_eq!(state.editor.active_workspace, 0);
        assert!(state.is_dirty());
    }

    // =========================================================================
    // Workspace Directory Picker Tests (Chunk: docs/chunks/workspace_dir_picker)
    // =========================================================================

    #[test]
    fn test_new_workspace_with_cancelled_picker_does_nothing() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.workspace_count(), 1);
        let _ = state.take_dirty_region();

        // Mock returns None (user cancelled)
        dir_picker::mock_set_next_directory(None);
        state.new_workspace();

        // Should still have only 1 workspace
        assert_eq!(state.editor.workspace_count(), 1);
        // Should not be dirty (no changes made)
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_new_workspace_with_selection_creates_workspace() {
        use crate::workspace::TabKind;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);
        state.update_viewport_dimensions(800.0, 600.0); // Need dimensions for terminal sizing

        assert_eq!(state.editor.workspace_count(), 1);

        // Mock returns a directory
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test/project")));
        state.new_workspace();

        // Should now have 2 workspaces
        assert_eq!(state.editor.workspace_count(), 2);
        // Should be switched to the new workspace
        assert_eq!(state.editor.active_workspace, 1);
        // Should be dirty
        assert!(state.is_dirty());

        // Chunk: docs/chunks/workspace_initial_terminal - Second workspace gets terminal tab
        // The new workspace should have a terminal tab, not an empty file tab
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.tab_count(), 1);
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.kind, TabKind::Terminal);
        assert_eq!(tab.label, "Terminal");
    }

    #[test]
    fn test_new_workspace_label_from_directory_name() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Mock returns a directory with a specific name
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/home/user/my_project")));
        state.new_workspace();

        // The workspace label should be derived from the directory name
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.label, "my_project");
        assert_eq!(workspace.root_path, PathBuf::from("/home/user/my_project"));
    }

    #[test]
    fn test_new_workspace_root_path_is_selected_directory() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        dir_picker::mock_set_next_directory(Some(PathBuf::from("/specific/path")));
        state.new_workspace();

        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.root_path, PathBuf::from("/specific/path"));
    }

    // =========================================================================
    // Workspace Initial Terminal Tests (Chunk: docs/chunks/workspace_initial_terminal)
    // =========================================================================

    #[test]
    fn test_startup_workspace_has_empty_file_tab() {
        use crate::workspace::TabKind;

        let mut state = EditorState::new_deferred(test_font_metrics());

        // Simulate startup workspace creation (first workspace of session)
        // Must be done before update_viewport_size since that requires an active workspace
        state.add_startup_workspace(PathBuf::from("/startup/project"));

        state.update_viewport_size(160.0);
        state.update_viewport_dimensions(800.0, 600.0);

        // Should have exactly 1 workspace
        assert_eq!(state.editor.workspace_count(), 1);

        // The startup workspace should have exactly 1 tab
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.tab_count(), 1);

        // The tab should be a File type (for welcome screen)
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.kind, TabKind::File);

        // The buffer should be empty (welcome screen state)
        // An empty file buffer has 1 line with length 0
        assert_eq!(tab.buffer().line_count(), 1);
        assert_eq!(tab.buffer().line_len(0), 0);
    }

    #[test]
    fn test_second_workspace_has_terminal_tab() {
        use crate::workspace::TabKind;

        let mut state = EditorState::new_deferred(test_font_metrics());

        // Create startup workspace first (must be done before viewport updates)
        state.add_startup_workspace(PathBuf::from("/startup/project"));
        assert_eq!(state.editor.workspace_count(), 1);

        state.update_viewport_size(160.0);
        state.update_viewport_dimensions(800.0, 600.0);

        // Create a second workspace via directory picker
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/second/project")));
        state.new_workspace();

        // Should now have 2 workspaces
        assert_eq!(state.editor.workspace_count(), 2);

        // Should be switched to the new workspace
        assert_eq!(state.editor.active_workspace, 1);

        // The new workspace should have exactly 1 tab
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.tab_count(), 1);

        // The tab should be a Terminal type
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.kind, TabKind::Terminal);

        // The terminal tab label should be "Terminal"
        assert_eq!(tab.label, "Terminal");
    }

    #[test]
    fn test_second_workspace_terminal_uses_workspace_root_path() {
        use crate::workspace::TabKind;

        let mut state = EditorState::new_deferred(test_font_metrics());

        // Create startup workspace first (must be done before viewport updates)
        state.add_startup_workspace(PathBuf::from("/startup/project"));

        state.update_viewport_size(160.0);
        state.update_viewport_dimensions(800.0, 600.0);

        // Create a second workspace with a specific root_path
        let expected_root = PathBuf::from("/specific/root/path");
        dir_picker::mock_set_next_directory(Some(expected_root.clone()));
        state.new_workspace();

        // The workspace should have the expected root_path
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.root_path, expected_root);

        // The terminal should have been spawned in this directory
        // (new_terminal_tab() uses workspace's root_path as cwd)
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.kind, TabKind::Terminal);
    }

    #[test]
    fn test_file_picker_queries_active_workspace_index() {
        use tempfile::TempDir;
        use std::fs::File;

        // Create a temp directory with a test file
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        File::create(root.join("test_file.txt")).unwrap();

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);
        state.update_viewport_dimensions(800.0, 600.0);

        // Create a workspace with our temp directory
        dir_picker::mock_set_next_directory(Some(root.to_path_buf()));
        state.new_workspace();

        // Wait for indexing to complete
        while state.editor.active_workspace().unwrap().file_index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Clear dirty region from workspace creation
        let _ = state.take_dirty_region();

        // Open file picker (Cmd+P)
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Verify selector is active
        assert_eq!(state.focus, EditorFocus::Selector);
        assert!(state.active_selector.is_some());

        // Verify the selector contains our test file
        let selector = state.active_selector.as_ref().unwrap();
        let items = selector.items();
        assert!(items.iter().any(|item| item.contains("test_file.txt")),
            "File picker should contain test_file.txt from workspace's file index");
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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);

        // Switch to first tab
        state.switch_tab(0);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_switch_tab_invalid_index_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only 1 tab exists
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

        // Try to switch to invalid index
        let _ = state.take_dirty_region();
        state.switch_tab(5);

        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);

        // Close the first tab
        state.close_tab(0);

        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert!(state.is_dirty());
    }

    #[test]
    fn test_close_last_tab_creates_new_empty_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only 1 tab exists
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);

        // Close the only tab - should create a new empty one
        state.close_tab(0);

        // Should still have 1 tab (new empty one)
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 3);

        // Switch to first tab
        state.switch_tab(0);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

        // Next tab
        state.next_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);

        state.next_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 2);

        // Wrap around
        state.next_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 3);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 2);

        // Previous tab
        state.prev_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);

        state.prev_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

        // Wrap around
        state.prev_tab();
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 2);
    }

    #[test]
    fn test_next_tab_single_tab_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Only 1 tab
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

        let _ = state.take_dirty_region();
        state.next_tab();

        // Should remain unchanged
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);

        // Cmd+W closes the active tab
        let cmd_w = KeyEvent::new(
            Key::Char('w'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_w);

        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
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
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

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

        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);
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
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);

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

        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);

        // Close active tab
        state.close_active_tab();

        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
    }

    // =========================================================================
    // Cmd+T New Tab Tests (Chunk: docs/chunks/content_tab_bar)
    // =========================================================================

    #[test]
    fn test_cmd_t_creates_new_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Initially one tab
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

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
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);
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

    // =========================================================================
    // Cmd+Shift+T Terminal Tab Tests (Chunk: docs/chunks/terminal_tab_spawn)
    // =========================================================================

    // Chunk: docs/chunks/tiling_workspace_integration - Use pane API
    #[test]
    fn test_cmd_shift_t_creates_terminal_tab() {
        use crate::workspace::TabKind;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // Set viewport size large enough to create a valid terminal
        // Window height = TAB_BAR_HEIGHT + content area (enough for at least 1 row)
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Initially one tab (the empty file tab)
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.tab_count(), 1);

        // Cmd+Shift+T should create a new terminal tab
        let cmd_shift_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_t);

        // Should now have 2 tabs
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.tab_count(), 2);

        // The active tab should be the new terminal tab (index 1)
        assert_eq!(workspace.active_tab_index(), 1);

        // The new tab should be a Terminal type
        let active_tab = workspace.active_tab().unwrap();
        assert_eq!(active_tab.kind, TabKind::Terminal);

        // The tab label should be "Terminal"
        assert_eq!(active_tab.label, "Terminal");
    }

    #[test]
    fn test_cmd_shift_t_multiple_terminals_numbered() {
        use crate::workspace::TabKind;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Initially one tab
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);

        let cmd_shift_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );

        // Press Cmd+Shift+T twice
        state.handle_key(cmd_shift_t.clone());
        state.handle_key(cmd_shift_t);

        // Should have 3 tabs (1 file + 2 terminals)
        let workspace = state.editor.active_workspace().unwrap();
        assert_eq!(workspace.tab_count(), 3);

        // Find the terminal tabs and check their labels
        let terminal_tabs: Vec<_> = workspace
            .tabs()
            .iter()
            .filter(|t| t.kind == TabKind::Terminal)
            .collect();

        assert_eq!(terminal_tabs.len(), 2);

        // First terminal should be "Terminal", second should be "Terminal 2"
        let labels: Vec<&str> = terminal_tabs.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"Terminal"), "Expected 'Terminal' label, got {:?}", labels);
        assert!(labels.contains(&"Terminal 2"), "Expected 'Terminal 2' label, got {:?}", labels);
    }

    #[test]
    fn test_cmd_shift_t_does_not_insert_t() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Cmd+Shift+T should NOT insert 'T' into the buffer
        let cmd_shift_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_t);

        // The original file tab's buffer should still be empty
        // (Note: active tab is now the terminal, so we need to check the first tab)
        let workspace = state.editor.active_workspace().unwrap();
        let file_tab = &workspace.tabs()[0];
        let buffer = file_tab.as_text_buffer().unwrap();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_new_tab_method() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);

        state.new_tab();

        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);
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

    // =========================================================================
    // Chunk: docs/chunks/find_strip_scroll_clearance - Viewport dimensions tests
    // =========================================================================

    #[test]
    fn test_update_viewport_dimensions_subtracts_tab_bar_height() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // line_height is 16.0 in test_font_metrics()

        // With window_height = 600.0 and TAB_BAR_HEIGHT = 32.0,
        // content_height = 600 - 32 = 568
        // visible_lines = floor(568 / 16) = 35
        state.update_viewport_dimensions(800.0, 600.0);

        let expected_content_height = 600.0 - TAB_BAR_HEIGHT;
        let expected_visible_lines = (expected_content_height / 16.0).floor() as usize;

        assert_eq!(
            state.viewport().visible_lines(),
            expected_visible_lines,
            "update_viewport_dimensions should pass content_height (window_height - TAB_BAR_HEIGHT) to viewport, \
             not the full window_height. Expected {} visible lines but got {}.",
            expected_visible_lines,
            state.viewport().visible_lines()
        );
    }

    #[test]
    fn test_update_viewport_size_subtracts_tab_bar_height() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // line_height is 16.0 in test_font_metrics()

        // With window_height = 600.0 and TAB_BAR_HEIGHT = 32.0,
        // content_height = 600 - 32 = 568
        // visible_lines = floor(568 / 16) = 35
        state.update_viewport_size(600.0);

        let expected_content_height = 600.0 - TAB_BAR_HEIGHT;
        let expected_visible_lines = (expected_content_height / 16.0).floor() as usize;

        assert_eq!(
            state.viewport().visible_lines(),
            expected_visible_lines,
            "update_viewport_size should pass content_height (window_height - TAB_BAR_HEIGHT) to viewport, \
             not the full window_height. Expected {} visible lines but got {}.",
            expected_visible_lines,
            state.viewport().visible_lines()
        );
    }

    #[test]
    fn test_find_scroll_clearance() {
        // This test verifies that when find mode is active and scrolling is needed
        // to reveal a match, the match lands at or above the second-to-last visible row
        // (i.e., above the find strip area).

        let mut state = EditorState::empty(test_font_metrics());

        // Create a buffer with 100 lines, each containing a unique identifier
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("line{:03}\n", i));
        }
        state.buffer_mut().insert_str(&content);

        // Set up viewport with window_height = 192 px
        // content_height = 192 - 32 (TAB_BAR_HEIGHT) = 160 px
        // visible_lines = 160 / 16 = 10 lines
        state.update_viewport_size(192.0);
        let visible_lines = state.viewport().visible_lines();
        assert_eq!(visible_lines, 10, "Sanity check: expected 10 visible lines");

        // Start at the top of the buffer
        state.buffer_mut().set_cursor(lite_edit_buffer::Position { line: 0, col: 0 });
        let line_count = state.buffer().line_count();
        state.viewport_mut().scroll_to(0, line_count);

        // Open find mode (Cmd+F)
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);
        assert_eq!(state.focus, EditorFocus::FindInFile);

        // Type a search query that matches a line near the bottom of what would scroll
        // Search for "line025" which is at line 25 (0-indexed)
        for c in "line025".chars() {
            state.handle_key(KeyEvent::char(c));
        }

        // After searching, the match should be scrolled into view
        let first_visible = state.viewport().first_visible_line();
        let match_line = 25_usize;

        // The match line should be within the effective visible area.
        // With find strip margin=1, match should be at position <= visible_lines - 2
        // (i.e., at row 8 or earlier, since visible_lines = 10)
        let match_screen_position = match_line.saturating_sub(first_visible);

        assert!(
            match_screen_position <= visible_lines.saturating_sub(2),
            "When find mode is active, matches should be scrolled to land above the find strip. \
             Match at line {} is at screen position {} (first_visible = {}, visible_lines = {}). \
             Expected screen position <= {} (visible_lines - 2).",
            match_line,
            match_screen_position,
            first_visible,
            visible_lines,
            visible_lines.saturating_sub(2)
        );
    }

    // =========================================================================
    // Tab Bar Click Tests (Chunk: docs/chunks/tab_bar_interaction)
    // =========================================================================

    #[test]
    // Chunk: docs/chunks/tiling_workspace_integration - Tests use screen-space coordinates (y=0 at top)
    fn test_click_tab_switches_to_that_tab() {
        use crate::left_rail::RAIL_WIDTH;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // Set view width larger to accommodate tabs
        state.view_width = 800.0;
        state.view_height = 320.0;
        state.update_viewport_size(320.0);

        // Add a second tab
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }

        // Should have 2 tabs, active_tab is 1 (switched to new tab on add)
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 1);

        // Clear dirty state
        let _ = state.take_dirty_region();

        // Click on the first tab (tab index 0)
        // In NSView coords (origin at bottom-left), we send the click position.
        // handle_mouse will flip to screen space.
        // Tab bar in NSView coords: y ∈ [view_height - TAB_BAR_HEIGHT, view_height)
        // So clicking at y = view_height - TAB_BAR_HEIGHT/2 is in the tab bar
        let nsview_tab_bar_y = (320.0 - TAB_BAR_HEIGHT / 2.0) as f64;
        // First tab starts at RAIL_WIDTH
        let first_tab_x = (RAIL_WIDTH + 20.0) as f64;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (first_tab_x, nsview_tab_bar_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        // Should have switched to tab 0
        assert_eq!(
            state.editor.active_workspace().unwrap().active_tab_index(),
            0,
            "Clicking on first tab should switch to tab 0"
        );
        assert!(state.is_dirty(), "Switching tabs should mark dirty");
    }

    #[test]
    fn test_click_active_tab_is_noop() {
        use crate::left_rail::RAIL_WIDTH;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.view_width = 800.0;
        state.view_height = 320.0;
        state.update_viewport_size(320.0);

        // Only 1 tab exists, and it's active
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);

        // Clear dirty state
        let _ = state.take_dirty_region();

        // Click on the active tab - should be a no-op (no state change)
        let tab_bar_y = (320.0 - TAB_BAR_HEIGHT / 2.0) as f64;
        let first_tab_x = (RAIL_WIDTH + 20.0) as f64;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (first_tab_x, tab_bar_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        // Should still be on tab 0
        assert_eq!(state.editor.active_workspace().unwrap().active_tab_index(), 0);
        // Switching to the same tab shouldn't mark dirty
        assert!(!state.is_dirty(), "Clicking active tab should not mark dirty");
    }

    #[test]
    fn test_tab_geometry_matches_workspace_indices() {
        // Verify that the tab_index in TabRect matches the workspace tab indices
        use crate::tab_bar::{calculate_tab_bar_geometry, tabs_from_workspace};

        let mut state = EditorState::empty(test_font_metrics());
        state.view_width = 800.0;

        // Add multiple tabs
        for _ in 0..3 {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }

        let workspace = state.editor.active_workspace().unwrap();
        let tabs = tabs_from_workspace(workspace);
        let glyph_width = state.font_metrics.advance_width as f32;
        let geometry = calculate_tab_bar_geometry(
            state.view_width,
            &tabs,
            glyph_width,
            workspace.tab_bar_view_offset(),
        );

        // Each tab_rect.tab_index should match its position
        for (i, tab_rect) in geometry.tab_rects.iter().enumerate() {
            assert_eq!(
                tab_rect.tab_index, i,
                "TabRect {} should have tab_index {}, got {}",
                i, i, tab_rect.tab_index
            );
        }
    }

    // =========================================================================
    // Tab viewport sync regression tests
    // Chunk: docs/chunks/tab_click_cursor_placement - Viewport sync tests
    // =========================================================================

    /// Tests that new tabs created with Cmd+T have their viewport sized correctly.
    ///
    /// Bug: Before the fix, new tabs had visible_lines = 0, causing dirty region
    /// calculations to produce DirtyRegion::None for all mutations, preventing
    /// cursor repaints after mouse clicks.
    // Chunk: docs/chunks/tab_click_cursor_placement - Regression test verifying new tabs have correct visible_lines
    #[test]
    fn test_new_tab_viewport_is_sized() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // Set viewport size (simulating initial window setup)
        // update_viewport_size subtracts TAB_BAR_HEIGHT (32px) to get content_height.
        // To get 10 visible lines: content_height = 10 * 16 = 160px
        // window_height = content_height + TAB_BAR_HEIGHT = 160 + 32 = 192px
        let window_height = (10.0 * 16.0) + TAB_BAR_HEIGHT;
        state.update_viewport_size(window_height);

        // Verify first tab has correct viewport
        assert_eq!(
            state.viewport().visible_lines(),
            10,
            "First tab should have 10 visible lines"
        );

        // Create a new tab (simulates Cmd+T)
        state.new_tab();

        // The new tab should also have correctly sized viewport
        assert_eq!(
            state.viewport().visible_lines(),
            10,
            "New tab should have 10 visible lines (not 0)"
        );

        // Insert some text into the new buffer
        state.buffer_mut().insert_str("Line 1\nLine 2\nLine 3\nLine 4\nLine 5");

        // Clear the dirty region from insertion and new_tab
        let _ = state.take_dirty_region();

        // Simulate a cursor move that would mark the cursor dirty
        // In the real flow, this happens via handle_mouse_down
        // Here we directly use viewport to test dirty_lines_to_region
        let dirty_lines = lite_edit_buffer::DirtyLines::Single(2);
        let line_count = state.buffer().line_count();
        let dirty_region = state.viewport().dirty_lines_to_region(&dirty_lines, line_count);

        // The dirty region should NOT be None (the bug was that it was always None)
        assert!(
            dirty_region.is_dirty(),
            "Dirty region for line 2 should not be None; got {:?}",
            dirty_region
        );
    }

    /// Tests that switching tabs correctly syncs the viewport.
    ///
    /// Bug: Before the fix, switching to a tab that was created but never activated
    /// would leave visible_lines = 0, preventing cursor repaints.
    // Chunk: docs/chunks/tab_click_cursor_placement - Regression test verifying tab switching maintains correct visible_lines
    #[test]
    fn test_switch_tab_viewport_is_sized() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // update_viewport_size subtracts TAB_BAR_HEIGHT to get content_height.
        // To get 10 visible lines: window_height = (10 * 16) + TAB_BAR_HEIGHT = 192
        let window_height = (10.0 * 16.0) + TAB_BAR_HEIGHT;
        state.update_viewport_size(window_height);

        // Create a second tab
        state.new_tab();

        // Insert text in tab 1 (the new tab is now active)
        state.buffer_mut().insert_str("Tab 1 content");

        // Switch back to tab 0
        state.switch_tab(0);
        assert_eq!(
            state.viewport().visible_lines(),
            10,
            "Tab 0 should have correct visible_lines after switching"
        );

        // Switch to tab 1
        state.switch_tab(1);
        assert_eq!(
            state.viewport().visible_lines(),
            10,
            "Tab 1 should have correct visible_lines after switching back"
        );

        // Clear the dirty region from all the tab operations
        let _ = state.take_dirty_region();

        // Mark a line dirty and verify region is computed correctly
        let dirty_lines = lite_edit_buffer::DirtyLines::Single(0);
        let line_count = state.buffer().line_count();
        let dirty_region = state.viewport().dirty_lines_to_region(&dirty_lines, line_count);

        assert!(
            dirty_region.is_dirty(),
            "Dirty region for line 0 should not be None after tab switch"
        );
    }

    /// Tests that associating a file (file picker confirmation) syncs the viewport.
    ///
    /// Bug: Before the fix, Cmd+T followed by file picker confirmation would leave
    /// the new tab with visible_lines = 0.
    // Chunk: docs/chunks/tab_click_cursor_placement - Regression test verifying file picker flow maintains correct visible_lines
    #[test]
    fn test_associate_file_viewport_is_sized() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use std::io::Write;

        let mut state = EditorState::empty(test_font_metrics());
        // update_viewport_size subtracts TAB_BAR_HEIGHT to get content_height.
        // To get 10 visible lines: window_height = (10 * 16) + TAB_BAR_HEIGHT = 192
        let window_height = (10.0 * 16.0) + TAB_BAR_HEIGHT;
        state.update_viewport_size(window_height);

        // Create a temporary file with known content
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_associate_file_viewport.txt");
        {
            let mut file = std::fs::File::create(&temp_file).unwrap();
            writeln!(file, "Line 1").unwrap();
            writeln!(file, "Line 2").unwrap();
            writeln!(file, "Line 3").unwrap();
        }

        // Create a new tab (simulates Cmd+T)
        state.new_tab();

        // Associate file (simulates file picker confirmation)
        state.associate_file(temp_file.clone());

        // Viewport should be correctly sized
        assert_eq!(
            state.viewport().visible_lines(),
            10,
            "Viewport should have 10 visible lines after associate_file"
        );

        // Clear the dirty region
        let _ = state.take_dirty_region();

        // Verify dirty region calculation works
        let dirty_lines = lite_edit_buffer::DirtyLines::Single(1);
        let line_count = state.buffer().line_count();
        let dirty_region = state.viewport().dirty_lines_to_region(&dirty_lines, line_count);

        assert!(
            dirty_region.is_dirty(),
            "Dirty region should not be None after associate_file"
        );

        // Clean up
        let _ = std::fs::remove_file(temp_file);
    }

    /// Tests that the helper skips syncing when view_height is not set.
    ///
    /// This tests the early return in sync_active_tab_viewport for the initial
    /// state before the first window resize.
    // Chunk: docs/chunks/tab_click_cursor_placement - Edge case test for initial state before window resize
    #[test]
    fn test_sync_viewport_skips_when_no_view_height() {
        let mut state = EditorState::empty(test_font_metrics());
        // Don't call update_viewport_size - view_height is 0.0

        // Create a new tab - should not panic even with view_height = 0
        state.new_tab();

        // Viewport should remain at 0 visible lines (no sync happened)
        assert_eq!(
            state.viewport().visible_lines(),
            0,
            "Viewport should have 0 visible lines when view_height is not set"
        );
    }

    // =========================================================================
    // Terminal Tab Safety Tests (Chunk: docs/chunks/terminal_active_tab_safety)
    // =========================================================================

    /// Tests that key events on a terminal tab don't panic.
    ///
    /// This is a regression test for the crash that occurred when Cmd+Shift+T
    /// spawned a terminal tab and subsequent key events tried to access
    /// the TextBuffer via `buffer()`.
    #[test]
    fn test_terminal_tab_key_events_no_panic() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        let cmd_shift_t = KeyEvent::new(
            Key::Char('t'),
            Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_shift_t);

        // Verify we're on a terminal tab
        let workspace = state.editor.active_workspace().unwrap();
        let tab = workspace.active_tab().unwrap();
        assert!(tab.as_text_buffer().is_none(), "Active tab should be a terminal");

        // These should not panic
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        state.handle_key(KeyEvent::new(Key::Return, Modifiers::default()));
        state.handle_key(KeyEvent::new(Key::Backspace, Modifiers::default()));
        state.handle_key(KeyEvent::new(Key::Up, Modifiers::default()));
    }

    /// Tests that mouse events on a terminal tab don't panic.
    #[test]
    fn test_terminal_tab_mouse_events_no_panic() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Mouse clicks should not panic
        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (100.0, 100.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click_event);

        let drag_event = MouseEvent {
            kind: MouseEventKind::Moved,
            position: (150.0, 100.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(drag_event);

        let up_event = MouseEvent {
            kind: MouseEventKind::Up,
            position: (150.0, 100.0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(up_event);
    }

    /// Tests that scroll events on a terminal tab don't panic.
    #[test]
    fn test_terminal_tab_scroll_events_no_panic() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Scroll events should not panic
        state.handle_scroll(ScrollDelta::new(0.0, 50.0));
        state.handle_scroll(ScrollDelta::new(0.0, -50.0));
    }

    /// Tests that Cmd+F doesn't open find strip on terminal tabs.
    #[test]
    fn test_terminal_tab_cmd_f_no_find_strip() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();
        assert_eq!(state.focus, EditorFocus::Buffer);

        // Press Cmd+F
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Focus should still be Buffer, not FindInFile
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert!(state.find_mini_buffer.is_none());
    }

    /// Tests that cursor blink toggle doesn't panic on terminal tabs.
    #[test]
    fn test_terminal_tab_cursor_blink_no_panic() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Set last_keystroke to the past so blink toggle works
        state.last_keystroke = Instant::now() - Duration::from_secs(1);

        // These should not panic
        let dirty1 = state.toggle_cursor_blink();
        let dirty2 = state.toggle_cursor_blink();

        // Should return dirty regions (FullViewport for terminal tabs)
        assert!(dirty1.is_dirty());
        assert!(dirty2.is_dirty());
    }

    /// Tests that viewport size updates don't panic on terminal tabs.
    #[test]
    fn test_terminal_tab_viewport_update_no_panic() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // These should not panic
        state.update_viewport_size(700.0);
        state.update_viewport_dimensions(1000.0, 800.0);
    }

    /// Tests that switching between file and terminal tabs works correctly.
    #[test]
    fn test_switch_between_file_and_terminal_tabs() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Type in the file tab
        state.handle_key(KeyEvent::char('h'));
        state.handle_key(KeyEvent::char('i'));
        assert_eq!(state.buffer().content(), "hi");

        // Create a terminal tab (now active)
        state.new_terminal_tab();

        // Key events should not panic
        state.handle_key(KeyEvent::char('x'));

        // Switch back to file tab
        state.switch_tab(0);

        // Buffer should still have the same content
        assert_eq!(state.buffer().content(), "hi");

        // Typing should work again
        state.handle_key(KeyEvent::char('!'));
        assert_eq!(state.buffer().content(), "hi!");
    }

    /// Tests that active_tab_is_file correctly identifies tab types.
    // Chunk: docs/chunks/terminal_active_tab_safety - Tests active_tab_is_file method
    #[test]
    fn test_active_tab_is_file() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Initially on a file tab
        assert!(state.active_tab_is_file());

        // Create a terminal tab
        state.new_terminal_tab();

        // Now on a terminal tab
        assert!(!state.active_tab_is_file());

        // Switch back to file tab
        state.switch_tab(0);

        // Back on file tab
        assert!(state.active_tab_is_file());
    }

    /// Tests that try_buffer returns None for terminal tabs.
    // Chunk: docs/chunks/terminal_active_tab_safety - Tests try_buffer method on terminal tabs
    #[test]
    fn test_try_buffer_on_terminal_tab() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // File tab should return Some
        assert!(state.try_buffer().is_some());

        // Create a terminal tab
        state.new_terminal_tab();

        // Terminal tab should return None
        assert!(state.try_buffer().is_none());
        assert!(state.try_buffer_mut().is_none());
    }

    /// Tests that save_file doesn't panic on terminal tabs.
    // Chunk: docs/chunks/terminal_active_tab_safety - Tests save_file doesn't panic on terminal tabs
    #[test]
    fn test_terminal_tab_save_no_panic() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Cmd+S should not panic
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);
    }

    // =========================================================================
    // Focus-aware cursor blink tests (Chunk: docs/chunks/cursor_blink_focus)
    // =========================================================================

    #[test]
    fn test_buffer_focus_blink_toggles_cursor_visible() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Ensure buffer focus (default)
        assert_eq!(state.focus, EditorFocus::Buffer);

        // Set last_keystroke to the past so blink toggle works
        state.last_keystroke = Instant::now() - Duration::from_secs(1);

        assert!(state.cursor_visible);
        state.toggle_cursor_blink();
        assert!(!state.cursor_visible);
        state.toggle_cursor_blink();
        assert!(state.cursor_visible);
    }

    #[test]
    fn test_overlay_focus_blink_toggles_overlay_cursor_visible() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open find strip to switch to FindInFile focus
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);
        assert_eq!(state.focus, EditorFocus::FindInFile);

        // Set last_overlay_keystroke to the past so blink toggle works
        state.last_overlay_keystroke = Instant::now() - Duration::from_secs(1);

        assert!(state.overlay_cursor_visible);
        state.toggle_cursor_blink();
        assert!(!state.overlay_cursor_visible);
        state.toggle_cursor_blink();
        assert!(state.overlay_cursor_visible);
    }

    #[test]
    fn test_overlay_focus_does_not_toggle_buffer_cursor() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open find strip to switch to FindInFile focus
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);
        assert_eq!(state.focus, EditorFocus::FindInFile);

        // Buffer cursor should be visible (static)
        assert!(state.cursor_visible);

        // Set last_overlay_keystroke to the past so blink toggle works
        state.last_overlay_keystroke = Instant::now() - Duration::from_secs(1);

        // Toggle blink multiple times
        state.toggle_cursor_blink();
        state.toggle_cursor_blink();
        state.toggle_cursor_blink();

        // Buffer cursor should still be visible (not toggled)
        assert!(
            state.cursor_visible,
            "Buffer cursor should remain static when overlay has focus"
        );
    }

    #[test]
    fn test_recent_overlay_keystroke_keeps_overlay_cursor_solid() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open find strip
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Overlay keystroke just happened (set by handle_cmd_f)
        // Toggle should keep overlay cursor visible
        state.toggle_cursor_blink();
        assert!(
            state.overlay_cursor_visible,
            "Recent keystroke should keep overlay cursor solid"
        );
    }

    #[test]
    fn test_focus_transition_to_overlay_resets_cursors() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Make buffer cursor invisible to verify it gets reset
        state.cursor_visible = false;

        // Open find strip
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Both cursors should be visible after transition
        assert!(
            state.cursor_visible,
            "Buffer cursor should be visible (static) when overlay opens"
        );
        assert!(
            state.overlay_cursor_visible,
            "Overlay cursor should be visible when overlay opens"
        );
    }

    #[test]
    fn test_focus_transition_from_overlay_resets_buffer_cursor() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open find strip
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Make buffer cursor invisible
        state.cursor_visible = false;

        // Close find strip with Escape
        let escape = KeyEvent::new(Key::Escape, Modifiers::default());
        state.handle_key(escape);

        // Buffer should have focus again
        assert_eq!(state.focus, EditorFocus::Buffer);

        // Buffer cursor should be visible and last_keystroke should be recent
        assert!(
            state.cursor_visible,
            "Buffer cursor should be visible after closing overlay"
        );

        // Toggle should not immediately blink off because keystroke is recent
        state.toggle_cursor_blink();
        assert!(
            state.cursor_visible,
            "Buffer cursor should stay solid briefly after closing overlay"
        );
    }

    #[test]
    fn test_overlay_keystroke_makes_cursor_visible() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Open find strip
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Make overlay cursor invisible
        state.overlay_cursor_visible = false;

        // Type in find strip
        state.handle_key(KeyEvent::char('a'));

        // Overlay cursor should become visible
        assert!(
            state.overlay_cursor_visible,
            "Typing in overlay should make cursor visible"
        );
    }

    // =========================================================================
    // Terminal Scrollback Viewport Tests
    // Chunk: docs/chunks/terminal_scrollback_viewport - Terminal scroll integration tests
    // =========================================================================

    #[test]
    fn test_terminal_scroll_updates_viewport() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Scroll down (positive delta = content moves up = see older content)
        state.handle_scroll(ScrollDelta::new(0.0, 32.0));

        // With a new terminal, there's no scrollback beyond the visible area,
        // so offset stays at 0 (clamped to valid range)
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.viewport.scroll_offset_px() >= 0.0);
    }

    #[test]
    fn test_terminal_viewport_is_at_bottom_initial() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use lite_edit_buffer::BufferView;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Check that viewport is at bottom
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        let terminal = tab.as_terminal_buffer().expect("terminal");
        let line_count = terminal.line_count();

        // For a fresh terminal, it's "at bottom" if:
        // 1. viewport is uninitialized (visible_lines=0) with scroll_offset=0, OR
        // 2. line_count <= visible_lines (all content fits), OR
        // 3. scroll_offset >= max_offset (scrolled to the end)
        assert!(
            tab.viewport.is_at_bottom(line_count),
            "New terminal should have viewport at bottom"
        );
    }

    #[test]
    fn test_viewport_scroll_to_bottom() {
        // Test the Viewport::scroll_to_bottom helper
        let mut viewport = crate::viewport::Viewport::new(16.0);
        viewport.update_size(160.0, 100); // 10 visible lines, 100 total lines

        // Scroll to middle
        viewport.scroll_to(50, 100);
        assert!(!viewport.is_at_bottom(100));

        // Scroll to bottom
        viewport.scroll_to_bottom(100);
        assert!(viewport.is_at_bottom(100));
        assert_eq!(viewport.first_visible_line(), 90); // 100 - 10 = 90
    }

    #[test]
    fn test_viewport_is_at_bottom_edge_cases() {
        let mut viewport = crate::viewport::Viewport::new(16.0);
        viewport.update_size(160.0, 100); // 10 visible lines

        // Empty content is always at bottom
        assert!(viewport.is_at_bottom(0));

        // Content smaller than viewport is at bottom
        assert!(viewport.is_at_bottom(5));

        // Exactly filling viewport is at bottom
        viewport.scroll_to(0, 10);
        assert!(viewport.is_at_bottom(10));

        // Larger content - check various positions
        viewport.scroll_to(0, 100);
        assert!(!viewport.is_at_bottom(100));

        viewport.scroll_to_bottom(100);
        assert!(viewport.is_at_bottom(100));
    }

    #[test]
    fn test_terminal_scroll_clamps_to_bounds() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Try to scroll up past the start (negative delta)
        state.handle_scroll(ScrollDelta::new(0.0, -1000.0));

        // Viewport should be clamped to 0
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert_eq!(tab.viewport.scroll_offset_px(), 0.0);

        // Try to scroll down past the end
        state.handle_scroll(ScrollDelta::new(0.0, 100000.0));

        // Viewport should be clamped to max (which is 0 for a fresh terminal)
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.viewport.scroll_offset_px() >= 0.0);
    }

    // =========================================================================
    // Terminal Initial Render Tests
    // Chunk: docs/chunks/terminal_viewport_init - Tests for terminal viewport initialization
    // =========================================================================

    /// Tests that poll_agents returns dirty after a new terminal tab is created
    /// and the shell has had time to produce output.
    ///
    /// This validates the core requirement: when a terminal tab is created and
    /// we poll for PTY events, we should eventually get a dirty region indicating
    /// that the terminal has content to render.
    #[test]
    fn test_poll_agents_dirty_after_terminal_creation() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // The terminal shell needs time to start and produce output.
        // Poll repeatedly until we get dirty activity.
        let mut found_dirty = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(20));
            let (dirty, _needs_rewakeup) = state.poll_agents();
            if dirty.is_dirty() {
                found_dirty = true;
                break;
            }
        }

        assert!(
            found_dirty,
            "poll_agents should return dirty when terminal produces output"
        );
    }

    /// Tests that new_terminal_tab marks the viewport dirty.
    ///
    /// This is separate from the PTY output - the act of creating a terminal
    /// tab should mark the viewport dirty so that at minimum an initial render
    /// is triggered (even if it shows an empty terminal).
    #[test]
    fn test_new_terminal_tab_marks_dirty() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Clear any existing dirty state
        let _ = state.take_dirty_region();
        assert!(!state.is_dirty());

        // Create a terminal tab
        state.new_terminal_tab();

        // Should be dirty after creating a tab
        assert!(
            state.is_dirty(),
            "EditorState should be dirty after creating a terminal tab"
        );
    }

    /// Tests that the terminal viewport has correct visible_rows immediately after creation.
    ///
    /// This validates the root fix from terminal_viewport_init: terminal tab viewports must
    /// have non-zero visible_rows immediately after creation, so scroll_to_bottom computes
    /// correct offsets. Without this, visible_rows=0 causes scroll_to_bottom to scroll past
    /// all content, producing a blank screen.
    #[test]
    fn test_terminal_viewport_has_visible_rows_immediately() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Verify that the viewport has non-zero visible_lines immediately
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");

        assert!(
            tab.viewport.visible_lines() > 0,
            "Terminal viewport should have non-zero visible_lines immediately after creation (got {})",
            tab.viewport.visible_lines()
        );

        // Visible lines should match expected value based on content height and line height
        let content_height = 600.0; // 600.0 + TAB_BAR_HEIGHT - TAB_BAR_HEIGHT
        let line_height = test_font_metrics().line_height;
        let expected_visible = (content_height as f64 / line_height).floor() as usize;

        assert_eq!(
            tab.viewport.visible_lines(),
            expected_visible,
            "Terminal viewport should have {} visible lines based on content height",
            expected_visible
        );
    }

    // =========================================================================
    // Dirty Flag Tests (Chunk: docs/chunks/unsaved_tab_tint)
    // =========================================================================

    /// Tests that editing a file buffer sets the tab's dirty flag to true.
    #[test]
    fn test_editing_sets_tab_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Initially, the tab should not be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "New tab should not be dirty initially");

        // Type a character
        state.handle_key(KeyEvent::char('a'));

        // Now the tab should be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.dirty, "Tab should be dirty after editing");
    }

    /// Tests that saving a file clears the tab's dirty flag.
    #[test]
    fn test_save_clears_dirty_flag() {
        use std::io::Write;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a temporary file
        let mut temp_file = tempfile::NamedTempFile::new().expect("create temp file");
        writeln!(temp_file, "initial content").expect("write to temp");
        let temp_path = temp_file.path().to_path_buf();

        // Associate the file with the tab
        state.associate_file(temp_path.clone());

        // Type a character to make the tab dirty
        state.handle_key(KeyEvent::char('X'));

        // Confirm the tab is dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.dirty, "Tab should be dirty after editing");

        // Save the file (Cmd+S)
        let cmd_s = KeyEvent::new(
            Key::Char('s'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_s);

        // Tab should no longer be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Tab should not be dirty after save");

        // Verify the file was actually written
        let content = std::fs::read_to_string(&temp_path).expect("read temp file");
        assert!(content.contains('X'), "Saved content should contain typed character");
    }

    /// Tests that the dirty flag persists across multiple edits.
    #[test]
    fn test_dirty_flag_persists_across_edits() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type several characters
        state.handle_key(KeyEvent::char('a'));
        state.handle_key(KeyEvent::char('b'));
        state.handle_key(KeyEvent::char('c'));

        // Tab should still be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.dirty, "Tab should remain dirty across multiple edits");
    }

    /// Tests that a clean tab is not marked dirty just from navigation/cursor movement.
    /// Note: This test may need adjustment if cursor movement triggers dirty region
    /// for other reasons. The plan acknowledges over-marking is acceptable.
    #[test]
    fn test_new_tab_starts_clean() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Create a new tab
        state.new_tab();

        // The new tab should not be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Newly created tab should not be dirty");
    }

    /// Tests that terminal tabs don't get marked dirty (they don't save to files).
    #[test]
    fn test_terminal_tab_not_marked_dirty_on_input() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Get initial dirty state
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        let initial_dirty = tab.dirty;

        // Send some input to the terminal
        state.handle_key(KeyEvent::char('l'));
        state.handle_key(KeyEvent::char('s'));

        // Terminal tab should not have its dirty flag changed
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert_eq!(tab.dirty, initial_dirty,
            "Terminal tab dirty flag should not change from input");
    }

    // =========================================================================
    // Navigation Does Not Set Dirty Tests (Chunk: docs/chunks/dirty_bit_navigation)
    // =========================================================================

    /// Tests that arrow key navigation does not set the tab's dirty flag.
    #[test]
    fn test_arrow_key_navigation_does_not_set_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add some content first (without marking dirty) by loading a file
        state.buffer_mut().insert_str("hello\nworld\nfoo");
        // Clear the dirty flag that would have been set
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Verify tab is clean
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Tab should start clean for this test");

        // Arrow down
        state.handle_key(KeyEvent::new(Key::Down, Modifiers::default()));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Arrow down should not set dirty");

        // Arrow up
        state.handle_key(KeyEvent::new(Key::Up, Modifiers::default()));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Arrow up should not set dirty");

        // Arrow left
        state.handle_key(KeyEvent::new(Key::Left, Modifiers::default()));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Arrow left should not set dirty");

        // Arrow right
        state.handle_key(KeyEvent::new(Key::Right, Modifiers::default()));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Arrow right should not set dirty");
    }

    /// Tests that Command+A (select all) does not set the tab's dirty flag.
    #[test]
    fn test_select_all_does_not_set_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add some content
        state.buffer_mut().insert_str("hello world");
        // Clear the dirty flag
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Verify tab is clean
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Tab should start clean for this test");

        // Cmd+A (select all)
        let cmd_a = KeyEvent::new(
            Key::Char('a'),
            Modifiers { command: true, ..Default::default() },
        );
        state.handle_key(cmd_a);

        // Tab should still not be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Select all should not set dirty");
    }

    /// Tests that Shift+arrow selection does not set the tab's dirty flag.
    #[test]
    fn test_shift_arrow_selection_does_not_set_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add some content
        state.buffer_mut().insert_str("hello world");
        // Clear the dirty flag
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Verify tab is clean
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Tab should start clean for this test");

        // Shift+Right (extend selection)
        let shift_right = KeyEvent::new(
            Key::Right,
            Modifiers { shift: true, ..Default::default() },
        );
        state.handle_key(shift_right);

        // Tab should still not be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Shift+arrow selection should not set dirty");

        // Shift+Left
        let shift_left = KeyEvent::new(
            Key::Left,
            Modifiers { shift: true, ..Default::default() },
        );
        state.handle_key(shift_left);

        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Shift+left selection should not set dirty");
    }

    /// Tests that Option+arrow word jump navigation does not set the tab's dirty flag.
    #[test]
    fn test_word_jump_does_not_set_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add some content with multiple words
        state.buffer_mut().insert_str("hello world foo bar");
        // Clear the dirty flag
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Verify tab is clean
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Tab should start clean for this test");

        // Option+Right (word jump)
        let opt_right = KeyEvent::new(
            Key::Right,
            Modifiers { option: true, ..Default::default() },
        );
        state.handle_key(opt_right);

        // Tab should still not be dirty
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Option+arrow word jump should not set dirty");

        // Option+Left (word jump back)
        let opt_left = KeyEvent::new(
            Key::Left,
            Modifiers { option: true, ..Default::default() },
        );
        state.handle_key(opt_left);

        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Option+left word jump should not set dirty");
    }

    /// Tests that Page Up/Down does not set the tab's dirty flag.
    #[test]
    fn test_page_up_down_does_not_set_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add many lines of content
        let content = (0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        state.buffer_mut().insert_str(&content);
        // Clear the dirty flag
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Verify tab is clean
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Tab should start clean for this test");

        // Page Down
        state.handle_key(KeyEvent::new(Key::PageDown, Modifiers::default()));

        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Page Down should not set dirty");

        // Page Up
        state.handle_key(KeyEvent::new(Key::PageUp, Modifiers::default()));

        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "Page Up should not set dirty");
    }

    /// Tests that content-mutating operations still correctly set the tab's dirty flag.
    /// This is a regression test to ensure the fix for navigation doesn't break mutations.
    #[test]
    fn test_mutations_still_set_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Start with a clean tab
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(!tab.dirty, "New tab should not be dirty initially");

        // Type a character - should set dirty
        state.handle_key(KeyEvent::char('a'));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.dirty, "Typing should set dirty");

        // Clear dirty flag to test other mutations
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Delete backward - should set dirty
        state.handle_key(KeyEvent::new(Key::Backspace, Modifiers::default()));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.dirty, "Backspace should set dirty");

        // Clear dirty flag
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        // Type some content then test delete forward
        state.handle_key(KeyEvent::char('b'));
        state.handle_key(KeyEvent::new(Key::Left, Modifiers::default())); // Move left (shouldn't set dirty alone)
        let ws = state.editor.active_workspace_mut().expect("workspace");
        let tab = ws.active_tab_mut().expect("tab");
        tab.dirty = false;

        state.handle_key(KeyEvent::new(Key::Delete, Modifiers::default()));
        let ws = state.editor.active_workspace().expect("workspace");
        let tab = ws.active_tab().expect("tab");
        assert!(tab.dirty, "Delete forward should set dirty");
    }

    // =========================================================================
    // Deferred Initialization Tests (Chunk: docs/chunks/startup_workspace_dialog)
    // =========================================================================

    #[test]
    fn test_editor_state_new_deferred_has_no_workspace() {
        let state = EditorState::new_deferred(test_font_metrics());
        assert_eq!(state.editor.workspace_count(), 0);
    }

    #[test]
    fn test_editor_state_new_deferred_can_add_workspace() {
        let mut state = EditorState::new_deferred(test_font_metrics());
        dir_picker::mock_set_next_directory(Some(PathBuf::from("/test")));
        state.new_workspace();
        assert_eq!(state.editor.workspace_count(), 1);
    }

    #[test]
    fn test_editor_state_new_deferred_add_startup_workspace() {
        let mut state = EditorState::new_deferred(test_font_metrics());
        state.add_startup_workspace(PathBuf::from("/my/project"));

        // Should have one workspace
        assert_eq!(state.editor.workspace_count(), 1);

        // Workspace should have the correct root path
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.root_path, PathBuf::from("/my/project"));

        // Workspace label should be derived from directory name
        assert_eq!(ws.label, "project");

        // Should have one tab (welcome screen)
        assert_eq!(ws.tab_count(), 1);
    }

    #[test]
    fn test_editor_state_new_deferred_workspace_label_from_dirname() {
        let mut state = EditorState::new_deferred(test_font_metrics());
        state.add_startup_workspace(PathBuf::from("/home/user/my-awesome-project"));

        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.label, "my-awesome-project");
    }

    #[test]
    fn test_editor_state_new_deferred_root_path_fallback_label() {
        let mut state = EditorState::new_deferred(test_font_metrics());
        // Root path "/" has no file_name, should use fallback
        state.add_startup_workspace(PathBuf::from("/"));

        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.label, "workspace");
    }

    // =========================================================================
    // Hover-scroll (pane-targeted scroll) tests
    // Chunk: docs/chunks/pane_hover_scroll - Tests for hover-targeted pane scrolling
    // =========================================================================

    #[test]
    fn test_scroll_without_position_uses_focused_pane() {
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

        // Initial scroll offset should be 0
        assert_eq!(state.viewport().first_visible_line(), 0);

        // Scroll without mouse position (mouse_position = None)
        // This should scroll the focused pane
        state.handle_scroll(ScrollDelta::new(0.0, 80.0));

        // Viewport should have scrolled
        assert_eq!(state.viewport().first_visible_line(), 5);
    }

    #[test]
    fn test_find_pane_at_scroll_position_returns_focused_when_no_position() {
        let content = "test content".to_string();
        let state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );

        let delta = ScrollDelta::new(0.0, 80.0);
        let pane_id = state.find_pane_at_scroll_position(&delta);

        // Should return the focused pane ID
        let expected_pane_id = state
            .editor
            .active_workspace()
            .map(|ws| ws.active_pane_id)
            .unwrap_or(0);
        assert_eq!(pane_id, expected_pane_id);
    }

    #[test]
    fn test_find_pane_at_scroll_position_outside_content_area_returns_focused() {
        let content = "test content".to_string();
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0);

        // Position in the tab bar area (y < TAB_BAR_HEIGHT)
        let delta = ScrollDelta::with_position(0.0, 80.0, 100.0, 10.0);
        let pane_id = state.find_pane_at_scroll_position(&delta);

        // Should return the focused pane ID
        let expected_pane_id = state
            .editor
            .active_workspace()
            .map(|ws| ws.active_pane_id)
            .unwrap_or(0);
        assert_eq!(pane_id, expected_pane_id);
    }

    #[test]
    fn test_find_pane_at_scroll_position_in_rail_returns_focused() {
        let content = "test content".to_string();
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0);

        // Position in the left rail area (x < RAIL_WIDTH)
        let delta = ScrollDelta::with_position(0.0, 80.0, 10.0, 50.0);
        let pane_id = state.find_pane_at_scroll_position(&delta);

        // Should return the focused pane ID
        let expected_pane_id = state
            .editor
            .active_workspace()
            .map(|ws| ws.active_pane_id)
            .unwrap_or(0);
        assert_eq!(pane_id, expected_pane_id);
    }

    #[test]
    fn test_find_pane_at_scroll_position_in_content_area_single_pane() {
        let content = "test content".to_string();
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_size(160.0);

        // Position in the content area (below tab bar, right of rail)
        // RAIL_WIDTH = 32.0, TAB_BAR_HEIGHT = 24.0
        let delta = ScrollDelta::with_position(0.0, 80.0, 100.0, 100.0);
        let pane_id = state.find_pane_at_scroll_position(&delta);

        // With single pane, should return the only pane's ID
        let expected_pane_id = state
            .editor
            .active_workspace()
            .map(|ws| ws.active_pane_id)
            .unwrap_or(0);
        assert_eq!(pane_id, expected_pane_id);
    }

    #[test]
    fn test_scroll_with_position_scrolls_correct_pane_single_pane_setup() {
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

        // Initial scroll offset should be 0
        assert_eq!(state.viewport().first_visible_line(), 0);

        // Scroll with mouse position in content area
        // RAIL_WIDTH = 32.0, TAB_BAR_HEIGHT = 24.0
        state.handle_scroll(ScrollDelta::with_position(0.0, 80.0, 100.0, 100.0));

        // Viewport should have scrolled (same behavior as without position in single pane)
        assert_eq!(state.viewport().first_visible_line(), 5);
    }

    #[test]
    fn test_scroll_delta_with_position_constructor() {
        let delta = ScrollDelta::with_position(1.0, 2.0, 100.0, 200.0);
        assert_eq!(delta.dx, 1.0);
        assert_eq!(delta.dy, 2.0);
        assert_eq!(delta.mouse_position, Some((100.0, 200.0)));
    }

    #[test]
    fn test_scroll_delta_new_has_no_position() {
        let delta = ScrollDelta::new(1.0, 2.0);
        assert_eq!(delta.dx, 1.0);
        assert_eq!(delta.dy, 2.0);
        assert_eq!(delta.mouse_position, None);
    }

    #[test]
    fn test_scroll_multi_pane_hits_non_focused_pane() {
        use crate::pane_layout::{Pane, PaneLayoutNode, SplitDirection};
        use crate::workspace::Tab;

        // Create a state
        let content = (0..50)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_dimensions(800.0, 600.0);

        // Create two panes with tabs using explicit IDs (matching workspace tests)
        let line_height = test_font_metrics().line_height as f32;
        let pane1_id = 1u64;
        let pane2_id = 2u64;

        let mut pane1 = Pane::new(pane1_id, 1);
        pane1.add_tab(Tab::new_file(
            1,
            lite_edit_buffer::TextBuffer::from_str(
                &(0..30).map(|i| format!("pane1 line {}", i)).collect::<Vec<_>>().join("\n"),
            ),
            "Pane1".to_string(),
            None,
            line_height,
        ));
        let mut pane2 = Pane::new(pane2_id, 2);
        pane2.add_tab(Tab::new_file(
            2,
            lite_edit_buffer::TextBuffer::from_str(
                &(0..30).map(|i| format!("pane2 line {}", i)).collect::<Vec<_>>().join("\n"),
            ),
            "Pane2".to_string(),
            None,
            line_height,
        ));

        // Set up split layout: horizontal split (left | right)
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.pane_root = PaneLayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane1)),
                second: Box::new(PaneLayoutNode::Leaf(pane2)),
            };
            ws.active_pane_id = pane1_id; // Focus on left pane
        }

        // Content area is right of RAIL_WIDTH (32) and below TAB_BAR_HEIGHT (24)
        // Content width = 800 - 32 = 768, Content height = 600 - 24 = 576
        // With horizontal split at ratio 0.5:
        // - Left pane: x=[0, 384), y=[0, 576)
        // - Right pane: x=[384, 768), y=[0, 576)
        //
        // In screen coordinates (from window top-left):
        // - Left pane: x=[32, 416), y=[24, 600)
        // - Right pane: x=[416, 800), y=[24, 600)

        // Scroll with mouse position over the RIGHT pane (while left pane is focused)
        // Screen coords: x=500 (in right pane), y=100 (below tab bar)
        let delta = ScrollDelta::with_position(0.0, 48.0, 500.0, 100.0);
        let target_pane_id = state.find_pane_at_scroll_position(&delta);

        // Should target the right pane (pane2), not the focused left pane (pane1)
        assert_eq!(target_pane_id, pane2_id, "Scroll should target pane under cursor, not focused pane");

        // Verify focused pane is still pane1
        let focused_pane_id = state
            .editor
            .active_workspace()
            .map(|ws| ws.active_pane_id)
            .unwrap_or(0);
        assert_eq!(focused_pane_id, pane1_id, "Focus should remain on pane1");
    }

    #[test]
    fn test_scroll_multi_pane_outside_panes_returns_focused() {
        use crate::pane_layout::{Pane, PaneLayoutNode, SplitDirection};
        use crate::workspace::Tab;

        // Create a state
        let content = "test".to_string();
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str(&content),
            test_font_metrics(),
        );
        state.update_viewport_dimensions(800.0, 600.0);

        // Create two panes
        let line_height = test_font_metrics().line_height as f32;
        let pane1_id = 1u64;
        let pane2_id = 2u64;

        let mut pane1 = Pane::new(pane1_id, 1);
        pane1.add_tab(Tab::new_file(
            1,
            lite_edit_buffer::TextBuffer::from_str("pane1 content"),
            "Pane1".to_string(),
            None,
            line_height,
        ));
        let mut pane2 = Pane::new(pane2_id, 2);
        pane2.add_tab(Tab::new_file(
            2,
            lite_edit_buffer::TextBuffer::from_str("pane2 content"),
            "Pane2".to_string(),
            None,
            line_height,
        ));

        // Set up split layout
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.pane_root = PaneLayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane1)),
                second: Box::new(PaneLayoutNode::Leaf(pane2)),
            };
            ws.active_pane_id = pane1_id;
        }

        // Scroll with mouse position in the rail area (outside content panes)
        let delta = ScrollDelta::with_position(0.0, 48.0, 10.0, 100.0);
        let target_pane_id = state.find_pane_at_scroll_position(&delta);

        // Should fall back to focused pane
        assert_eq!(target_pane_id, pane1_id);
    }

    // =========================================================================
    // Pane Close Last Tab Tests (Chunk: docs/chunks/pane_close_last_tab)
    // =========================================================================

    use crate::pane_layout::{Pane, PaneLayoutNode, SplitDirection};

    /// Helper to create an EditorState with a horizontal split (two panes side by side).
    /// Each pane has exactly one tab.
    fn create_hsplit_state() -> EditorState {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);
        let line_height = test_font_metrics().line_height as f32;

        // Get the workspace
        let ws = state.editor.active_workspace_mut().unwrap();
        let ws_id = ws.id;

        // Create two panes, each with one tab
        let mut pane1 = Pane::new(1, ws_id);
        pane1.add_tab(crate::workspace::Tab::empty_file(100, line_height));

        let mut pane2 = Pane::new(2, ws_id);
        pane2.add_tab(crate::workspace::Tab::empty_file(101, line_height));

        // Create horizontal split layout (pane1 left, pane2 right)
        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };
        ws.active_pane_id = 1; // Start focused on left pane
        // Note: next_pane_id is private, but we don't need to set it for tests
        // since we're manually constructing the pane tree

        state
    }

    /// Closing the last tab in a pane with multiple panes should:
    /// 1. Remove the empty pane from the layout via cleanup_empty_panes
    /// 2. Move focus to an adjacent pane
    /// 3. NOT panic
    // Chunk: docs/chunks/pane_close_last_tab - Cleanup empty panes on last tab close
    #[test]
    fn test_close_last_tab_in_multi_pane_layout_no_panic() {
        let mut state = create_hsplit_state();

        // Verify initial state: 2 panes
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 2);
        assert_eq!(ws.active_pane_id, 1);

        // Close the only tab in the active pane (pane 1)
        // This should NOT panic and should collapse the tree to a single pane
        state.close_tab(0);

        // After closing, tree should collapse to single pane (pane 2)
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 1, "Tree should collapse to single pane");

        // Active pane should now be pane 2 (the remaining pane)
        assert_eq!(ws.active_pane_id, 2, "Focus should move to remaining pane");

        // The remaining pane should have its tab
        let pane = ws.active_pane().expect("should have active pane");
        assert_eq!(pane.tab_count(), 1);
    }

    /// Three-pane layout: HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
    /// Close last tab in B → tree becomes HSplit(Pane[A], Pane[C])
    #[test]
    fn test_close_last_tab_in_nested_layout() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);
        let line_height = test_font_metrics().line_height as f32;

        let ws = state.editor.active_workspace_mut().unwrap();
        let ws_id = ws.id;

        // Create three panes
        let mut pane_a = Pane::new(1, ws_id);
        pane_a.add_tab(crate::workspace::Tab::empty_file(100, line_height));

        let mut pane_b = Pane::new(2, ws_id);
        pane_b.add_tab(crate::workspace::Tab::empty_file(101, line_height));

        let mut pane_c = Pane::new(3, ws_id);
        pane_c.add_tab(crate::workspace::Tab::empty_file(102, line_height));

        // Create HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Split {
                direction: crate::pane_layout::SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane_b)),
                second: Box::new(PaneLayoutNode::Leaf(pane_c)),
            }),
        };
        ws.active_pane_id = 2; // Focus on pane B

        // Verify initial state: 3 panes
        assert_eq!(ws.pane_root.pane_count(), 3);

        // Close the only tab in pane B
        state.close_tab(0);

        // After closing, tree should be HSplit(Pane[A], Pane[C]) - 2 panes
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 2, "Tree should collapse to 2 panes");

        // Pane A and C should still exist
        assert!(ws.pane_root.get_pane(1).is_some(), "Pane A should exist");
        assert!(ws.pane_root.get_pane(3).is_some(), "Pane C should exist");
        // Pane B should be gone
        assert!(ws.pane_root.get_pane(2).is_none(), "Pane B should be removed");

        // Focus should have moved to an adjacent pane
        let active_pane = ws.active_pane().expect("should have active pane");
        assert!(active_pane.id == 1 || active_pane.id == 3, "Focus should be on A or C");
    }

    /// Single-pane single-tab behavior should be unchanged: replace with empty tab.
    #[test]
    fn test_close_last_tab_single_pane_unchanged() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Verify initial state: single pane with one tab
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 1);
        assert_eq!(ws.tab_count(), 1);

        // Close the only tab
        state.close_tab(0);

        // Should still have one pane with one (empty) tab
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 1);
        assert_eq!(ws.tab_count(), 1);
        assert_eq!(ws.active_tab().unwrap().label, "Untitled");
    }

    /// Multi-tab pane: closing a non-last tab doesn't trigger cleanup.
    #[test]
    fn test_close_non_last_tab_no_cleanup() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a second tab
        state.new_tab();

        // Verify initial state: single pane with two tabs
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 1);
        assert_eq!(ws.tab_count(), 2);

        // Close the first tab
        state.close_tab(0);

        // Should still have one pane, now with one tab
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.pane_root.pane_count(), 1);
        assert_eq!(ws.tab_count(), 1);
    }

    // =========================================================================
    // Chunk: docs/chunks/split_scroll_viewport - Post-split viewport sync tests
    // =========================================================================

    /// Helper to create a vertical split (top/bottom panes) with content.
    #[allow(unused_imports)]
    fn create_vsplit_state_with_content() -> EditorState {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::left_rail::RAIL_WIDTH;
        let _ = (TAB_BAR_HEIGHT, RAIL_WIDTH); // silence unused warnings (values used in comments)
        let mut state = EditorState::empty(test_font_metrics());

        // Window: 800x600 total
        // Content area: (800 - RAIL_WIDTH) x 600 = (800-56) x 600 = 744 x 600
        state.update_viewport_dimensions(800.0, 600.0);
        let line_height = test_font_metrics().line_height as f32;

        // Get the workspace
        let ws = state.editor.active_workspace_mut().unwrap();
        let ws_id = ws.id;

        // Create two panes, each with one tab containing content
        let mut pane1 = Pane::new(1, ws_id);
        let mut tab1 = crate::workspace::Tab::empty_file(100, line_height);
        // Insert 50 lines of content into the tab
        if let Some(buf) = tab1.as_text_buffer_mut() {
            for i in 0..50 {
                buf.insert_str(&format!("Line {}\n", i));
            }
            buf.set_cursor(lite_edit_buffer::Position::new(0, 0));
        }
        pane1.add_tab(tab1);

        let mut pane2 = Pane::new(2, ws_id);
        let mut tab2 = crate::workspace::Tab::empty_file(101, line_height);
        // Insert 30 lines of content into the second tab
        if let Some(buf) = tab2.as_text_buffer_mut() {
            for i in 0..30 {
                buf.insert_str(&format!("Content {}\n", i));
            }
            buf.set_cursor(lite_edit_buffer::Position::new(0, 0));
        }
        pane2.add_tab(tab2);

        // Create vertical split layout (pane1 top, pane2 bottom)
        // This is the "horizontal split" described in the bug (splits horizontally,
        // resulting in top/bottom panes with reduced vertical space each)
        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };
        ws.active_pane_id = 1; // Start focused on top pane

        // Sync viewports after constructing the split
        state.sync_pane_viewports();

        state
    }

    /// After a vertical split, each pane's viewport should have reduced visible_lines.
    #[test]
    fn test_vsplit_reduces_visible_lines() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let state = create_vsplit_state_with_content();
        let line_height = test_font_metrics().line_height as f32;

        // Content area: 744 x 600
        // With vertical split at ratio 0.5:
        // - Top pane: 744 x 300 (half of 600)
        // - Bottom pane: 744 x 300
        // Pane content height = pane height - TAB_BAR_HEIGHT = 300 - 32 = 268
        // Visible lines = floor(268 / line_height)

        let expected_pane_height = 600.0 / 2.0; // 300
        let expected_content_height = expected_pane_height - TAB_BAR_HEIGHT;
        let expected_visible_lines = (expected_content_height / line_height).floor() as usize;

        let ws = state.editor.active_workspace().unwrap();

        // Check top pane's tab viewport
        let top_pane = ws.pane_root.get_pane(1).expect("pane 1 should exist");
        let top_tab = top_pane.active_tab().expect("should have active tab");
        assert_eq!(
            top_tab.viewport.visible_lines(),
            expected_visible_lines,
            "Top pane visible_lines should match split geometry"
        );

        // Check bottom pane's tab viewport
        let bottom_pane = ws.pane_root.get_pane(2).expect("pane 2 should exist");
        let bottom_tab = bottom_pane.active_tab().expect("should have active tab");
        assert_eq!(
            bottom_tab.viewport.visible_lines(),
            expected_visible_lines,
            "Bottom pane visible_lines should match split geometry"
        );
    }

    /// A tab with more lines than visible should be scrollable after split.
    #[test]
    fn test_tab_becomes_scrollable_after_split() {
        let mut state = create_vsplit_state_with_content();

        // The top pane has 50 lines of content
        // After split, visible_lines is approximately floor((300-32)/line_height)
        // With line_height ~20, visible_lines is floor(268/20) = 13 lines
        // 50 lines > 13 visible lines, so content should be scrollable

        let ws = state.editor.active_workspace_mut().unwrap();
        let top_pane = ws.pane_root.get_pane_mut(1).expect("pane 1 should exist");
        let top_tab = top_pane.active_tab_mut().expect("should have active tab");

        let visible_lines = top_tab.viewport.visible_lines();
        let line_count = top_tab.as_text_buffer().unwrap().line_count();

        assert!(
            line_count > visible_lines,
            "Content ({} lines) should exceed viewport ({} visible lines)",
            line_count,
            visible_lines
        );

        // Verify scrolling works: scroll to line 10
        top_tab.viewport.scroll_to(10, line_count);
        assert_eq!(
            top_tab.viewport.first_visible_line(),
            10,
            "Should be able to scroll to line 10"
        );

        // Verify we can scroll to near the end
        let max_scroll = line_count.saturating_sub(visible_lines);
        top_tab.viewport.scroll_to(max_scroll, line_count);
        assert_eq!(
            top_tab.viewport.first_visible_line(),
            max_scroll,
            "Should be able to scroll to max position"
        );
    }

    /// Window resize should update all pane viewports.
    #[test]
    fn test_resize_updates_all_pane_viewports() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = create_vsplit_state_with_content();
        let line_height = test_font_metrics().line_height as f32;

        // Get initial visible lines
        let initial_visible_lines = {
            let ws = state.editor.active_workspace().unwrap();
            let pane = ws.pane_root.get_pane(1).unwrap();
            pane.active_tab().unwrap().viewport.visible_lines()
        };

        // Resize window to double height (600 -> 1200)
        state.update_viewport_dimensions(800.0, 1200.0);

        // After resize, each pane has half of 1200 = 600 height
        // Pane content height = 600 - 32 = 568
        // Visible lines = floor(568 / line_height) ≈ 28 lines
        let new_pane_height = 1200.0 / 2.0;
        let new_content_height = new_pane_height - TAB_BAR_HEIGHT;
        let expected_new_visible_lines = (new_content_height / line_height).floor() as usize;

        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.pane_root.get_pane(1).unwrap();
        let actual_visible_lines = pane.active_tab().unwrap().viewport.visible_lines();

        assert!(
            actual_visible_lines > initial_visible_lines,
            "Visible lines should increase after window resize: was {}, now {}",
            initial_visible_lines,
            actual_visible_lines
        );
        assert_eq!(
            actual_visible_lines, expected_new_visible_lines,
            "Visible lines should match expected geometry after resize"
        );
    }

    /// Scroll offset should be clamped when pane shrinks.
    #[test]
    fn test_scroll_clamped_on_shrink() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = create_vsplit_state_with_content();
        let line_height = test_font_metrics().line_height as f32;

        // Scroll to line 30 in the top pane (which has 50 lines)
        {
            let ws = state.editor.active_workspace_mut().unwrap();
            let pane = ws.pane_root.get_pane_mut(1).unwrap();
            let tab = pane.active_tab_mut().unwrap();
            let line_count = tab.as_text_buffer().unwrap().line_count();
            tab.viewport.scroll_to(30, line_count);
            assert_eq!(tab.viewport.first_visible_line(), 30);
        }

        // Shrink window height significantly (600 -> 200)
        // This makes panes very small (100px each)
        // Pane content height = 100 - 32 = 68px
        // Visible lines = floor(68 / 20) ≈ 3 lines
        // Max scroll = 50 - 3 = 47, so scroll of 30 should still be valid
        state.update_viewport_dimensions(800.0, 200.0);

        // Verify scroll is still at line 30 (within valid range)
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.pane_root.get_pane(1).unwrap();
        let tab = pane.active_tab().unwrap();
        let visible_lines = tab.viewport.visible_lines();
        let first_visible = tab.viewport.first_visible_line();

        // The scroll should be valid (not clamped) since 30 < 50 - 3 = 47
        assert!(
            first_visible <= 30,
            "Scroll should be at most 30 after shrink (got {})",
            first_visible
        );
    }

    // =========================================================================
    // Chunk: docs/chunks/vsplit_scroll - Vertical split scroll bounds tests
    // =========================================================================

    /// Regression test: After a vertical split, scroll_pane should use pane-specific
    /// dimensions for scroll bound clamping, not full-window dimensions.
    ///
    /// Bug: When scrolling in a vertical split pane, the EditorContext was created
    /// with full-window height instead of pane height. This caused the WrapLayout
    /// to underestimate total screen rows for wrapped content, resulting in a max
    /// scroll offset that was too low to reach the end of the document.
    #[test]
    fn test_vsplit_scroll_uses_pane_dimensions() {
        use crate::pane_layout::SplitDirection;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        // Window: 800x600 total
        state.update_viewport_dimensions(800.0, 600.0);

        // Get workspace
        let ws = state.editor.active_workspace_mut().unwrap();
        let ws_id = ws.id;

        // Create a pane with 100 lines of content (enough to require scrolling)
        let mut pane1 = Pane::new(1, ws_id);
        let line_height = test_font_metrics().line_height as f32;
        let mut tab1 = crate::workspace::Tab::empty_file(100, line_height);
        if let Some(buf) = tab1.as_text_buffer_mut() {
            for i in 0..100 {
                buf.insert_str(&format!("Line number {}\n", i));
            }
            buf.set_cursor(lite_edit_buffer::Position::new(0, 0));
        }
        pane1.add_tab(tab1);

        // Create a second pane
        let mut pane2 = Pane::new(2, ws_id);
        let tab2 = crate::workspace::Tab::empty_file(101, line_height);
        pane2.add_tab(tab2);

        // Create vertical split (pane1 top, pane2 bottom)
        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };
        ws.active_pane_id = 1;

        // Sync viewports after constructing the split
        state.sync_pane_viewports();

        // Calculate expected dimensions
        // Content area: (800 - RAIL_WIDTH) x 600 = 744 x 600
        // Pane height: 600 * 0.5 = 300
        // Pane content height: 300 - TAB_BAR_HEIGHT = 268
        let pane_height = 600.0 * 0.5;
        let expected_content_height = pane_height - TAB_BAR_HEIGHT;
        let expected_visible_lines = (expected_content_height / line_height).floor() as usize;

        // Verify the helper returns correct pane dimensions
        let dims = state.get_pane_content_dimensions(1);
        assert!(dims.is_some(), "Should find pane 1 dimensions");
        let (content_height, _content_width) = dims.unwrap();
        assert!(
            (content_height - expected_content_height).abs() < 0.01,
            "Content height should be {} but got {}",
            expected_content_height,
            content_height
        );

        // Get line count for scroll calculations
        let line_count = {
            let ws = state.editor.active_workspace().unwrap();
            let pane = ws.pane_root.get_pane(1).unwrap();
            let tab = pane.active_tab().unwrap();
            tab.as_text_buffer().unwrap().line_count()
        };

        // Calculate max scroll position based on pane content height
        // max_scroll_line = line_count - visible_lines
        let max_scroll_line = line_count.saturating_sub(expected_visible_lines);

        // Try to scroll to the end of the document
        // Create a large scroll delta that should take us to max position
        let large_scroll_px = (line_count as f64) * (line_height as f64);
        state.handle_scroll(ScrollDelta::new(0.0, large_scroll_px));

        // The viewport should be scrolled to near the max position
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.pane_root.get_pane(1).unwrap();
        let tab = pane.active_tab().unwrap();
        let first_visible = tab.viewport.first_visible_line();

        // With the fix, we should be able to reach near the end.
        // The bug was that first_visible would be much smaller than max_scroll_line
        // because the scroll was clamped using wrong (full-window) dimensions.
        assert!(
            first_visible >= max_scroll_line.saturating_sub(1),
            "Should be able to scroll to line {} (max), but got line {}. \
             This indicates scroll_pane may be using full-window dimensions instead of pane dimensions.",
            max_scroll_line,
            first_visible
        );
    }

    /// Regression test: In a horizontal split (side-by-side panes), lines wrap more
    /// in the narrower panes. The scroll clamping must use the pane's width to
    /// compute wrap layout correctly.
    ///
    /// Bug: When scrolling in a horizontal split pane, the WrapLayout was created
    /// with full-window width, causing it to underestimate how many screen rows
    /// would result from line wrapping in the narrower pane.
    #[test]
    fn test_hsplit_scroll_uses_pane_width_for_wrap() {
        use crate::left_rail::RAIL_WIDTH;
        use crate::pane_layout::SplitDirection;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let metrics = test_font_metrics();
        let mut state = EditorState::empty(metrics);
        // Window: 800x600 total
        state.update_viewport_dimensions(800.0, 600.0);

        let line_height = metrics.line_height as f32;
        let advance_width = metrics.advance_width as f32;

        // Content area: (800 - RAIL_WIDTH) x 600
        let content_width = 800.0 - RAIL_WIDTH;

        // Get workspace
        let ws = state.editor.active_workspace_mut().unwrap();
        let ws_id = ws.id;

        // Create a pane with lines that are 100 chars long
        // These lines WILL wrap in a narrow pane but WON'T wrap in a full-width pane
        let mut pane1 = Pane::new(1, ws_id);
        let mut tab1 = crate::workspace::Tab::empty_file(100, line_height);
        if let Some(buf) = tab1.as_text_buffer_mut() {
            // Create 20 lines, each 100 characters long
            for i in 0..20 {
                let line = format!("{:0>100}\n", i); // 100 digits + newline
                buf.insert_str(&line);
            }
            buf.set_cursor(lite_edit_buffer::Position::new(0, 0));
        }
        pane1.add_tab(tab1);

        // Create a second empty pane
        let mut pane2 = Pane::new(2, ws_id);
        let tab2 = crate::workspace::Tab::empty_file(101, line_height);
        pane2.add_tab(tab2);

        // Create horizontal split (pane1 left, pane2 right) - each pane gets half width
        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };
        ws.active_pane_id = 1;

        // Sync viewports
        state.sync_pane_viewports();

        // Calculate expected pane dimensions
        // Left pane width: content_width * 0.5
        // Pane content height: 600 - TAB_BAR_HEIGHT
        let pane_width = content_width * 0.5;
        let pane_content_height = 600.0 - TAB_BAR_HEIGHT;

        // Verify the helper returns correct pane dimensions
        let dims = state.get_pane_content_dimensions(1);
        assert!(dims.is_some(), "Should find pane 1 dimensions");
        let (content_height, returned_width) = dims.unwrap();
        assert!(
            (returned_width - pane_width).abs() < 0.01,
            "Pane width should be {} but got {}",
            pane_width,
            returned_width
        );

        // Calculate how many chars fit per row in the narrow pane
        let cols_per_row_narrow = (pane_width / advance_width).floor() as usize;
        // Calculate how many chars fit per row in full-window width
        let cols_per_row_full = (content_width / advance_width).floor() as usize;

        // With 100-char lines:
        // - In narrow pane: lines should wrap (100 chars > cols_per_row_narrow)
        // - In full-width: lines might not wrap (depending on exact width)
        // The key is: narrow pane should have MORE screen rows than buffer lines

        let line_chars = 100;
        let screen_rows_narrow = (line_chars + cols_per_row_narrow - 1) / cols_per_row_narrow;
        let screen_rows_full = (line_chars + cols_per_row_full - 1) / cols_per_row_full;

        // Verify that narrow pane requires more screen rows per line (more wrapping)
        assert!(
            screen_rows_narrow >= screen_rows_full,
            "Narrow pane should have at least as many screen rows per line ({}) as full width ({})",
            screen_rows_narrow,
            screen_rows_full
        );

        // Now verify scrolling works correctly with the narrower pane
        // Get line count
        let line_count = {
            let ws = state.editor.active_workspace().unwrap();
            let pane = ws.pane_root.get_pane(1).unwrap();
            let tab = pane.active_tab().unwrap();
            tab.as_text_buffer().unwrap().line_count()
        };

        // Calculate total screen rows in narrow pane (accounting for wrapping)
        let total_screen_rows = line_count * screen_rows_narrow;
        let visible_rows = (pane_content_height / line_height).floor() as usize;
        let max_scroll_rows = total_screen_rows.saturating_sub(visible_rows);

        // Try to scroll to the end
        let large_scroll_px = (total_screen_rows as f64) * (line_height as f64);
        state.handle_scroll(ScrollDelta::new(0.0, large_scroll_px));

        // Get the scroll offset
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.pane_root.get_pane(1).unwrap();
        let tab = pane.active_tab().unwrap();
        let scroll_offset_px = tab.viewport.scroll_offset_px();

        // The key invariant: with the fix, we should be able to scroll further
        // than we would with the bug (which used full-window width for wrapping).
        //
        // With full-window width:
        // - cols_per_row_full = floor(744 / 8) = 93
        // - screen_rows_per_line = ceil(100 / 93) = 2
        // - total_screen_rows_buggy = 20 * 2 = 40
        // - max_scroll_rows_buggy = 40 - visible_rows
        //
        // With pane width (fix):
        // - cols_per_row_narrow = floor(372 / 8) = 46
        // - screen_rows_per_line = ceil(100 / 46) = 3
        // - total_screen_rows_fixed = 20 * 3 = 60
        // - max_scroll_rows_fixed = 60 - visible_rows
        //
        // The fix allows scrolling ~20 more rows (one line of content difference
        // per line that wraps more).
        let total_screen_rows_buggy = line_count * screen_rows_full;
        let max_scroll_rows_buggy = total_screen_rows_buggy.saturating_sub(visible_rows);
        let buggy_max_offset = (max_scroll_rows_buggy as f32) * line_height;

        // The scroll offset should exceed what the buggy calculation would allow
        // (if the bug existed, scroll_offset_px would be clamped to buggy_max_offset)
        assert!(
            scroll_offset_px > buggy_max_offset,
            "Scroll offset {} should exceed buggy max {} (which uses full-window width). \
             This indicates the fix is working - we can scroll further with correct pane width.",
            scroll_offset_px,
            buggy_max_offset
        );

        // Also verify we're not wildly off from reasonable bounds
        let fixed_max_offset = (max_scroll_rows as f32) * line_height;
        assert!(
            scroll_offset_px <= fixed_max_offset + line_height,
            "Scroll offset {} should not greatly exceed calculated max {} (pane width).",
            scroll_offset_px,
            fixed_max_offset
        );
    }

    // =========================================================================
    // Chunk: docs/chunks/dirty_tab_close_confirm - EditorState integration tests
    // =========================================================================

    #[test]
    fn test_close_dirty_tab_opens_confirm_dialog() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Type some text to make the buffer dirty
        state.handle_key(KeyEvent::char('h'));
        state.handle_key(KeyEvent::char('e'));
        state.handle_key(KeyEvent::char('l'));
        state.handle_key(KeyEvent::char('l'));
        state.handle_key(KeyEvent::char('o'));
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);

        // Try to close the dirty tab
        state.close_tab(0);

        // Should open confirm dialog instead of closing
        assert!(state.confirm_dialog.is_some());
        assert_eq!(
            state.confirm_dialog.as_ref().unwrap().prompt,
            "Abandon unsaved changes?"
        );
        // Tab should still be there
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
    }

    // Chunk: docs/chunks/generic_yes_no_modal - Updated to use confirm_context
    #[test]
    fn test_close_dirty_tab_sets_confirm_context() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make the buffer dirty
        state.handle_key(KeyEvent::char('x'));
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);

        // Get the pane_id before closing
        let expected_pane_id = state.editor.active_workspace().unwrap().active_pane_id;

        // Try to close the dirty tab
        state.close_tab(0);

        // confirm_context should have the CloseDirtyTab variant with pane_id and tab index
        assert!(state.confirm_context.is_some());
        match state.confirm_context.as_ref().unwrap() {
            ConfirmDialogContext::CloseDirtyTab { pane_id, tab_idx } => {
                assert_eq!(*pane_id, expected_pane_id);
                assert_eq!(*tab_idx, 0);
            }
            _ => panic!("Expected CloseDirtyTab context"),
        }
    }

    #[test]
    fn test_close_dirty_tab_sets_focus_to_confirm_dialog() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Start with buffer focus
        assert_eq!(state.focus, EditorFocus::Buffer);

        // Make the buffer dirty
        state.handle_key(KeyEvent::char('x'));

        // Try to close the dirty tab
        state.close_tab(0);

        // Focus should be on confirm dialog
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
    }

    #[test]
    fn test_close_clean_tab_still_closes_immediately() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Add a second tab so we can close one
        {
            let tab_id = state.editor.gen_tab_id();
            let line_height = state.editor.line_height();
            let tab = crate::workspace::Tab::empty_file(tab_id, line_height);
            state.editor.active_workspace_mut().unwrap().add_tab(tab);
        }
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 2);

        // Close the clean first tab (should close immediately)
        state.close_tab(0);

        // Tab should be removed, no confirm dialog
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        assert!(state.confirm_dialog.is_none());
        // Chunk: docs/chunks/generic_yes_no_modal - Updated to use confirm_context
        assert!(state.confirm_context.is_none());
        assert_eq!(state.focus, EditorFocus::Buffer);
    }

    #[test]
    fn test_confirm_dialog_escape_closes_dialog_keeps_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make buffer dirty
        state.handle_key(KeyEvent::char('x'));

        // Try to close dirty tab (opens confirm dialog)
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
        assert!(state.confirm_dialog.is_some());

        // Press Escape to cancel
        let escape = KeyEvent::new(Key::Escape, Modifiers::default());
        state.handle_key(escape);

        // Dialog should be closed, tab still there, focus back to buffer
        assert!(state.confirm_dialog.is_none());
        // Chunk: docs/chunks/generic_yes_no_modal - Updated to use confirm_context
        assert!(state.confirm_context.is_none());
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        // Tab should still be dirty
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
    }

    #[test]
    fn test_confirm_dialog_enter_on_cancel_closes_dialog_keeps_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make buffer dirty
        state.handle_key(KeyEvent::char('x'));

        // Try to close dirty tab (opens confirm dialog)
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Default selection is Cancel, press Enter
        let enter = KeyEvent::new(Key::Return, Modifiers::default());
        state.handle_key(enter);

        // Dialog should be closed, tab still there
        assert!(state.confirm_dialog.is_none());
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        // Tab should still be dirty
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
    }

    #[test]
    fn test_confirm_dialog_tab_then_enter_closes_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make buffer dirty
        state.handle_key(KeyEvent::char('x'));
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);

        // Try to close dirty tab (opens confirm dialog)
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Press Tab to select Abandon
        let tab = KeyEvent::new(Key::Tab, Modifiers::default());
        state.handle_key(tab);

        // Verify selection changed (dialog still open)
        assert!(state.confirm_dialog.is_some());
        assert_eq!(
            state.confirm_dialog.as_ref().unwrap().selected,
            crate::confirm_dialog::ConfirmButton::Abandon
        );

        // Press Enter to confirm
        let enter = KeyEvent::new(Key::Return, Modifiers::default());
        state.handle_key(enter);

        // Dialog should be closed, tab should be closed (replaced with empty)
        // Note: closing the last tab replaces it with an empty tab
        assert!(state.confirm_dialog.is_none());
        // Chunk: docs/chunks/generic_yes_no_modal - Updated to use confirm_context
        assert!(state.confirm_context.is_none());
        assert_eq!(state.focus, EditorFocus::Buffer);
        // The old dirty tab was closed (replaced with new empty tab)
        assert!(!state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
    }

    #[test]
    fn test_cmd_p_blocked_during_confirm_dialog() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make buffer dirty and open confirm dialog
        state.handle_key(KeyEvent::char('x'));
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Try to open file picker with Cmd+P
        let cmd_p = KeyEvent::new(
            Key::Char('p'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_p);

        // Should still be in ConfirmDialog, not Selector
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
        assert!(state.active_selector.is_none());
    }

    #[test]
    fn test_cmd_f_blocked_during_confirm_dialog() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(160.0);

        // Make buffer dirty and open confirm dialog
        state.handle_key(KeyEvent::char('x'));
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Try to open find strip with Cmd+F
        let cmd_f = KeyEvent::new(
            Key::Char('f'),
            Modifiers {
                command: true,
                ..Default::default()
            },
        );
        state.handle_key(cmd_f);

        // Should still be in ConfirmDialog, not FindInFile
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
        assert!(state.find_mini_buffer.is_none());
    }

    // =========================================================================
    // Terminal Resize Sync Tests (Chunk: docs/chunks/terminal_resize_sync)
    // =========================================================================

    /// Tests that sync_pane_viewports resizes the terminal when viewport dimensions change.
    #[test]
    fn test_sync_pane_viewports_resizes_terminal() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::left_rail::RAIL_WIDTH;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Get initial terminal size
        let initial_size = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        // Resize window (double the height)
        state.update_viewport_dimensions(800.0, 1200.0 + TAB_BAR_HEIGHT);

        // Terminal should have more rows now
        let new_size = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        // With double the content height, we should have roughly double the rows
        assert!(
            new_size.1 > initial_size.1,
            "Terminal rows should increase after resize: was {:?}, now {:?}",
            initial_size,
            new_size
        );
    }

    /// Tests that terminal resize works correctly when a pane splits (reducing the content area).
    #[test]
    fn test_terminal_resize_on_split() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::pane_layout::Direction;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Get initial terminal size
        let initial_rows = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size().1 // rows
        };

        // Create a second tab so we can move it to trigger a split
        state.new_tab();

        // Move the new tab to create a vertical split (which reduces height per pane)
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.move_active_tab(Direction::Down);
        }

        // Sync viewports after split
        state.sync_pane_viewports();

        // Terminal should have fewer rows now (roughly half, since the pane is split vertically)
        let new_rows = {
            // The terminal tab is in the first pane (pane_id 0 or the original pane)
            // We need to find it
            let ws = state.editor.active_workspace().unwrap();
            for pane in ws.all_panes() {
                for tab in &pane.tabs {
                    if let Some(term) = tab.as_terminal_buffer() {
                        return assert!(
                            term.size().1 < initial_rows,
                            "Terminal rows should decrease after split: was {}, now {}",
                            initial_rows,
                            term.size().1
                        );
                    }
                }
            }
            panic!("Terminal tab not found after split");
        };
    }

    /// Tests that sync_pane_viewports doesn't call terminal resize when dimensions haven't changed.
    #[test]
    fn test_terminal_resize_skipped_when_unchanged() {
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a terminal tab
        state.new_terminal_tab();

        // Get initial terminal size
        let initial_size = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        // Call sync_pane_viewports again with the same dimensions
        state.sync_pane_viewports();

        // Size should be unchanged
        let new_size = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        assert_eq!(initial_size, new_size, "Terminal size should not change");
    }

    /// Tests that the terminal size matches the expected dimensions based on pane geometry.
    #[test]
    fn test_terminal_size_matches_pane_geometry() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::left_rail::RAIL_WIDTH;

        let mut state = EditorState::empty(test_font_metrics());
        let fm = test_font_metrics();

        // Set up viewport
        let view_width = 800.0;
        let view_height = 600.0 + TAB_BAR_HEIGHT;
        state.update_viewport_dimensions(view_width, view_height);

        // Create a terminal tab
        state.new_terminal_tab();

        // Calculate expected terminal dimensions
        let content_width = view_width - RAIL_WIDTH;
        let content_height = view_height - TAB_BAR_HEIGHT;
        let expected_cols = (content_width as f64 / fm.advance_width).floor() as usize;
        let expected_rows = (content_height as f64 / fm.line_height).floor() as usize;

        // Get actual terminal size
        let (actual_cols, actual_rows) = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        assert_eq!(
            actual_cols, expected_cols,
            "Terminal columns should match pane width"
        );
        assert_eq!(
            actual_rows, expected_rows,
            "Terminal rows should match pane height"
        );
    }

    // =========================================================================
    // Confirm dialog mouse interaction tests
    // Chunk: docs/chunks/generic_yes_no_modal - Mouse click tests for confirm dialog
    // =========================================================================

    /// Helper to convert screen-space y (y=0 at top) to NSView y (y=0 at bottom).
    /// handle_mouse expects NSView coordinates.
    fn screen_to_nsview_y(screen_y: f64, view_height: f32) -> f64 {
        view_height as f64 - screen_y
    }

    #[test]
    fn test_mouse_click_cancel_button_closes_dialog() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(800.0); // Set a reasonable view size
        state.view_width = 800.0;
        state.view_height = 600.0;

        // Make buffer dirty and open confirm dialog
        state.handle_key(KeyEvent::char('x'));
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
        assert!(state.confirm_dialog.is_some());

        // Calculate geometry to find cancel button position (in screen space)
        let dialog = state.confirm_dialog.as_ref().unwrap();
        let line_height = state.font_metrics.line_height as f32;
        let glyph_width = state.font_metrics.advance_width as f32;
        let geometry = calculate_confirm_dialog_geometry(
            state.view_width,
            state.view_height,
            line_height,
            glyph_width,
            dialog,
        );

        // Click in the center of the cancel button
        // Convert from screen space to NSView space for handle_mouse
        let screen_x = geometry.cancel_button_x + geometry.button_width / 2.0;
        let screen_y = geometry.buttons_y + geometry.button_height / 2.0;
        let nsview_y = screen_to_nsview_y(screen_y as f64, state.view_height);
        let click = MouseEvent {
            kind: MouseEventKind::Down,
            position: (screen_x as f64, nsview_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click);

        // Dialog should be closed, tab still there, focus back to buffer
        assert!(state.confirm_dialog.is_none());
        assert!(state.confirm_context.is_none());
        assert_eq!(state.focus, EditorFocus::Buffer);
        assert_eq!(state.editor.active_workspace().unwrap().tab_count(), 1);
        // Tab should still be dirty (not closed)
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
    }

    #[test]
    fn test_mouse_click_confirm_button_closes_tab() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(800.0);
        state.view_width = 800.0;
        state.view_height = 600.0;

        // Make buffer dirty and open confirm dialog
        state.handle_key(KeyEvent::char('x'));
        assert!(state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);

        // Calculate geometry to find confirm button position (in screen space)
        let dialog = state.confirm_dialog.as_ref().unwrap();
        let line_height = state.font_metrics.line_height as f32;
        let glyph_width = state.font_metrics.advance_width as f32;
        let geometry = calculate_confirm_dialog_geometry(
            state.view_width,
            state.view_height,
            line_height,
            glyph_width,
            dialog,
        );

        // Click in the center of the confirm button (abandon_button_x is the confirm button)
        // Convert from screen space to NSView space for handle_mouse
        let screen_x = geometry.abandon_button_x + geometry.button_width / 2.0;
        let screen_y = geometry.buttons_y + geometry.button_height / 2.0;
        let nsview_y = screen_to_nsview_y(screen_y as f64, state.view_height);
        let click = MouseEvent {
            kind: MouseEventKind::Down,
            position: (screen_x as f64, nsview_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click);

        // Dialog should be closed, tab should be closed (replaced with empty)
        assert!(state.confirm_dialog.is_none());
        assert!(state.confirm_context.is_none());
        assert_eq!(state.focus, EditorFocus::Buffer);
        // The old dirty tab was closed (replaced with new empty tab)
        assert!(!state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
    }

    #[test]
    fn test_mouse_click_outside_buttons_does_nothing() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(800.0);
        state.view_width = 800.0;
        state.view_height = 600.0;

        // Make buffer dirty and open confirm dialog
        state.handle_key(KeyEvent::char('x'));
        state.close_tab(0);
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
        assert!(state.confirm_dialog.is_some());

        // Click somewhere in the dialog panel but not on buttons (top area)
        let dialog = state.confirm_dialog.as_ref().unwrap();
        let line_height = state.font_metrics.line_height as f32;
        let glyph_width = state.font_metrics.advance_width as f32;
        let geometry = calculate_confirm_dialog_geometry(
            state.view_width,
            state.view_height,
            line_height,
            glyph_width,
            dialog,
        );

        // Click in the prompt area (above the buttons in screen space)
        // Convert from screen space to NSView space for handle_mouse
        let screen_x = geometry.panel_x + geometry.panel_width / 2.0;
        let screen_y = geometry.prompt_y - 5.0; // Above buttons, in prompt area
        let nsview_y = screen_to_nsview_y(screen_y as f64, state.view_height);
        let click = MouseEvent {
            kind: MouseEventKind::Down,
            position: (screen_x as f64, nsview_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click);

        // Dialog should still be open
        assert!(state.confirm_dialog.is_some());
        assert_eq!(state.focus, EditorFocus::ConfirmDialog);
    }

    #[test]
    fn test_mouse_click_updates_selection_before_close() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(800.0);
        state.view_width = 800.0;
        state.view_height = 600.0;

        // Make buffer dirty and open confirm dialog
        state.handle_key(KeyEvent::char('x'));
        state.close_tab(0);

        // Verify default selection is Cancel
        assert_eq!(
            state.confirm_dialog.as_ref().unwrap().selected,
            crate::confirm_dialog::ConfirmButton::Cancel
        );

        // Calculate geometry to find confirm button position (in screen space)
        let dialog = state.confirm_dialog.as_ref().unwrap();
        let line_height = state.font_metrics.line_height as f32;
        let glyph_width = state.font_metrics.advance_width as f32;
        let geometry = calculate_confirm_dialog_geometry(
            state.view_width,
            state.view_height,
            line_height,
            glyph_width,
            dialog,
        );

        // Click on confirm button - before closing, selection should change to Abandon
        // (The dialog gets closed immediately, so we can't observe the selection change,
        // but the test verifies that clicking confirm button works correctly)
        // Convert from screen space to NSView space for handle_mouse
        let screen_x = geometry.abandon_button_x + geometry.button_width / 2.0;
        let screen_y = geometry.buttons_y + geometry.button_height / 2.0;
        let nsview_y = screen_to_nsview_y(screen_y as f64, state.view_height);
        let click = MouseEvent {
            kind: MouseEventKind::Down,
            position: (screen_x as f64, nsview_y),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        state.handle_mouse(click);

        // Dialog should be closed and the tab should be closed
        assert!(state.confirm_dialog.is_none());
        assert!(!state.editor.active_workspace().unwrap().active_tab().unwrap().dirty);
    }

    // =========================================================================
    // Welcome Screen Scroll Tests (Chunk: docs/chunks/welcome_scroll)
    // =========================================================================

    #[test]
    fn test_welcome_screen_scroll_updates_offset() {
        // An empty file tab should route scroll to welcome_scroll_offset_px
        let mut state = EditorState::empty(test_font_metrics());
        // Ensure the active tab is an empty file tab (welcome screen)
        assert!(state.editor.should_show_welcome_screen());

        state.handle_scroll(ScrollDelta::new(0.0, 50.0));

        let offset = state
            .editor
            .active_workspace()
            .unwrap()
            .active_tab()
            .unwrap()
            .welcome_scroll_offset_px();
        assert!((offset - 50.0).abs() < 0.001, "expected 50.0, got {offset}");
        assert!(state.is_dirty());
    }

    #[test]
    fn test_welcome_screen_scroll_clamps_at_zero() {
        // Scrolling up from offset 0 should stay at 0
        let mut state = EditorState::empty(test_font_metrics());
        assert!(state.editor.should_show_welcome_screen());

        // Scroll up (negative dy)
        state.handle_scroll(ScrollDelta::new(0.0, -100.0));

        let offset = state
            .editor
            .active_workspace()
            .unwrap()
            .active_tab()
            .unwrap()
            .welcome_scroll_offset_px();
        assert!((offset - 0.0).abs() < 0.001, "expected 0.0, got {offset}");
    }

    #[test]
    fn test_non_welcome_scroll_uses_viewport() {
        // A non-empty file tab should NOT update welcome_scroll_offset_px
        let mut state = EditorState::new(
            lite_edit_buffer::TextBuffer::from_str("hello world"),
            test_font_metrics(),
        );
        state.update_viewport_size(320.0);
        assert!(!state.editor.should_show_welcome_screen());

        state.handle_scroll(ScrollDelta::new(0.0, 50.0));

        let offset = state
            .editor
            .active_workspace()
            .unwrap()
            .active_tab()
            .unwrap()
            .welcome_scroll_offset_px();
        assert!((offset - 0.0).abs() < 0.001, "welcome offset should remain 0 for non-welcome tab");
    }

    // =========================================================================
    // File Drop Tests (Chunk: docs/chunks/dragdrop_file_paste)
    // =========================================================================

    #[test]
    fn test_file_drop_inserts_shell_escaped_path_in_buffer() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Drop a single file
        state.handle_file_drop(vec!["/Users/test/file.txt".to_string()]);

        // Should be shell-escaped with single quotes
        assert_eq!(state.buffer().content(), "'/Users/test/file.txt'");
    }

    #[test]
    fn test_file_drop_escapes_spaces() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Drop a file with spaces in the name
        state.handle_file_drop(vec!["/Users/test/my file.txt".to_string()]);

        // Spaces inside single quotes don't need extra escaping
        assert_eq!(state.buffer().content(), "'/Users/test/my file.txt'");
    }

    #[test]
    fn test_file_drop_escapes_single_quotes() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Drop a file with single quote in the name
        state.handle_file_drop(vec!["/Users/test/foo's.txt".to_string()]);

        // Single quotes escaped with the '\'' pattern
        assert_eq!(state.buffer().content(), "'/Users/test/foo'\\''s.txt'");
    }

    #[test]
    fn test_file_drop_multiple_files() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Drop multiple files
        state.handle_file_drop(vec![
            "/path/to/file1.txt".to_string(),
            "/path/to/file2.txt".to_string(),
        ]);

        // Should be space-separated
        assert_eq!(
            state.buffer().content(),
            "'/path/to/file1.txt' '/path/to/file2.txt'"
        );
    }

    #[test]
    fn test_file_drop_empty_paths_is_noop() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Drop no files
        state.handle_file_drop(vec![]);

        // Buffer should remain empty
        assert!(state.buffer().is_empty());
    }

    #[test]
    fn test_file_drop_ignored_when_selector_focused() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Simulate selector focus
        state.focus = EditorFocus::Selector;

        // Try to drop a file
        state.handle_file_drop(vec!["/Users/test/file.txt".to_string()]);

        // Buffer should remain empty because selector mode ignores drops
        assert!(state.buffer().is_empty());
    }

    #[test]
    fn test_file_drop_marks_tab_dirty() {
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_size(320.0);

        // Initially the tab should not be dirty
        let tab = state
            .editor
            .active_workspace()
            .unwrap()
            .active_tab()
            .unwrap();
        assert!(!tab.dirty);

        // Drop a file
        state.handle_file_drop(vec!["/path/to/file.txt".to_string()]);

        // Tab should now be marked dirty
        let tab = state
            .editor
            .active_workspace()
            .unwrap()
            .active_tab()
            .unwrap();
        assert!(tab.dirty, "Tab should be marked dirty after file drop");
    }

    // =========================================================================
    // Multi-Pane Tab Click Routing Tests (Chunk: docs/chunks/split_tab_click)
    // =========================================================================

    /// Helper to create a multi-pane EditorState with a vertical split (top/bottom).
    ///
    /// Layout:
    /// ```text
    /// +---------------+
    /// |   Top Pane    |  (pane_id=1, tabs: "top1.rs", "top2.rs")
    /// +---------------+
    /// |  Bottom Pane  |  (pane_id=2, tabs: "bottom1.rs", "bottom2.rs")
    /// +---------------+
    /// ```
    fn create_vertical_split_state() -> EditorState {
        use crate::pane_layout::{Pane, PaneLayoutNode, SplitDirection};
        use crate::workspace::Tab;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let line_height = test_font_metrics().line_height as f32;
        let pane1_id = 1u64;
        let pane2_id = 2u64;

        // Top pane with 2 tabs
        let mut pane1 = Pane::new(pane1_id, 1);
        pane1.add_tab(Tab::new_file(
            100,
            lite_edit_buffer::TextBuffer::from_str("top1 content"),
            "top1.rs".to_string(),
            None,
            line_height,
        ));
        pane1.add_tab(Tab::new_file(
            101,
            lite_edit_buffer::TextBuffer::from_str("top2 content"),
            "top2.rs".to_string(),
            None,
            line_height,
        ));
        pane1.switch_tab(0); // Make first tab active

        // Bottom pane with 2 tabs
        let mut pane2 = Pane::new(pane2_id, 1);
        pane2.add_tab(Tab::new_file(
            102,
            lite_edit_buffer::TextBuffer::from_str("bottom1 content"),
            "bottom1.rs".to_string(),
            None,
            line_height,
        ));
        pane2.add_tab(Tab::new_file(
            103,
            lite_edit_buffer::TextBuffer::from_str("bottom2 content"),
            "bottom2.rs".to_string(),
            None,
            line_height,
        ));
        pane2.switch_tab(0); // Make first tab active

        // Create vertical split layout (top | bottom)
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.pane_root = PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane1)),
                second: Box::new(PaneLayoutNode::Leaf(pane2)),
            };
            ws.active_pane_id = pane1_id; // Focus on top pane
        }

        state
    }

    /// Helper to create a multi-pane EditorState with a horizontal split (left/right).
    ///
    /// Layout:
    /// ```text
    /// +-------+-------+
    /// | Left  | Right |
    /// | Pane  | Pane  |
    /// +-------+-------+
    /// ```
    fn create_horizontal_split_state() -> EditorState {
        use crate::pane_layout::{Pane, PaneLayoutNode, SplitDirection};
        use crate::workspace::Tab;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        let line_height = test_font_metrics().line_height as f32;
        let pane1_id = 1u64;
        let pane2_id = 2u64;

        // Left pane with 2 tabs
        let mut pane1 = Pane::new(pane1_id, 1);
        pane1.add_tab(Tab::new_file(
            100,
            lite_edit_buffer::TextBuffer::from_str("left1 content"),
            "left1.rs".to_string(),
            None,
            line_height,
        ));
        pane1.add_tab(Tab::new_file(
            101,
            lite_edit_buffer::TextBuffer::from_str("left2 content"),
            "left2.rs".to_string(),
            None,
            line_height,
        ));
        pane1.switch_tab(0);

        // Right pane with 2 tabs
        let mut pane2 = Pane::new(pane2_id, 1);
        pane2.add_tab(Tab::new_file(
            102,
            lite_edit_buffer::TextBuffer::from_str("right1 content"),
            "right1.rs".to_string(),
            None,
            line_height,
        ));
        pane2.add_tab(Tab::new_file(
            103,
            lite_edit_buffer::TextBuffer::from_str("right2 content"),
            "right2.rs".to_string(),
            None,
            line_height,
        ));
        pane2.switch_tab(0);

        // Create horizontal split layout (left | right)
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.pane_root = PaneLayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane1)),
                second: Box::new(PaneLayoutNode::Leaf(pane2)),
            };
            ws.active_pane_id = pane1_id; // Focus on left pane
        }

        state
    }

    #[test]
    fn test_tab_click_vertical_split_top_pane() {
        // Clicking a tab in the top pane should activate that tab in the top pane only
        let mut state = create_vertical_split_state();

        // Layout: 800x600 window, RAIL_WIDTH=56, TAB_BAR_HEIGHT=32
        // Pane bounds: (56, 0, 744, 600)
        // Vertical split at 0.5 ratio:
        // - Top pane: x=56, y=0, width=744, height=300
        // - Bottom pane: x=56, y=300, width=744, height=300
        //
        // Tab geometry (calculated via calculate_pane_tab_bar_geometry):
        // Tab width for "top1.rs" = 118 px
        // First tab: x=[56, 174)
        // Second tab: x=[175, 293) (with 1px spacing)

        // Click on the second tab in the TOP pane (tab index 1)
        let click_x = 200.0; // Well inside second tab [175, 293)
        let click_y = 16.0;  // Middle of top pane's tab bar [0, 32)

        state.handle_tab_bar_click(click_x, click_y);

        // Verify: top pane should now have tab index 1 active
        let ws = state.editor.active_workspace().unwrap();
        let top_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(top_pane.active_tab, 1, "Top pane should have second tab (index 1) active");

        // Verify: bottom pane should still have tab index 0 active
        let bottom_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(bottom_pane.active_tab, 0, "Bottom pane should still have first tab (index 0) active");
    }

    #[test]
    fn test_tab_click_vertical_split_bottom_pane() {
        // Clicking a tab in the bottom pane should activate that tab in the bottom pane only
        // and switch focus to the bottom pane
        let mut state = create_vertical_split_state();

        // Layout: 800x600 window
        // Vertical split at 0.5 ratio with bounds (32, 0, 768, 600):
        // - Top pane: x=32, y=0, width=768, height=300
        // - Bottom pane: x=32, y=300, width=768, height=300
        //
        // Bottom pane's tab bar is at y ∈ [300, 332)
        // Tab width for "bottom1.rs" (10 chars): 12+6+4+80+4+16+12 = 134 (using char_width*10=80)
        // First tab: x ∈ [32, 166)
        // Second tab starts at: 32 + 134 + 1 = 167

        // Click on the second tab in the BOTTOM pane (tab index 1)
        let click_x = 185.0; // Well inside second tab
        let click_y = 316.0; // Middle of bottom pane's tab bar (y=300 + 16)

        state.handle_tab_bar_click(click_x, click_y);

        // Verify: focus should have switched to bottom pane
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.active_pane_id, 2, "Focus should have switched to bottom pane");

        // Verify: bottom pane should now have tab index 1 active
        let bottom_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(bottom_pane.active_tab, 1, "Bottom pane should have second tab (index 1) active");

        // Verify: top pane should still have tab index 0 active
        let top_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(top_pane.active_tab, 0, "Top pane should still have first tab (index 0) active");
    }

    #[test]
    fn test_tab_click_horizontal_split_left_pane() {
        // Clicking a tab in the left pane should activate that tab in the left pane only
        let mut state = create_horizontal_split_state();

        // Layout: 800x600 window, RAIL_WIDTH=56
        // Horizontal split at 0.5 ratio with bounds (56, 0, 744, 600):
        // - Left pane: x=56, y=0, width=372, height=600
        // - Right pane: x=428, y=0, width=372, height=600
        //
        // Tab width for "left1.rs" (8 chars) = 118
        // First tab: x=[56, 174)
        // Second tab: x=[175, 293)

        // Click on the second tab in the LEFT pane (tab index 1)
        let click_x = 200.0; // Well inside second tab [175, 293)
        let click_y = 16.0;  // Middle of left pane's tab bar

        state.handle_tab_bar_click(click_x, click_y);

        // Verify: left pane should now have tab index 1 active
        let ws = state.editor.active_workspace().unwrap();
        let left_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(left_pane.active_tab, 1, "Left pane should have second tab (index 1) active");

        // Verify: right pane should still have tab index 0 active
        let right_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(right_pane.active_tab, 0, "Right pane should still have first tab (index 0) active");
    }

    #[test]
    fn test_tab_click_horizontal_split_right_pane() {
        // Clicking a tab in the right pane should activate that tab in the right pane only
        // and switch focus to the right pane
        let mut state = create_horizontal_split_state();

        // Layout: 800x600 window
        // Horizontal split at 0.5 ratio with bounds (32, 0, 768, 600):
        // - Left pane: x=32, width=384, so right edge at 416
        // - Right pane: x=416, width=384
        //
        // Tab width for "right1.rs" (9 chars): 12+6+4+72+4+16+12 = 126
        // First tab in right pane: x ∈ [416, 542)
        // Second tab starts at: 416 + 126 + 1 = 543

        // Click on the second tab in the RIGHT pane (tab index 1)
        let click_x = 560.0; // Well inside second tab (543 + ~17)
        let click_y = 16.0;  // Middle of right pane's tab bar

        state.handle_tab_bar_click(click_x, click_y);

        // Verify: focus should have switched to right pane
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.active_pane_id, 2, "Focus should have switched to right pane");

        // Verify: right pane should now have tab index 1 active
        let right_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(right_pane.active_tab, 1, "Right pane should have second tab (index 1) active");

        // Verify: left pane should still have tab index 0 active
        let left_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(left_pane.active_tab, 0, "Left pane should still have first tab (index 0) active");
    }

    #[test]
    fn test_tab_click_inactive_pane_switches_focus() {
        // Clicking a tab in an inactive pane should switch focus to that pane
        let mut state = create_horizontal_split_state();

        // Initially, left pane (id=1) is focused
        assert_eq!(
            state.editor.active_workspace().unwrap().active_pane_id,
            1,
            "Initially left pane should be focused"
        );

        // Click on the first tab in the RIGHT pane (inactive)
        // Right pane starts at x=416, first tab is x ∈ [416, 542)
        let click_x = 480.0; // Inside right pane's first tab
        let click_y = 16.0;  // Middle of tab bar

        state.handle_tab_bar_click(click_x, click_y);

        // Verify: focus should have switched to right pane
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.active_pane_id, 2, "Focus should have switched to right pane");
    }

    #[test]
    fn test_single_pane_tab_click_still_works() {
        // Regression test: single-pane layout should continue to work
        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Add a second tab to the default single pane
        let tab2 = crate::workspace::Tab::new_file(
            200,
            lite_edit_buffer::TextBuffer::from_str("second tab"),
            "second.rs".to_string(),
            None,
            test_font_metrics().line_height as f32,
        );

        if let Some(ws) = state.editor.active_workspace_mut() {
            if let Some(pane) = ws.active_pane_mut() {
                pane.add_tab(tab2);
                pane.switch_tab(0); // Start with first tab active
            }
        }

        // Click on the second tab
        // Single pane bounds: (RAIL_WIDTH=56, 0, 744, 600)
        // First tab is the default "untitled" tab (8 chars) = width 118, x=[56, 174)
        // Second tab "second.rs" (9 chars) = width 126, x=[175, 301)
        let click_x = 200.0; // Inside second tab [175, 301)
        let click_y = 16.0;  // Middle of tab bar [0, 32)

        state.handle_tab_bar_click(click_x, click_y);

        // Verify: second tab should be active
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.active_pane().unwrap();
        assert_eq!(pane.active_tab, 1, "Second tab should be active");
    }

    // =========================================================================
    // Chunk: docs/chunks/pane_tabs_interaction - Full click dispatch path tests
    // =========================================================================
    //
    // These tests verify that handle_mouse correctly routes clicks to
    // handle_tab_bar_click for non-top-left panes in split layouts.
    // The existing split_tab_click tests call handle_tab_bar_click directly;
    // these tests exercise the full dispatch path through handle_mouse.

    /// Tests that handle_mouse routes clicks to the bottom pane's tab bar
    /// in a vertical split layout.
    ///
    /// This is a regression test for the bug where clicks at y > TAB_BAR_HEIGHT
    /// were not routed to handle_tab_bar_click, causing non-top-left pane tabs
    /// to be unresponsive.
    #[test]
    fn test_handle_mouse_routes_to_bottom_pane_tab_bar() {
        use crate::input::MouseEventKind;

        let mut state = create_vertical_split_state();

        // Layout: 800x600 window
        // Vertical split at 0.5 ratio with bounds (RAIL_WIDTH=56, 0, 744, 600):
        // - Top pane: x=56, y=0, width=744, height=300
        // - Bottom pane: x=56, y=300, width=744, height=300
        //
        // Bottom pane's tab bar is at y ∈ [300, 332) in screen space
        // NSView coords (origin at bottom-left): y = view_height - screen_y
        // For y=316 in screen space: nsview_y = 600 - 316 = 284

        // Click on the second tab in the BOTTOM pane via handle_mouse
        let click_x = 185.0; // Well inside second tab
        let nsview_y = 284.0; // 600 - 316 (middle of bottom pane's tab bar in NSView coords)

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: focus should have switched to bottom pane
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.active_pane_id, 2, "Focus should have switched to bottom pane");

        // Verify: bottom pane should now have tab index 1 active
        let bottom_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(bottom_pane.active_tab, 1, "Bottom pane should have second tab (index 1) active");

        // Verify: top pane should still have tab index 0 active
        let top_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(top_pane.active_tab, 0, "Top pane should still have first tab (index 0) active");
    }

    /// Tests that handle_mouse routes clicks to the right pane's tab bar
    /// in a horizontal split layout.
    ///
    /// This is a regression test for the bug where clicks in non-top-left pane
    /// tab bars were not routed to handle_tab_bar_click.
    #[test]
    fn test_handle_mouse_routes_to_right_pane_tab_bar() {
        use crate::input::MouseEventKind;

        let mut state = create_horizontal_split_state();

        // Layout: 800x600 window, RAIL_WIDTH=56
        // Horizontal split at 0.5 ratio with bounds (56, 0, 744, 600):
        // - Left pane: x=56, y=0, width=372, height=600
        // - Right pane: x=428, y=0, width=372, height=600
        //
        // Right pane's tab bar is at y ∈ [0, 32) in screen space
        // NSView coords: y = view_height - screen_y - height
        // For y=16 in screen space (middle of tab bar): nsview_y = 600 - 16 = 584

        // Click on the second tab in the RIGHT pane via handle_mouse
        let click_x = 560.0; // Well inside second tab in right pane
        let nsview_y = 584.0; // 600 - 16 (middle of right pane's tab bar in NSView coords)

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: focus should have switched to right pane
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(ws.active_pane_id, 2, "Focus should have switched to right pane");

        // Verify: right pane should now have tab index 1 active
        let right_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(right_pane.active_tab, 1, "Right pane should have second tab (index 1) active");

        // Verify: left pane should still have tab index 0 active
        let left_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(left_pane.active_tab, 0, "Left pane should still have first tab (index 0) active");
    }

    /// Tests that handle_mouse still routes top-left pane clicks correctly.
    ///
    /// This is a regression test to ensure the fix for non-top-left panes
    /// doesn't break single-pane or top-left pane behavior.
    #[test]
    fn test_handle_mouse_routes_to_top_left_pane_tab_bar() {
        use crate::input::MouseEventKind;

        let mut state = create_vertical_split_state();

        // Layout: 800x600 window
        // Vertical split at 0.5 ratio:
        // - Top pane: y=0, height=300 (tab bar at y ∈ [0, 32))
        //
        // For y=16 in screen space: nsview_y = 600 - 16 = 584

        // Click on the second tab in the TOP pane via handle_mouse
        let click_x = 200.0; // Well inside second tab
        let nsview_y = 584.0; // 600 - 16 (middle of top pane's tab bar in NSView coords)

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: top pane should now have tab index 1 active
        let ws = state.editor.active_workspace().unwrap();
        let top_pane = ws.pane_root.get_pane(1).unwrap();
        assert_eq!(top_pane.active_tab, 1, "Top pane should have second tab (index 1) active");

        // Verify: bottom pane should still have tab index 0 active
        let bottom_pane = ws.pane_root.get_pane(2).unwrap();
        assert_eq!(bottom_pane.active_tab, 0, "Bottom pane should still have first tab (index 0) active");
    }

    /// Tests that handle_mouse routes clicks correctly in single-pane layouts.
    ///
    /// This is a regression test to ensure the multi-pane fix doesn't break
    /// single-pane behavior.
    #[test]
    fn test_handle_mouse_routes_to_single_pane_tab_bar() {
        use crate::input::MouseEventKind;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0);

        // Add a second tab to the default single pane
        let tab2 = crate::workspace::Tab::new_file(
            200,
            lite_edit_buffer::TextBuffer::from_str("second tab"),
            "second.rs".to_string(),
            None,
            test_font_metrics().line_height as f32,
        );

        if let Some(ws) = state.editor.active_workspace_mut() {
            if let Some(pane) = ws.active_pane_mut() {
                pane.add_tab(tab2);
                pane.switch_tab(0); // Start with first tab active
            }
        }

        // Click on the second tab via handle_mouse
        // Single pane tab bar is at y ∈ [0, 32) in screen space
        // For y=16: nsview_y = 600 - 16 = 584
        let click_x = 200.0; // Inside second tab
        let nsview_y = 584.0;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: second tab should be active
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.active_pane().unwrap();
        assert_eq!(pane.active_tab, 1, "Second tab should be active");
    }

    // =========================================================================
    // Cursor Positioning in Split Layouts (Chunk: docs/chunks/pane_cursor_click_offset)
    // =========================================================================
    //
    // These tests verify that mouse clicks in non-primary panes result in
    // correct cursor positioning. The bug was that clicks in the right pane
    // of a horizontal split would be offset to the right, and clicks in the
    // bottom pane of a vertical split would be offset downward.

    /// Tests that clicking in the right pane of a horizontal split positions
    /// the cursor correctly without rightward offset.
    ///
    /// The bug: When clicking at the origin of the right pane's content area,
    /// the cursor would incorrectly land at column 50 instead of column 0.
    #[test]
    fn test_cursor_click_right_pane_horizontal_split() {
        use crate::input::MouseEventKind;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = create_horizontal_split_state();

        // Layout: 800x600 window, RAIL_WIDTH=56, TAB_BAR_HEIGHT=32
        // Horizontal split at 0.5 ratio with bounds (56, 0, 744, 600):
        // - Left pane: x=56, y=0, width=372, height=600, content starts at y=32
        // - Right pane: x=428, y=0, width=372, height=600, content starts at y=32
        //
        // Click at the top-left of the right pane's content area
        // Screen coords: x=428 (right pane origin), y=32 (below tab bar)
        // NSView coords: y = view_height - screen_y = 600 - 32 = 568

        // First, switch focus to right pane
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.active_pane_id = 2;
        }

        // Click at the origin of the right pane's content
        let click_x = 428.0 + 8.0; // First character position (1 cell width in)
        let nsview_y = 600.0 - (TAB_BAR_HEIGHT as f64 + 8.0); // Slightly below tab bar

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: cursor should be near column 0-1, NOT offset by the right pane's x position
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.pane_root.get_pane(2).unwrap();
        if let Some(tab) = pane.active_tab() {
            if let Some(buffer) = tab.as_text_buffer() {
                let cursor = buffer.cursor_position();
                // With cell width 8.0, click at local_x=8 should give column ~1
                // The bug would place cursor at a much higher column (e.g., 46+)
                assert!(
                    cursor.col < 5,
                    "Cursor column should be near 0 (got {}), not offset by pane position",
                    cursor.col
                );
            }
        }
    }

    /// Tests that clicking in the bottom pane of a vertical split positions
    /// the cursor correctly without downward offset.
    ///
    /// The bug: When clicking at the origin of the bottom pane's content area,
    /// the cursor would incorrectly land at a line much lower than expected.
    #[test]
    fn test_cursor_click_bottom_pane_vertical_split() {
        use crate::input::MouseEventKind;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = create_vertical_split_state();

        // Layout: 800x600 window, RAIL_WIDTH=56, TAB_BAR_HEIGHT=32
        // Vertical split at 0.5 ratio with bounds (56, 0, 744, 600):
        // - Top pane: x=56, y=0, width=744, height=300, content starts at y=32
        // - Bottom pane: x=56, y=300, width=744, height=300, content starts at y=332
        //
        // Click at the top-left of the bottom pane's content area
        // Screen coords: x=64 (slightly into content), y=340 (first line of bottom pane content)
        // NSView coords: y = view_height - screen_y = 600 - 340 = 260

        // First, switch focus to bottom pane
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.active_pane_id = 2;
        }

        // Click at the origin of the bottom pane's content
        let click_x = 64.0; // Inside content area
        let screen_y = 300.0 + TAB_BAR_HEIGHT as f64 + 8.0; // First line of bottom pane
        let nsview_y = 600.0 - screen_y;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: cursor should be on line 0-1, NOT offset by the pane's y position
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.pane_root.get_pane(2).unwrap();
        if let Some(tab) = pane.active_tab() {
            if let Some(buffer) = tab.as_text_buffer() {
                let cursor = buffer.cursor_position();
                // Click at local_y=8 with line_height=16 should give line ~0
                // The bug would place cursor at a much higher line
                assert!(
                    cursor.line < 3,
                    "Cursor line should be near 0 (got {}), not offset by pane position",
                    cursor.line
                );
            }
        }
    }

    /// Tests that clicking in the top-left (primary) pane still works correctly.
    /// This is a regression test to ensure the fix doesn't break the working case.
    #[test]
    fn test_cursor_click_top_left_pane_no_regression() {
        use crate::input::MouseEventKind;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = create_horizontal_split_state();

        // Layout: 800x600 window, RAIL_WIDTH=56
        // - Left pane: x=56, y=0, width=372, height=600, content starts at y=32
        //
        // Click at the origin of the left pane's content
        let click_x = 64.0; // Slightly into content
        let screen_y = TAB_BAR_HEIGHT as f64 + 8.0;
        let nsview_y = 600.0 - screen_y;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: cursor should be near origin
        let ws = state.editor.active_workspace().unwrap();
        let pane = ws.active_pane().unwrap();
        if let Some(tab) = pane.active_tab() {
            if let Some(buffer) = tab.as_text_buffer() {
                let cursor = buffer.cursor_position();
                assert!(
                    cursor.col < 3,
                    "Left pane cursor column should be near 0 (got {})",
                    cursor.col
                );
                assert!(
                    cursor.line < 3,
                    "Left pane cursor line should be near 0 (got {})",
                    cursor.line
                );
            }
        }
    }

    /// Tests that clicking in the content area of a non-focused pane
    /// switches focus to that pane.
    #[test]
    fn test_click_switches_focus_to_right_pane() {
        use crate::input::MouseEventKind;
        use crate::tab_bar::TAB_BAR_HEIGHT;

        let mut state = create_horizontal_split_state();

        // Ensure focus is on left pane (pane 1)
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.active_pane_id = 1;
        }

        // Click in the right pane's content area
        // Right pane: x=428, content starts at y=32
        let click_x = 500.0;
        let screen_y = TAB_BAR_HEIGHT as f64 + 50.0;
        let nsview_y = 600.0 - screen_y;

        let click_event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (click_x, nsview_y),
            modifiers: crate::input::Modifiers::default(),
            click_count: 1,
        };

        state.handle_mouse(click_event);

        // Verify: focus should have switched to right pane
        let ws = state.editor.active_workspace().unwrap();
        assert_eq!(
            ws.active_pane_id, 2,
            "Focus should have switched to right pane (pane 2)"
        );
    }

    // =========================================================================
    // Terminal initial sizing in split pane tests
    // Chunk: docs/chunks/terminal_pane_initial_sizing - Terminal sizing in split panes
    // =========================================================================

    /// Tests that a terminal tab opened in a split pane receives correct initial dimensions
    /// matching the pane's actual dimensions, not the full window dimensions.
    #[test]
    fn test_terminal_initial_sizing_in_split_pane() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::left_rail::RAIL_WIDTH;
        use crate::pane_layout::Direction;

        let mut state = EditorState::empty(test_font_metrics());
        state.update_viewport_dimensions(800.0, 600.0 + TAB_BAR_HEIGHT);

        // Create a file tab and split horizontally
        state.new_tab();
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.move_active_tab(Direction::Right);
        }
        state.sync_pane_viewports();

        // Now create a terminal tab in the active pane (right half)
        state.new_terminal_tab();

        // Get terminal size
        let (term_cols, _term_rows) = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        // Calculate expected columns for the RIGHT pane (half of content area)
        let content_width = 800.0 - RAIL_WIDTH;
        let pane_width = content_width * 0.5; // Half due to horizontal split
        let expected_cols = (pane_width as f64 / test_font_metrics().advance_width).floor() as usize;

        // Terminal should be sized for the PANE, not the full window
        assert_eq!(term_cols, expected_cols,
            "Terminal should have {} columns for pane width {}, but has {}",
            expected_cols, pane_width, term_cols);
    }

    /// Tests that a terminal tab in a vertical split receives correct initial row count
    /// matching the pane's height, not the full window height.
    #[test]
    fn test_terminal_initial_sizing_in_vertical_split() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::pane_layout::Direction;

        let mut state = EditorState::empty(test_font_metrics());
        let view_height = 600.0 + TAB_BAR_HEIGHT;
        state.update_viewport_dimensions(800.0, view_height);

        // Create a file tab and split vertically
        state.new_tab();
        if let Some(ws) = state.editor.active_workspace_mut() {
            ws.move_active_tab(Direction::Down);
        }
        state.sync_pane_viewports();

        // Create a terminal tab in the active pane (bottom half)
        state.new_terminal_tab();

        // Get terminal size
        let (_term_cols, term_rows) = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        // Calculate expected rows for the BOTTOM pane (half of view_height, minus its tab bar)
        // In pane layout calculation, bounds are (0, 0, content_width, view_height)
        // So each pane in vertical split has height = view_height / 2
        let pane_height = view_height * 0.5; // Half due to vertical split
        let pane_content_height = pane_height - TAB_BAR_HEIGHT;
        let expected_rows = (pane_content_height as f64 / test_font_metrics().line_height).floor() as usize;

        // Terminal should be sized for the PANE, not the full window
        assert_eq!(term_rows, expected_rows,
            "Terminal should have {} rows for pane content height {}, but has {}",
            expected_rows, pane_content_height, term_rows);
    }

    /// Tests that a terminal tab in a single-pane layout still receives correct dimensions
    /// (regression test to ensure the fix doesn't break the common case).
    #[test]
    fn test_terminal_initial_sizing_in_single_pane() {
        use crate::tab_bar::TAB_BAR_HEIGHT;
        use crate::left_rail::RAIL_WIDTH;

        let mut state = EditorState::empty(test_font_metrics());
        let view_height = 600.0 + TAB_BAR_HEIGHT;
        state.update_viewport_dimensions(800.0, view_height);

        // Create a terminal tab in the default single pane
        state.new_terminal_tab();

        // Get terminal size
        let (term_cols, term_rows) = {
            let ws = state.editor.active_workspace().unwrap();
            let tab = ws.active_pane().unwrap().active_tab().unwrap();
            let term = tab.as_terminal_buffer().unwrap();
            term.size()
        };

        // Calculate expected dimensions for full content area
        // In single pane layout, pane height = view_height, content height = pane height - TAB_BAR_HEIGHT
        let content_width = 800.0 - RAIL_WIDTH;
        let content_height = view_height - TAB_BAR_HEIGHT; // 600.0

        let expected_cols = (content_width as f64 / test_font_metrics().advance_width).floor() as usize;
        let expected_rows = (content_height as f64 / test_font_metrics().line_height).floor() as usize;

        assert_eq!(term_cols, expected_cols, "Terminal columns mismatch in single pane");
        assert_eq!(term_rows, expected_rows, "Terminal rows mismatch in single pane");
    }

    // =========================================================================
    // Cursor clamping tests (Chunk: docs/chunks/base_snapshot_reload)
    // =========================================================================

    #[test]
    fn test_clamp_position_empty_buffer() {
        let buffer = TextBuffer::new();
        let pos = clamp_position_to_buffer(Position::new(5, 10), &buffer);
        assert_eq!(pos, Position::new(0, 0));
    }

    #[test]
    fn test_clamp_position_line_beyond_buffer() {
        let buffer = TextBuffer::from_str("line1\nline2");
        let pos = clamp_position_to_buffer(Position::new(10, 0), &buffer);
        assert_eq!(pos.line, 1); // clamped to last line
    }

    #[test]
    fn test_clamp_position_col_beyond_line() {
        let buffer = TextBuffer::from_str("abc");
        let pos = clamp_position_to_buffer(Position::new(0, 10), &buffer);
        assert_eq!(pos.col, 3); // clamped to end of line
    }

    #[test]
    fn test_clamp_position_valid() {
        let buffer = TextBuffer::from_str("hello\nworld");
        let pos = clamp_position_to_buffer(Position::new(1, 3), &buffer);
        assert_eq!(pos, Position::new(1, 3)); // unchanged
    }

    #[test]
    fn test_clamp_position_last_valid_position() {
        let buffer = TextBuffer::from_str("hello\nworld");
        // Last valid position is (1, 5) - line 1, column 5
        let pos = clamp_position_to_buffer(Position::new(1, 5), &buffer);
        assert_eq!(pos, Position::new(1, 5)); // unchanged
    }

    #[test]
    fn test_clamp_position_first_line() {
        let buffer = TextBuffer::from_str("hello\nworld");
        let pos = clamp_position_to_buffer(Position::new(0, 0), &buffer);
        assert_eq!(pos, Position::new(0, 0)); // unchanged
    }
}
