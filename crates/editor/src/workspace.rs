// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
// Chunk: docs/chunks/agent_lifecycle - Agent lifecycle tracking for Composer-like workflows
// Chunk: docs/chunks/workspace_dir_picker - Per-workspace FileIndex
//!
//! Workspace data model for the editor.
//!
//! This module implements the two-level tab hierarchy:
//! - `Editor` contains multiple `Workspace`s (top-level, shown in left rail)
//! - `Workspace` contains multiple `Tab`s (files, terminals, etc.)
//!
//! The workspace model enables Composer-like multi-agent workflows where each
//! workspace represents an independent working context.

use std::path::PathBuf;

use crate::event_channel::EventSender;
use crate::file_index::FileIndex;
use crate::pane_layout::{gen_pane_id, Pane, PaneId, PaneLayoutNode};
use crate::viewport::Viewport;
use lite_edit_buffer::{BufferView, TextBuffer};
use lite_edit_syntax::{LanguageRegistry, SyntaxHighlighter, SyntaxTheme};
// Chunk: docs/chunks/terminal_flood_starvation - PollResult for byte-budgeted polling
use lite_edit_terminal::{AgentConfig, AgentHandle, AgentState, PollResult, TerminalBuffer};

// =============================================================================
// ID Types
// =============================================================================

/// Unique identifier for a workspace.
pub type WorkspaceId = u64;

/// Unique identifier for a tab within a workspace.
pub type TabId = u64;

// =============================================================================
// WorkspaceStatus
// =============================================================================

/// Status of a workspace, primarily for agent-driven workflows.
///
/// The status drives the visual indicator in the left rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceStatus {
    /// No agent, just editing (grey indicator)
    #[default]
    Idle,
    /// Agent working autonomously (green indicator)
    Running,
    /// Agent waiting for user input (yellow indicator)
    NeedsInput,
    /// Waiting too long without response (orange indicator)
    Stale,
    /// Agent finished successfully (checkmark green indicator)
    Completed,
    /// Agent crashed or errored (red indicator)
    Errored,
}

// =============================================================================
// TabKind
// =============================================================================

/// The kind of content a tab holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabKind {
    /// A file being edited
    #[default]
    File,
    /// A terminal emulator
    Terminal,
    /// Agent output/conversation
    AgentOutput,
    /// A diff view
    Diff,
}

// =============================================================================
// TabBuffer
// =============================================================================

/// The buffer backing a tab.
///
/// This enum avoids trait object downcasting complexity by storing concrete
/// types. The `BufferView` trait is implemented to provide unified rendering.
pub enum TabBuffer {
    /// A file editing buffer
    File(TextBuffer),
    /// A standalone terminal (no agent)
    Terminal(TerminalBuffer),
    /// A placeholder for the agent terminal.
    ///
    /// The actual terminal buffer lives in `Workspace.agent`. When rendering
    /// a tab with this variant, access `workspace.agent.terminal()` instead.
    AgentTerminal,
}

impl std::fmt::Debug for TabBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TabBuffer::File(buf) => f.debug_tuple("File").field(buf).finish(),
            TabBuffer::Terminal(_) => f.debug_tuple("Terminal").field(&"<TerminalBuffer>").finish(),
            TabBuffer::AgentTerminal => write!(f, "AgentTerminal"),
        }
    }
}

impl TabBuffer {
    /// Returns a reference to the underlying `BufferView`.
    ///
    /// # Panics
    ///
    /// Panics for `AgentTerminal` variant, which is a placeholder.
    /// Use `Workspace::agent_terminal()` instead for agent tabs.
    pub fn as_buffer_view(&self) -> &dyn BufferView {
        match self {
            TabBuffer::File(buf) => buf,
            TabBuffer::Terminal(buf) => buf,
            TabBuffer::AgentTerminal => {
                panic!("AgentTerminal is a placeholder - use Workspace::agent_terminal()")
            }
        }
    }

    /// Returns a mutable reference to the underlying `BufferView`.
    ///
    /// # Panics
    ///
    /// Panics for `AgentTerminal` variant, which is a placeholder.
    /// Use `Workspace::agent_terminal_mut()` instead for agent tabs.
    pub fn as_buffer_view_mut(&mut self) -> &mut dyn BufferView {
        match self {
            TabBuffer::File(buf) => buf,
            TabBuffer::Terminal(buf) => buf,
            TabBuffer::AgentTerminal => {
                panic!("AgentTerminal is a placeholder - use Workspace::agent_terminal_mut()")
            }
        }
    }

    /// Attempts to get a reference to the underlying `TextBuffer`.
    ///
    /// Returns `Some` for file tabs, `None` for other tab types.
    pub fn as_text_buffer(&self) -> Option<&TextBuffer> {
        match self {
            TabBuffer::File(buf) => Some(buf),
            TabBuffer::Terminal(_) | TabBuffer::AgentTerminal => None,
        }
    }

    /// Attempts to get a mutable reference to the underlying `TextBuffer`.
    ///
    /// Returns `Some` for file tabs, `None` for other tab types.
    pub fn as_text_buffer_mut(&mut self) -> Option<&mut TextBuffer> {
        match self {
            TabBuffer::File(buf) => Some(buf),
            TabBuffer::Terminal(_) | TabBuffer::AgentTerminal => None,
        }
    }

    /// Attempts to get a reference to the underlying `TerminalBuffer`.
    ///
    /// Returns `Some` for terminal tabs, `None` for other tab types.
    /// For `AgentTerminal`, returns `None` - use `Workspace::agent_terminal()` instead.
    pub fn as_terminal_buffer(&self) -> Option<&TerminalBuffer> {
        match self {
            TabBuffer::Terminal(buf) => Some(buf),
            TabBuffer::File(_) | TabBuffer::AgentTerminal => None,
        }
    }

    /// Attempts to get a mutable reference to the underlying `TerminalBuffer`.
    ///
    /// Returns `Some` for terminal tabs, `None` for other tab types.
    /// For `AgentTerminal`, returns `None` - use `Workspace::agent_terminal_mut()` instead.
    pub fn as_terminal_buffer_mut(&mut self) -> Option<&mut TerminalBuffer> {
        match self {
            TabBuffer::Terminal(buf) => Some(buf),
            TabBuffer::File(_) | TabBuffer::AgentTerminal => None,
        }
    }

    /// Returns true if this is an agent terminal placeholder.
    pub fn is_agent_terminal(&self) -> bool {
        matches!(self, TabBuffer::AgentTerminal)
    }
}

// =============================================================================
// Tab
// =============================================================================

// Chunk: docs/chunks/content_tab_bar - Per-tab model: kind, buffer ref, dirty flag, unread badge
// Chunk: docs/chunks/syntax_highlighting - Added syntax highlighter field
// Chunk: docs/chunks/welcome_scroll - Welcome screen scroll offset field
/// A tab within a workspace.
///
/// Each tab owns its own buffer and viewport (for independent scroll positions).
pub struct Tab {
    /// Unique identifier for this tab
    pub id: TabId,
    /// Display label (filename, terminal title, etc.)
    pub label: String,
    /// The buffer backing this tab
    buffer: TabBuffer,
    /// The viewport (scroll state) for this tab
    pub viewport: Viewport,
    /// The kind of content this tab holds
    pub kind: TabKind,
    /// Whether the tab has unsaved changes
    pub dirty: bool,
    /// Whether the tab has unread content (for terminals)
    pub unread: bool,
    /// The file associated with this tab (for file tabs)
    pub associated_file: Option<PathBuf>,
    /// The syntax highlighter for file tabs (if language detected)
    highlighter: Option<SyntaxHighlighter>,
    /// Vertical scroll offset for the welcome screen, in pixels.
    ///
    /// Only meaningful when this is an empty File tab showing the welcome screen.
    /// Reset to 0 when a new blank tab becomes active. The lower bound (≥ 0) is
    /// enforced by `set_welcome_scroll_offset_px`; the upper bound is clamped at
    /// render time by `calculate_welcome_geometry`.
    welcome_scroll_offset_px: f32,
}

impl Tab {
    /// Creates a new file tab with the given buffer and optional file path.
    pub fn new_file(id: TabId, buffer: TextBuffer, label: String, path: Option<PathBuf>, line_height: f32) -> Self {
        Self {
            id,
            label,
            buffer: TabBuffer::File(buffer),
            viewport: Viewport::new(line_height),
            kind: TabKind::File,
            dirty: false,
            unread: false,
            associated_file: path,
            highlighter: None,
            welcome_scroll_offset_px: 0.0,
        }
    }

    /// Creates an empty file tab.
    pub fn empty_file(id: TabId, line_height: f32) -> Self {
        Self::new_file(id, TextBuffer::new(), "Untitled".to_string(), None, line_height)
    }

    /// Creates a new agent terminal tab.
    ///
    /// This is a placeholder tab - the actual terminal buffer lives in `Workspace.agent`.
    /// Use `Workspace::agent_terminal()` to access the terminal buffer.
    pub fn new_agent(id: TabId, label: String, line_height: f32) -> Self {
        Self {
            id,
            label,
            buffer: TabBuffer::AgentTerminal,
            viewport: Viewport::new(line_height),
            kind: TabKind::AgentOutput,
            dirty: false,
            unread: false,
            associated_file: None,
            highlighter: None,
            welcome_scroll_offset_px: 0.0,
        }
    }

    /// Creates a new standalone terminal tab.
    pub fn new_terminal(id: TabId, terminal: TerminalBuffer, label: String, line_height: f32) -> Self {
        Self {
            id,
            label,
            buffer: TabBuffer::Terminal(terminal),
            viewport: Viewport::new(line_height),
            kind: TabKind::Terminal,
            dirty: false,
            unread: false,
            associated_file: None,
            highlighter: None,
            welcome_scroll_offset_px: 0.0,
        }
    }

    /// Returns true if this is an agent terminal tab.
    pub fn is_agent_tab(&self) -> bool {
        self.buffer.is_agent_terminal()
    }

    /// Returns a reference to the buffer as a `BufferView`.
    pub fn buffer(&self) -> &dyn BufferView {
        self.buffer.as_buffer_view()
    }

    /// Returns a mutable reference to the buffer as a `BufferView`.
    pub fn buffer_mut(&mut self) -> &mut dyn BufferView {
        self.buffer.as_buffer_view_mut()
    }

    /// Returns a reference to the underlying `TextBuffer` if this is a file tab.
    pub fn as_text_buffer(&self) -> Option<&TextBuffer> {
        self.buffer.as_text_buffer()
    }

    /// Returns a mutable reference to the underlying `TextBuffer` if this is a file tab.
    pub fn as_text_buffer_mut(&mut self) -> Option<&mut TextBuffer> {
        self.buffer.as_text_buffer_mut()
    }

    // Chunk: docs/chunks/terminal_active_tab_safety - Terminal buffer access
    /// Returns a reference to the underlying `TerminalBuffer` if this is a terminal tab.
    pub fn as_terminal_buffer(&self) -> Option<&TerminalBuffer> {
        self.buffer.as_terminal_buffer()
    }

    /// Returns a mutable reference to the underlying `TerminalBuffer` if this is a terminal tab.
    pub fn as_terminal_buffer_mut(&mut self) -> Option<&mut TerminalBuffer> {
        self.buffer.as_terminal_buffer_mut()
    }

    /// Returns mutable references to both the text buffer and viewport.
    ///
    /// This method is needed to satisfy the borrow checker when both need
    /// to be passed to a function together. Returns `None` if this is not
    /// a file tab.
    pub fn buffer_and_viewport_mut(&mut self) -> Option<(&mut TextBuffer, &mut Viewport)> {
        match &mut self.buffer {
            TabBuffer::File(buf) => Some((buf, &mut self.viewport)),
            TabBuffer::Terminal(_) | TabBuffer::AgentTerminal => None,
        }
    }

    // Chunk: docs/chunks/terminal_scrollback_viewport - Terminal viewport access
    /// Returns mutable references to both the terminal buffer and viewport.
    ///
    /// This method is needed for terminal scrollback viewport support, where
    /// scroll events need access to both the terminal (for mode queries and
    /// line count) and the viewport (for scroll offset updates).
    ///
    /// Returns `None` if this is not a terminal tab.
    pub fn terminal_and_viewport_mut(&mut self) -> Option<(&mut TerminalBuffer, &mut Viewport)> {
        match &mut self.buffer {
            TabBuffer::Terminal(term) => Some((term, &mut self.viewport)),
            TabBuffer::File(_) | TabBuffer::AgentTerminal => None,
        }
    }

    // Chunk: docs/chunks/welcome_scroll - Welcome screen scroll offset accessors
    /// Returns the current vertical scroll offset for the welcome screen, in pixels.
    pub fn welcome_scroll_offset_px(&self) -> f32 {
        self.welcome_scroll_offset_px
    }

    /// Sets the vertical scroll offset for the welcome screen, in pixels.
    ///
    /// Enforces the lower bound (≥ 0). The upper bound is enforced at render time
    /// by `calculate_welcome_geometry`.
    pub fn set_welcome_scroll_offset_px(&mut self, offset: f32) {
        self.welcome_scroll_offset_px = offset.max(0.0);
    }

    // Chunk: docs/chunks/content_tab_bar - Unread badge support
    /// Marks the tab as having unread content.
    ///
    /// This is typically used for terminal tabs when output arrives while the
    /// tab is not active.
    pub fn mark_unread(&mut self) {
        self.unread = true;
    }

    /// Clears the unread state.
    ///
    /// Called when the tab becomes active to indicate the user has seen the content.
    pub fn clear_unread(&mut self) {
        self.unread = false;
    }

    // =========================================================================
    // Syntax Highlighting (Chunk: docs/chunks/syntax_highlighting)
    // =========================================================================

    /// Sets up syntax highlighting for this tab based on file extension.
    ///
    /// Call this after loading file content. If the extension is recognized,
    /// a highlighter is created with the full source for initial parsing.
    ///
    /// # Arguments
    ///
    /// * `registry` - The language registry for extension-to-language mapping
    /// * `theme` - The syntax theme for styling
    ///
    /// Returns `true` if a highlighter was successfully created.
    pub fn setup_highlighting(
        &mut self,
        registry: &LanguageRegistry,
        theme: SyntaxTheme,
    ) -> bool {
        // Only file tabs can have highlighting
        let (path, buffer) = match (&self.associated_file, &self.buffer) {
            (Some(p), TabBuffer::File(buf)) => (p, buf),
            _ => return false,
        };

        // Get extension from path
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => return false,
        };

        // Look up language config
        let config = match registry.config_for_extension(ext) {
            Some(c) => c,
            None => return false,
        };

        // Get current buffer content for initial parse
        let source = buffer.content();

        // Create highlighter
        match SyntaxHighlighter::new(config, &source, theme) {
            Some(hl) => {
                self.highlighter = Some(hl);
                true
            }
            None => false,
        }
    }

    /// Returns a reference to the syntax highlighter, if available.
    pub fn highlighter(&self) -> Option<&SyntaxHighlighter> {
        self.highlighter.as_ref()
    }

    /// Notifies the highlighter of a buffer edit for incremental parsing.
    ///
    /// Call this after any buffer mutation (insert, delete, etc.) to keep
    /// the syntax tree up to date.
    ///
    /// # Arguments
    ///
    /// * `event` - The edit event describing the change
    pub fn notify_edit(&mut self, event: lite_edit_syntax::EditEvent) {
        if let (Some(hl), TabBuffer::File(buffer)) = (&mut self.highlighter, &self.buffer) {
            let source = buffer.content();
            hl.edit(event, &source);
        }
    }

    /// Syncs the highlighter with the current buffer content.
    ///
    /// This is a simpler alternative to `notify_edit` that performs a full
    /// re-parse. Use this when you don't have precise edit information.
    pub fn sync_highlighter(&mut self) {
        if let (Some(hl), TabBuffer::File(buffer)) = (&mut self.highlighter, &mut self.buffer) {
            let source = buffer.content();
            hl.update_source(&source);
        }
    }
}

impl std::fmt::Debug for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tab")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("dirty", &self.dirty)
            .field("unread", &self.unread)
            .field("associated_file", &self.associated_file)
            .field("highlighter", &self.highlighter.as_ref().map(|_| "<SyntaxHighlighter>"))
            .finish()
    }
}

// =============================================================================
// Workspace
// =============================================================================

// Chunk: docs/chunks/content_tab_bar - Owns tab list and tab_bar_view_offset for horizontal scroll
// Chunk: docs/chunks/tiling_workspace_integration - Pane tree integration
// Chunk: docs/chunks/workspace_dir_picker - Per-workspace FileIndex
/// A workspace containing panes with tabs.
///
/// Each workspace represents an independent working context (e.g., a worktree,
/// an agent session, or a standalone editing environment).
///
/// The workspace uses a binary pane layout tree to organize tabs. With a single pane
/// (no splits), the editor behaves identically to a flat tab list. Splits create
/// additional panes for tiling window manager-style layouts.
pub struct Workspace {
    /// Unique identifier for this workspace
    pub id: WorkspaceId,
    /// Display label (branch name, project name, etc.)
    pub label: String,
    /// The root path for this workspace (typically the worktree root)
    pub root_path: PathBuf,
    /// The pane layout tree containing all panes and tabs.
    ///
    /// Initially a single `Leaf` node. Splits create `Split` nodes that
    /// divide space between two children.
    pub pane_root: PaneLayoutNode,
    /// The ID of the currently active pane.
    ///
    /// All tab operations delegate to this pane.
    pub active_pane_id: PaneId,
    /// Counter for generating unique pane IDs within this workspace.
    next_pane_id: u64,
    /// Status indicator for the left rail
    pub status: WorkspaceStatus,
    /// The agent running in this workspace (if any).
    ///
    /// When an agent is attached, its terminal is accessible via `agent_terminal()`.
    /// The first tab is typically an `AgentTerminal` placeholder that renders from here.
    pub agent: Option<AgentHandle>,
    // Chunk: docs/chunks/workspace_dir_picker - Per-workspace file index
    /// The file index for fuzzy file matching in this workspace.
    ///
    /// Each workspace has its own FileIndex rooted at its `root_path`, ensuring
    /// the file picker (Cmd+P) searches the correct directory for each workspace.
    pub file_index: FileIndex,
    /// The cache version at the last query (for streaming refresh during indexing).
    pub last_cache_version: u64,
}

impl Workspace {
    /// Creates a new workspace with no tabs.
    ///
    /// The workspace is initialized with a single empty pane (a `Leaf` node).
    // Chunk: docs/chunks/workspace_dir_picker - Initialize FileIndex for workspace
    // Chunk: docs/chunks/file_change_events - Optional EventSender for file change callbacks
    pub fn new(id: WorkspaceId, label: String, root_path: PathBuf) -> Self {
        Self::new_with_event_sender(id, label, root_path, None)
    }

    // Chunk: docs/chunks/file_change_events - File change event wiring
    /// Creates a new workspace with no tabs, with an optional EventSender for file change events.
    ///
    /// If an EventSender is provided, the FileIndex will forward file content changes
    /// to the event channel, enabling the editor to detect external file modifications.
    pub fn new_with_event_sender(
        id: WorkspaceId,
        label: String,
        root_path: PathBuf,
        event_sender: Option<EventSender>,
    ) -> Self {
        let mut next_pane_id = 0u64;
        let pane_id = gen_pane_id(&mut next_pane_id);
        let pane = Pane::new(pane_id, id);

        // Chunk: docs/chunks/deletion_rename_handling - Wire up all file event callbacks
        // Start FileIndex with or without file event callbacks
        let file_index = if let Some(sender) = event_sender {
            // Clone sender for each callback (EventSender is Arc-wrapped internally)
            let change_sender = sender.clone();
            let delete_sender = sender.clone();
            let rename_sender = sender;

            FileIndex::start_with_callbacks(
                root_path.clone(),
                // on_change: content modification (debounced)
                Some(move |path| {
                    // Ignore send errors (channel might be closed during shutdown)
                    let _ = change_sender.send_file_changed(path);
                }),
                // on_delete: file deletion (immediate)
                Some(move |path| {
                    let _ = delete_sender.send_file_deleted(path);
                }),
                // on_rename: file rename (immediate, from -> to)
                Some(move |from, to| {
                    let _ = rename_sender.send_file_renamed(from, to);
                }),
            )
        } else {
            FileIndex::start(root_path.clone())
        };

        Self {
            id,
            label,
            root_path,
            pane_root: PaneLayoutNode::single_pane(pane),
            active_pane_id: pane_id,
            next_pane_id,
            status: WorkspaceStatus::Idle,
            agent: None,
            file_index,
            last_cache_version: 0,
        }
    }

    /// Creates a new workspace with a single empty tab.
    ///
    /// The workspace is initialized with a single pane containing one empty file tab.
    // Chunk: docs/chunks/workspace_dir_picker - Initialize FileIndex for workspace
    pub fn with_empty_tab(id: WorkspaceId, tab_id: TabId, label: String, root_path: PathBuf, line_height: f32) -> Self {
        Self::with_empty_tab_and_event_sender(id, tab_id, label, root_path, line_height, None)
    }

    // Chunk: docs/chunks/file_change_events - File change event wiring
    /// Creates a new workspace with a single empty tab and an optional EventSender.
    ///
    /// If an EventSender is provided, the FileIndex will forward file content changes
    /// to the event channel, enabling the editor to detect external file modifications.
    pub fn with_empty_tab_and_event_sender(
        id: WorkspaceId,
        tab_id: TabId,
        label: String,
        root_path: PathBuf,
        line_height: f32,
        event_sender: Option<EventSender>,
    ) -> Self {
        let mut ws = Self::new_with_event_sender(id, label, root_path, event_sender);
        let tab = Tab::empty_file(tab_id, line_height);
        // Add to the active pane
        if let Some(pane) = ws.pane_root.get_pane_mut(ws.active_pane_id) {
            pane.add_tab(tab);
        }
        ws
    }

    /// Generates a new unique pane ID.
    pub fn gen_pane_id(&mut self) -> PaneId {
        gen_pane_id(&mut self.next_pane_id)
    }

    // =========================================================================
    // Pane accessors (Chunk: docs/chunks/tiling_workspace_integration)
    // =========================================================================

    /// Returns a reference to the active pane.
    pub fn active_pane(&self) -> Option<&Pane> {
        self.pane_root.get_pane(self.active_pane_id)
    }

    /// Returns a mutable reference to the active pane.
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.pane_root.get_pane_mut(self.active_pane_id)
    }

    /// Returns a flat list of all panes in this workspace.
    pub fn all_panes(&self) -> Vec<&Pane> {
        self.pane_root.all_panes()
    }

    /// Returns a flat list of mutable references to all panes in this workspace.
    pub fn all_panes_mut(&mut self) -> Vec<&mut Pane> {
        self.pane_root.all_panes_mut()
    }

    // =========================================================================
    // Pane focus and tab movement (Chunk: docs/chunks/tiling_focus_keybindings)
    // =========================================================================

    /// Switches focus to the pane in the given direction.
    ///
    /// Uses `find_target_in_direction` to determine which pane is visually
    /// adjacent in the given direction. If an existing pane is found, updates
    /// `active_pane_id` to that pane.
    ///
    /// # Returns
    ///
    /// `true` if focus was switched to a different pane, `false` if no pane
    /// exists in that direction (focus remains unchanged).
    pub fn switch_focus(&mut self, direction: crate::pane_layout::Direction) -> bool {
        use crate::pane_layout::MoveTarget;

        let target = self.pane_root.find_target_in_direction(self.active_pane_id, direction);

        match target {
            MoveTarget::ExistingPane(target_id) => {
                self.active_pane_id = target_id;
                true
            }
            MoveTarget::SplitPane(_, _) => {
                // No adjacent pane in that direction - focus stays put
                false
            }
        }
    }

    // Chunk: docs/chunks/pane_close_last_tab - Cleanup empty panes on last tab close
    /// Finds a pane to focus after the current active pane is removed.
    ///
    /// Searches for an adjacent pane in direction order: Right, Left, Down, Up.
    /// Returns the ID of the first existing pane found in any direction.
    ///
    /// # Returns
    ///
    /// `Some(PaneId)` if an adjacent pane exists, `None` if the active pane is
    /// the only pane in the tree.
    pub fn find_fallback_focus(&self) -> Option<PaneId> {
        use crate::pane_layout::{Direction, MoveTarget};

        // Search in direction order: Right, Left, Down, Up
        // This prefers horizontal neighbors first, then vertical
        for direction in [Direction::Right, Direction::Left, Direction::Down, Direction::Up] {
            let target = self.pane_root.find_target_in_direction(self.active_pane_id, direction);
            if let MoveTarget::ExistingPane(target_id) = target {
                return Some(target_id);
            }
        }

        // No adjacent pane found - the active pane is the only one
        None
    }

    /// Moves the active tab of the focused pane in the given direction.
    ///
    /// Uses `move_tab` from `pane_layout` to:
    /// - Move the tab to an existing pane in that direction, OR
    /// - Create a new pane via split if no existing target
    ///
    /// After a successful move, focus follows the moved tab to its new pane.
    ///
    /// # Returns
    ///
    /// The `MoveResult` indicating what happened.
    pub fn move_active_tab(&mut self, direction: crate::pane_layout::Direction) -> crate::pane_layout::MoveResult {
        use crate::pane_layout::{move_tab, MoveResult};

        let source_pane_id = self.active_pane_id;

        // Pre-generate pane ID to avoid borrow conflict
        // (We can't capture `self` in the closure while also borrowing `pane_root` mutably)
        let new_pane_id = self.gen_pane_id();

        let result = move_tab(
            &mut self.pane_root,
            source_pane_id,
            direction,
            || new_pane_id,
        );

        // Update focus to follow the moved tab
        match result {
            MoveResult::MovedToExisting { target_pane_id, .. } => {
                self.active_pane_id = target_pane_id;
            }
            MoveResult::MovedToNew { new_pane_id, .. } => {
                self.active_pane_id = new_pane_id;
            }
            MoveResult::Rejected | MoveResult::SourceNotFound => {
                // Focus unchanged
            }
        }

        result
    }

    // =========================================================================
    // Tab operations - delegate to active pane
    // =========================================================================

    /// Adds a tab to the active pane.
    pub fn add_tab(&mut self, tab: Tab) {
        if let Some(pane) = self.active_pane_mut() {
            pane.add_tab(tab);
        }
    }

    /// Closes a tab at the given index in the active pane, returning the removed tab.
    ///
    /// Returns `None` if the index is out of bounds.
    /// After closing, the active tab is adjusted to remain valid.
    pub fn close_tab(&mut self, index: usize) -> Option<Tab> {
        self.active_pane_mut()?.close_tab(index)
    }

    /// Returns a reference to the active tab in the active pane, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_pane()?.active_tab()
    }

    /// Returns a mutable reference to the active tab in the active pane, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_pane_mut()?.active_tab_mut()
    }

    /// Switches to the tab at the given index in the active pane.
    ///
    /// Does nothing if the index is out of bounds. When switching to a new tab,
    /// clears its unread state.
    pub fn switch_tab(&mut self, index: usize) {
        if let Some(pane) = self.active_pane_mut() {
            pane.switch_tab(index);
        }
    }

    /// Returns the number of tabs in the active pane.
    ///
    /// For the total tab count across all panes, use `total_tab_count()`.
    pub fn tab_count(&self) -> usize {
        self.active_pane().map(|p| p.tab_count()).unwrap_or(0)
    }

    /// Returns the total number of tabs across all panes in this workspace.
    pub fn total_tab_count(&self) -> usize {
        self.pane_root.all_panes().iter().map(|p| p.tab_count()).sum()
    }

    /// Returns the tabs in the active pane.
    ///
    /// This provides compatibility with code that expects a flat tab list.
    /// For multi-pane access, use `active_pane()` or `all_panes()`.
    pub fn tabs(&self) -> &[Tab] {
        self.active_pane().map(|p| p.tabs.as_slice()).unwrap_or(&[])
    }

    /// Returns the active tab index in the active pane.
    ///
    /// This provides compatibility with code that expects a flat tab list.
    pub fn active_tab_index(&self) -> usize {
        self.active_pane().map(|p| p.active_tab).unwrap_or(0)
    }

    /// Returns the tab bar view offset for the active pane.
    ///
    /// This provides compatibility with code that uses workspace-level tab bar offset.
    pub fn tab_bar_view_offset(&self) -> f32 {
        self.active_pane().map(|p| p.tab_bar_view_offset).unwrap_or(0.0)
    }

    /// Sets the tab bar view offset for the active pane.
    pub fn set_tab_bar_view_offset(&mut self, offset: f32) {
        if let Some(pane) = self.active_pane_mut() {
            pane.tab_bar_view_offset = offset;
        }
    }

    // =========================================================================
    // Agent lifecycle methods (Chunk: docs/chunks/agent_lifecycle)
    // =========================================================================

    /// Computes the workspace status from the agent state.
    ///
    /// This derives `WorkspaceStatus` from `AgentState`:
    /// - No agent → Idle
    /// - Starting/Running → Running
    /// - NeedsInput → NeedsInput
    /// - Stale → Stale
    /// - Exited(0) → Completed
    /// - Exited(non-zero) → Errored
    pub fn compute_status(&self) -> WorkspaceStatus {
        match &self.agent {
            None => WorkspaceStatus::Idle,
            Some(agent) => match agent.state() {
                AgentState::Starting | AgentState::Running => WorkspaceStatus::Running,
                AgentState::NeedsInput { .. } => WorkspaceStatus::NeedsInput,
                AgentState::Stale { .. } => WorkspaceStatus::Stale,
                AgentState::Exited { code: 0 } => WorkspaceStatus::Completed,
                AgentState::Exited { .. } => WorkspaceStatus::Errored,
            },
        }
    }

    /// Returns a reference to the agent's terminal buffer, if an agent is attached.
    pub fn agent_terminal(&self) -> Option<&TerminalBuffer> {
        self.agent.as_ref().map(|a| a.terminal())
    }

    /// Returns a mutable reference to the agent's terminal buffer, if an agent is attached.
    pub fn agent_terminal_mut(&mut self) -> Option<&mut TerminalBuffer> {
        self.agent.as_mut().map(|a| a.terminal_mut())
    }

    /// Launches an agent in this workspace.
    ///
    /// Creates an `AgentHandle` and adds an `AgentTerminal` tab as the first tab
    /// in the active pane. The agent terminal is pinned (always at index 0).
    ///
    /// # Arguments
    ///
    /// * `config` - Agent configuration (command, args, timeouts)
    /// * `tab_id` - ID for the new agent tab
    /// * `cols` - Terminal width in columns
    /// * `rows` - Terminal height in rows
    /// * `line_height` - Line height for the viewport
    ///
    /// # Errors
    ///
    /// Returns an error if an agent is already attached or if spawning fails.
    pub fn launch_agent(
        &mut self,
        config: AgentConfig,
        tab_id: TabId,
        cols: usize,
        rows: usize,
        line_height: f32,
    ) -> std::io::Result<()> {
        if self.agent.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Workspace already has an agent attached",
            ));
        }

        // Spawn the agent
        let agent = AgentHandle::spawn(config, cols, rows)?;

        // Create the agent tab
        let agent_tab = Tab::new_agent(tab_id, "Agent".to_string(), line_height);

        // Insert at the beginning (pinned position) in the active pane
        if let Some(pane) = self.active_pane_mut() {
            // Adjust active_tab index if needed
            if !pane.tabs.is_empty() && pane.active_tab > 0 {
                pane.active_tab += 1;
            }
            pane.tabs.insert(0, agent_tab);
            // Switch to the agent tab
            pane.active_tab = 0;
        }

        // Store the agent handle
        self.agent = Some(agent);

        // Update status
        self.status = self.compute_status();

        Ok(())
    }

    /// Restarts the agent if it's in Exited state.
    ///
    /// # Errors
    ///
    /// Returns an error if no agent is attached or if the agent is not in Exited state.
    pub fn restart_agent(&mut self) -> std::io::Result<()> {
        let agent = self.agent.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "No agent attached")
        })?;

        agent.restart()?;
        self.status = self.compute_status();
        Ok(())
    }

    /// Stops the running agent.
    ///
    /// # Errors
    ///
    /// Returns an error if no agent is attached or if the agent is already exited.
    pub fn stop_agent(&mut self) -> std::io::Result<()> {
        let agent = self.agent.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "No agent attached")
        })?;

        agent.stop()?;
        self.status = self.compute_status();
        Ok(())
    }

    /// Polls the agent and updates workspace status.
    ///
    /// Call this each frame to process PTY events and update state.
    /// Returns true if any PTY output was processed.
    pub fn poll_agent(&mut self) -> bool {
        let had_events = if let Some(ref mut agent) = self.agent {
            agent.poll()
        } else {
            false
        };

        // Always update status after polling
        self.status = self.compute_status();

        had_events
    }

    // Chunk: docs/chunks/terminal_tab_spawn - Poll standalone terminals
    // Chunk: docs/chunks/terminal_scrollback_viewport - Auto-follow on new output
    // Chunk: docs/chunks/tiling_workspace_integration - Iterate all panes
    // Chunk: docs/chunks/terminal_tab_spawn - Polls PTY events for all standalone terminal tabs
    // Chunk: docs/chunks/terminal_flood_starvation - Needs rewakeup propagation
    /// Polls PTY events for all standalone terminal tabs across all panes.
    ///
    /// This method also handles auto-follow behavior: when the viewport is at
    /// the bottom before polling, new output will advance the viewport to keep
    /// showing the latest content.
    ///
    /// Returns `(had_events, needs_rewakeup)`:
    /// - `had_events`: true if any terminal had output
    /// - `needs_rewakeup`: true if any terminal hit its byte budget and has more
    ///   data pending (caller should schedule a follow-up wakeup)
    pub fn poll_standalone_terminals(&mut self) -> (bool, bool) {
        use lite_edit_buffer::BufferView;

        let mut had_events = false;
        let mut needs_rewakeup = false;

        for pane in self.pane_root.all_panes_mut() {
            for tab in &mut pane.tabs {
                if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
                    // Track if we're at bottom before polling (for auto-follow)
                    // Also track if we're in alt screen (no auto-follow in alt screen)
                    let was_at_bottom = viewport.is_at_bottom(terminal.line_count());
                    let was_alt_screen = terminal.is_alt_screen();

                    let result = terminal.poll_events();

                    match result {
                        PollResult::Processed | PollResult::MorePending => {
                            had_events = true;
                            if matches!(result, PollResult::MorePending) {
                                needs_rewakeup = true;
                            }

                            // Auto-follow behavior: if we were at bottom and in primary screen,
                            // advance the viewport to show new content
                            let now_alt_screen = terminal.is_alt_screen();

                            // Handle mode transition: alt -> primary means snap to bottom
                            if was_alt_screen && !now_alt_screen {
                                viewport.scroll_to_bottom(terminal.line_count());
                            } else if !now_alt_screen && was_at_bottom {
                                // Primary screen auto-follow
                                viewport.scroll_to_bottom(terminal.line_count());
                            }
                        }
                        PollResult::Idle => {}
                    }
                }
            }
        }
        (had_events, needs_rewakeup)
    }
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("root_path", &self.root_path)
            .field("pane_count", &self.pane_root.pane_count())
            .field("active_pane_id", &self.active_pane_id)
            .field("status", &self.status)
            .field("agent", &self.agent.as_ref().map(|a| a.state()))
            .finish()
    }
}

// =============================================================================
// Editor
// =============================================================================

/// The top-level editor state containing all workspaces.
///
/// This struct manages the workspace collection and provides methods for
/// workspace creation, switching, and closing.
// Chunk: docs/chunks/file_change_events - EventSender for file change callbacks
pub struct Editor {
    /// The workspaces in the editor
    pub workspaces: Vec<Workspace>,
    /// Index of the currently active workspace
    pub active_workspace: usize,
    /// Counter for generating unique workspace IDs
    next_workspace_id: u64,
    /// Counter for generating unique tab IDs
    next_tab_id: u64,
    /// Line height for creating new tabs (cached from font metrics)
    line_height: f32,
    /// Event sender for file change callbacks (cloned to each workspace's FileIndex)
    event_sender: Option<EventSender>,
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor")
            .field("workspaces", &self.workspaces)
            .field("active_workspace", &self.active_workspace)
            .field("next_workspace_id", &self.next_workspace_id)
            .field("next_tab_id", &self.next_tab_id)
            .field("line_height", &self.line_height)
            .field("event_sender", &self.event_sender.as_ref().map(|_| "<EventSender>"))
            .finish()
    }
}

impl Editor {
    /// Creates a new editor with one empty workspace.
    pub fn new(line_height: f32) -> Self {
        let mut editor = Self {
            workspaces: Vec::new(),
            active_workspace: 0,
            next_workspace_id: 0,
            next_tab_id: 0,
            line_height,
            event_sender: None,
        };

        // Create an initial empty workspace
        let root_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        editor.new_workspace_internal("untitled".to_string(), root_path, true);

        editor
    }

    // Chunk: docs/chunks/startup_workspace_dialog - Deferred initialization for startup dialog
    /// Creates a new editor with no workspaces.
    ///
    /// This constructor is used during application startup when the workspace
    /// directory needs to be selected via a dialog before creating any workspaces.
    /// After calling this, use `new_workspace()` to add a workspace once the
    /// directory is known.
    ///
    /// # Arguments
    ///
    /// * `line_height` - The line height for creating new tabs (from font metrics)
    pub fn new_deferred(line_height: f32) -> Self {
        Self {
            workspaces: Vec::new(),
            active_workspace: 0,
            next_workspace_id: 0,
            next_tab_id: 0,
            line_height,
            event_sender: None,
        }
    }

    // Chunk: docs/chunks/file_change_events - Set event sender for file change callbacks
    /// Sets the event sender for file change callbacks.
    ///
    /// When set, new workspaces will receive a clone of this sender, enabling
    /// their FileIndex to forward file content changes to the event channel.
    /// Existing workspaces are not affected (they were created before the
    /// sender was available).
    ///
    /// This should be called early in application startup, before creating
    /// the workspaces that need file change events.
    pub fn set_event_sender(&mut self, sender: EventSender) {
        self.event_sender = Some(sender);
    }

    /// Generates a new unique workspace ID.
    fn gen_workspace_id(&mut self) -> WorkspaceId {
        let id = self.next_workspace_id;
        self.next_workspace_id += 1;
        id
    }

    /// Generates a new unique tab ID.
    pub fn gen_tab_id(&mut self) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        id
    }

    // Chunk: docs/chunks/file_change_events - Pass EventSender to workspaces
    /// Internal method to create a workspace.
    fn new_workspace_internal(&mut self, label: String, root_path: PathBuf, with_tab: bool) -> WorkspaceId {
        let ws_id = self.gen_workspace_id();
        let event_sender = self.event_sender.clone();
        let workspace = if with_tab {
            let tab_id = self.gen_tab_id();
            Workspace::with_empty_tab_and_event_sender(
                ws_id,
                tab_id,
                label,
                root_path,
                self.line_height,
                event_sender,
            )
        } else {
            Workspace::new_with_event_sender(ws_id, label, root_path, event_sender)
        };
        self.workspaces.push(workspace);
        ws_id
    }

    /// Creates a new workspace with an empty tab and switches to it.
    ///
    /// Returns the ID of the new workspace.
    pub fn new_workspace(&mut self, label: String, root_path: PathBuf) -> WorkspaceId {
        let ws_id = self.new_workspace_internal(label, root_path, true);
        // Switch to the new workspace
        self.active_workspace = self.workspaces.len() - 1;
        ws_id
    }

    // Chunk: docs/chunks/workspace_initial_terminal - Terminal tab for subsequent workspaces
    /// Creates a new workspace without any initial tabs and switches to it.
    ///
    /// Returns the ID of the new workspace.
    pub fn new_workspace_without_tab(&mut self, label: String, root_path: PathBuf) -> WorkspaceId {
        let ws_id = self.new_workspace_internal(label, root_path, false);
        self.active_workspace = self.workspaces.len() - 1;
        ws_id
    }

    /// Closes the workspace at the given index.
    ///
    /// Returns the removed workspace, or `None` if the index is invalid or
    /// this is the last workspace (cannot close the last workspace).
    pub fn close_workspace(&mut self, index: usize) -> Option<Workspace> {
        // Cannot close the last workspace
        if self.workspaces.len() <= 1 {
            return None;
        }

        if index >= self.workspaces.len() {
            return None;
        }

        let removed = self.workspaces.remove(index);

        // Adjust active_workspace to remain valid
        if self.active_workspace >= self.workspaces.len() {
            self.active_workspace = self.workspaces.len() - 1;
        } else if self.active_workspace > index {
            self.active_workspace = self.active_workspace.saturating_sub(1);
        }

        Some(removed)
    }

    /// Returns a reference to the active workspace.
    pub fn active_workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active_workspace)
    }

    /// Returns a mutable reference to the active workspace.
    pub fn active_workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active_workspace)
    }

    /// Switches to the workspace at the given index.
    ///
    /// Does nothing if the index is out of bounds.
    pub fn switch_workspace(&mut self, index: usize) {
        if index < self.workspaces.len() {
            self.active_workspace = index;
        }
    }

    /// Returns the number of workspaces.
    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    /// Returns the line height used for creating new tabs.
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    // Chunk: docs/chunks/renderer_polymorphic_buffer - Polymorphic buffer access
    /// Returns a reference to the active tab's BufferView.
    ///
    /// Returns `None` if there is no active workspace or tab.
    /// Handles AgentTerminal placeholder by delegating to workspace agent.
    pub fn active_buffer_view(&self) -> Option<&dyn BufferView> {
        let workspace = self.active_workspace()?;
        let tab = workspace.active_tab()?;

        if tab.buffer.is_agent_terminal() {
            // AgentTerminal is a placeholder - get the actual buffer from workspace
            workspace.agent_terminal().map(|t| t as &dyn BufferView)
        } else {
            Some(tab.buffer())
        }
    }

    // Chunk: docs/chunks/welcome_scroll - Welcome screen scroll offset for the active tab
    /// Returns the welcome screen vertical scroll offset for the active tab, in pixels.
    ///
    /// Returns 0.0 if there is no active workspace or tab.
    pub fn welcome_scroll_offset_px(&self) -> f32 {
        self.active_workspace()
            .and_then(|ws| ws.active_tab())
            .map(|t| t.welcome_scroll_offset_px())
            .unwrap_or(0.0)
    }

    // Chunk: docs/chunks/welcome_screen - Welcome screen visibility check
    // Chunk: docs/chunks/welcome_file_backed - Exclude file-backed tabs from welcome screen
    /// Returns true if the welcome screen should be shown for the active tab.
    ///
    /// The welcome screen is displayed when:
    /// - There is an active workspace with an active tab
    /// - The active tab is a File tab (not Terminal or AgentOutput)
    /// - The tab is NOT backed by a file on disk (associated_file is None)
    /// - The tab's TextBuffer is empty
    ///
    /// This provides a Vim-style welcome/intro screen on initial launch and
    /// when creating new empty tabs. File-backed tabs (even if empty) show
    /// their actual (empty) contents instead.
    pub fn should_show_welcome_screen(&self) -> bool {
        let workspace = match self.active_workspace() {
            Some(ws) => ws,
            None => return false,
        };

        let tab = match workspace.active_tab() {
            Some(t) => t,
            None => return false,
        };

        // Only show welcome screen for File tabs
        if tab.kind != TabKind::File {
            return false;
        }

        // Don't show welcome screen for file-backed tabs (even if empty)
        // The welcome screen is for fresh, unassociated scratch buffers only
        if tab.associated_file.is_some() {
            return false;
        }

        // Check if the buffer is empty
        match tab.as_text_buffer() {
            Some(buffer) => buffer.is_empty(),
            None => false,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_LINE_HEIGHT: f32 = 16.0;

    // =========================================================================
    // TabBuffer Tests
    // =========================================================================

    #[test]
    fn test_tab_buffer_as_text_buffer() {
        let buffer = TextBuffer::from_str("hello");
        let tab_buffer = TabBuffer::File(buffer);

        let text_buf = tab_buffer.as_text_buffer().unwrap();
        assert_eq!(text_buf.content(), "hello");
    }

    #[test]
    fn test_tab_buffer_as_text_buffer_mut() {
        let buffer = TextBuffer::from_str("hello");
        let mut tab_buffer = TabBuffer::File(buffer);

        let text_buf = tab_buffer.as_text_buffer_mut().unwrap();
        // Cursor starts at (0, 0), so inserting prepends
        text_buf.insert_str("pre ");
        assert_eq!(text_buf.content(), "pre hello");
    }

    // =========================================================================
    // Tab Tests
    // =========================================================================

    #[test]
    fn test_tab_new_file() {
        let buffer = TextBuffer::from_str("content");
        let tab = Tab::new_file(1, buffer, "test.rs".to_string(), Some(PathBuf::from("/test.rs")), TEST_LINE_HEIGHT);

        assert_eq!(tab.id, 1);
        assert_eq!(tab.label, "test.rs");
        assert_eq!(tab.kind, TabKind::File);
        assert!(!tab.dirty);
        assert!(!tab.unread);
        assert_eq!(tab.associated_file, Some(PathBuf::from("/test.rs")));
    }

    #[test]
    fn test_tab_empty_file() {
        let tab = Tab::empty_file(1, TEST_LINE_HEIGHT);

        assert_eq!(tab.id, 1);
        assert_eq!(tab.label, "Untitled");
        assert_eq!(tab.kind, TabKind::File);
        assert!(tab.associated_file.is_none());
    }

    #[test]
    fn test_tab_as_text_buffer() {
        let buffer = TextBuffer::from_str("hello");
        let tab = Tab::new_file(1, buffer, "test.rs".to_string(), None, TEST_LINE_HEIGHT);

        let text_buf = tab.as_text_buffer().unwrap();
        assert_eq!(text_buf.content(), "hello");
    }

    #[test]
    fn test_tab_as_text_buffer_mut() {
        let buffer = TextBuffer::from_str("hello");
        let mut tab = Tab::new_file(1, buffer, "test.rs".to_string(), None, TEST_LINE_HEIGHT);

        let text_buf = tab.as_text_buffer_mut().unwrap();
        // Cursor starts at (0, 0), so inserting prepends
        text_buf.insert_str("pre ");
        assert_eq!(text_buf.content(), "pre hello");
    }

    // =========================================================================
    // Workspace Tests (Chunk: docs/chunks/tiling_workspace_integration)
    // =========================================================================

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        assert_eq!(ws.id, 1);
        assert_eq!(ws.label, "test");
        assert_eq!(ws.root_path, PathBuf::from("/test"));
        // Empty pane has no tabs, but pane exists
        assert_eq!(ws.pane_root.pane_count(), 1);
        assert_eq!(ws.tab_count(), 0);
        assert_eq!(ws.status, WorkspaceStatus::Idle);
    }

    #[test]
    fn test_workspace_with_empty_tab() {
        let ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);

        assert_eq!(ws.tab_count(), 1);
        assert_eq!(ws.active_tab_index(), 0);
        assert!(ws.active_tab().is_some());
    }

    #[test]
    fn test_workspace_add_tab() {
        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));
        let tab = Tab::empty_file(1, TEST_LINE_HEIGHT);

        ws.add_tab(tab);

        assert_eq!(ws.tab_count(), 1);
        assert_eq!(ws.active_tab_index(), 0);
    }

    #[test]
    fn test_workspace_close_tab() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        let tab2 = Tab::empty_file(2, TEST_LINE_HEIGHT);
        ws.add_tab(tab2);

        assert_eq!(ws.tab_count(), 2);
        assert_eq!(ws.active_tab_index(), 1); // add_tab switches to new tab

        let removed = ws.close_tab(1);
        assert!(removed.is_some());
        assert_eq!(ws.tab_count(), 1);
        assert_eq!(ws.active_tab_index(), 0);
    }

    #[test]
    fn test_workspace_close_tab_adjusts_active() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        ws.add_tab(Tab::empty_file(2, TEST_LINE_HEIGHT));
        ws.add_tab(Tab::empty_file(3, TEST_LINE_HEIGHT));
        // Now active_tab is 2 (third tab)

        // Close the second tab (index 1)
        ws.close_tab(1);
        // Active should still point to the last tab (now index 1)
        assert_eq!(ws.active_tab_index(), 1);
    }

    #[test]
    fn test_workspace_switch_tab() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        ws.add_tab(Tab::empty_file(2, TEST_LINE_HEIGHT));
        ws.switch_tab(0);

        assert_eq!(ws.active_tab_index(), 0);
    }

    #[test]
    fn test_workspace_switch_tab_invalid() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        ws.switch_tab(10); // Out of bounds

        assert_eq!(ws.active_tab_index(), 0); // Unchanged
    }

    // =========================================================================
    // Editor Tests
    // =========================================================================

    #[test]
    fn test_editor_new() {
        let editor = Editor::new(TEST_LINE_HEIGHT);

        assert_eq!(editor.workspaces.len(), 1);
        assert_eq!(editor.active_workspace, 0);
        assert!(editor.active_workspace().is_some());
    }

    #[test]
    fn test_editor_active_workspace() {
        let editor = Editor::new(TEST_LINE_HEIGHT);
        let ws = editor.active_workspace().unwrap();

        assert_eq!(ws.label, "untitled");
        assert_eq!(ws.tab_count(), 1);
    }

    #[test]
    fn test_editor_new_workspace() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);

        let ws_id = editor.new_workspace("test".to_string(), PathBuf::from("/test"));

        assert_eq!(editor.workspaces.len(), 2);
        assert_eq!(editor.active_workspace, 1); // Switched to new workspace
        assert_eq!(editor.active_workspace().unwrap().id, ws_id);
    }

    #[test]
    fn test_editor_switch_workspace() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);
        editor.new_workspace("test".to_string(), PathBuf::from("/test"));

        editor.switch_workspace(0);
        assert_eq!(editor.active_workspace, 0);

        editor.switch_workspace(1);
        assert_eq!(editor.active_workspace, 1);
    }

    #[test]
    fn test_editor_switch_workspace_invalid() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);
        editor.switch_workspace(10); // Out of bounds

        assert_eq!(editor.active_workspace, 0); // Unchanged
    }

    #[test]
    fn test_editor_close_workspace() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);
        editor.new_workspace("test".to_string(), PathBuf::from("/test"));

        assert_eq!(editor.workspaces.len(), 2);

        let removed = editor.close_workspace(1);
        assert!(removed.is_some());
        assert_eq!(editor.workspaces.len(), 1);
    }

    #[test]
    fn test_editor_cannot_close_last_workspace() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);

        assert_eq!(editor.workspaces.len(), 1);

        let removed = editor.close_workspace(0);
        assert!(removed.is_none()); // Cannot close last workspace
        assert_eq!(editor.workspaces.len(), 1);
    }

    #[test]
    fn test_editor_close_workspace_adjusts_active() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);
        editor.new_workspace("test1".to_string(), PathBuf::from("/test1"));
        editor.new_workspace("test2".to_string(), PathBuf::from("/test2"));
        // Now active_workspace is 2 (third workspace)

        editor.close_workspace(1);
        // Active should now be 1 (was 2, but we removed index 1, so it shifts)
        assert_eq!(editor.active_workspace, 1);
    }

    #[test]
    fn test_editor_workspace_count() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);
        assert_eq!(editor.workspace_count(), 1);

        editor.new_workspace("test".to_string(), PathBuf::from("/test"));
        assert_eq!(editor.workspace_count(), 2);
    }

    #[test]
    fn test_editor_gen_tab_id_is_unique() {
        let mut editor = Editor::new(TEST_LINE_HEIGHT);

        let id1 = editor.gen_tab_id();
        let id2 = editor.gen_tab_id();
        let id3 = editor.gen_tab_id();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    // =========================================================================
    // Unread Badge Tests (Chunk: docs/chunks/content_tab_bar)
    // =========================================================================

    #[test]
    fn test_tab_mark_unread() {
        let mut tab = Tab::empty_file(1, TEST_LINE_HEIGHT);
        assert!(!tab.unread);

        tab.mark_unread();
        assert!(tab.unread);
    }

    #[test]
    fn test_tab_clear_unread() {
        let mut tab = Tab::empty_file(1, TEST_LINE_HEIGHT);
        tab.mark_unread();
        assert!(tab.unread);

        tab.clear_unread();
        assert!(!tab.unread);
    }

    #[test]
    fn test_switch_tab_clears_unread() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);

        // Add a second tab and mark it unread
        let mut tab2 = Tab::empty_file(2, TEST_LINE_HEIGHT);
        tab2.mark_unread();
        ws.add_tab(tab2);

        // Switch back to first tab
        ws.switch_tab(0);

        // Now switch to the second tab - its unread state should clear
        assert!(ws.tabs()[1].unread); // Still unread before switch
        ws.switch_tab(1);
        assert!(!ws.tabs()[1].unread); // Cleared after switch
    }

    // =========================================================================
    // Workspace FileIndex Tests (Chunk: docs/chunks/workspace_dir_picker)
    // =========================================================================

    #[test]
    fn test_workspace_has_file_index() {
        let ws = Workspace::new(1, "test".to_string(), PathBuf::from("/nonexistent/test"));
        // FileIndex should be initialized (even for non-existent paths)
        // The is_indexing flag will be false once the walker completes
        // (immediately for non-existent paths)
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!ws.file_index.is_indexing());
    }

    #[test]
    fn test_workspace_file_index_uses_root_path() {
        use tempfile::TempDir;
        use std::fs::File;

        // Create a temp directory with a test file
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        File::create(root.join("test_file.rs")).unwrap();

        let ws = Workspace::new(1, "test".to_string(), root.to_path_buf());

        // Wait for indexing to complete
        while ws.file_index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Query should find the test file
        let results = ws.file_index.query("");
        assert!(results.iter().any(|r| r.path == PathBuf::from("test_file.rs")));
    }

    #[test]
    fn test_multiple_workspaces_have_independent_file_indexes() {
        use tempfile::TempDir;
        use std::fs::File;

        // Create two temp directories with different files
        let temp1 = TempDir::new().unwrap();
        let temp2 = TempDir::new().unwrap();
        File::create(temp1.path().join("file_in_ws1.rs")).unwrap();
        File::create(temp2.path().join("file_in_ws2.rs")).unwrap();

        let ws1 = Workspace::new(1, "ws1".to_string(), temp1.path().to_path_buf());
        let ws2 = Workspace::new(2, "ws2".to_string(), temp2.path().to_path_buf());

        // Wait for indexing to complete
        while ws1.file_index.is_indexing() || ws2.file_index.is_indexing() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Each workspace should only see its own files
        let results1 = ws1.file_index.query("");
        let results2 = ws2.file_index.query("");

        assert!(results1.iter().any(|r| r.path == PathBuf::from("file_in_ws1.rs")));
        assert!(!results1.iter().any(|r| r.path == PathBuf::from("file_in_ws2.rs")));

        assert!(results2.iter().any(|r| r.path == PathBuf::from("file_in_ws2.rs")));
        assert!(!results2.iter().any(|r| r.path == PathBuf::from("file_in_ws1.rs")));
    }

    #[test]
    fn test_workspace_with_empty_tab_has_file_index() {
        let ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/nonexistent"), TEST_LINE_HEIGHT);
        // FileIndex should be initialized
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!ws.file_index.is_indexing());
    }

    // =========================================================================
    // Deferred Initialization Tests (Chunk: docs/chunks/startup_workspace_dialog)
    // =========================================================================

    #[test]
    fn test_editor_new_deferred_has_no_workspaces() {
        let editor = Editor::new_deferred(TEST_LINE_HEIGHT);
        assert_eq!(editor.workspace_count(), 0);
        assert!(editor.active_workspace().is_none());
    }

    #[test]
    fn test_editor_new_deferred_can_add_workspace() {
        let mut editor = Editor::new_deferred(TEST_LINE_HEIGHT);
        editor.new_workspace("test".to_string(), PathBuf::from("/test"));
        assert_eq!(editor.workspace_count(), 1);
        assert!(editor.active_workspace().is_some());
    }

    #[test]
    fn test_editor_new_deferred_active_workspace_index_zero() {
        let editor = Editor::new_deferred(TEST_LINE_HEIGHT);
        // Even with no workspaces, active_workspace index defaults to 0
        assert_eq!(editor.active_workspace, 0);
    }

    #[test]
    fn test_editor_new_deferred_preserves_line_height() {
        let editor = Editor::new_deferred(TEST_LINE_HEIGHT);
        assert_eq!(editor.line_height(), TEST_LINE_HEIGHT);
    }

    #[test]
    fn test_editor_new_deferred_first_workspace_gets_tab() {
        let mut editor = Editor::new_deferred(TEST_LINE_HEIGHT);
        editor.new_workspace("test".to_string(), PathBuf::from("/test"));

        // new_workspace creates a workspace with one empty tab
        let ws = editor.active_workspace().unwrap();
        assert_eq!(ws.tab_count(), 1);
        assert!(ws.active_tab().is_some());
    }

    // =========================================================================
    // Pane Focus and Tab Movement Tests (Chunk: docs/chunks/tiling_focus_keybindings)
    // =========================================================================

    use crate::pane_layout::{Direction, Pane, PaneLayoutNode, SplitDirection};

    // Helper to create a workspace with a horizontal split (two panes side by side)
    fn create_hsplit_workspace() -> Workspace {
        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Create two panes with tabs
        let mut pane1 = Pane::new(1, 1);
        pane1.add_tab(Tab::empty_file(1, TEST_LINE_HEIGHT));
        pane1.add_tab(Tab::empty_file(2, TEST_LINE_HEIGHT));

        let mut pane2 = Pane::new(2, 1);
        pane2.add_tab(Tab::empty_file(3, TEST_LINE_HEIGHT));

        // Create horizontal split layout (pane1 left, pane2 right)
        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane1)),
            second: Box::new(PaneLayoutNode::Leaf(pane2)),
        };
        ws.active_pane_id = 1; // Start focused on left pane
        ws.next_pane_id = 3;   // Next ID after pane1 and pane2

        ws
    }

    #[test]
    fn test_switch_focus_right() {
        let mut ws = create_hsplit_workspace();
        assert_eq!(ws.active_pane_id, 1); // Start on left pane

        let switched = ws.switch_focus(Direction::Right);

        assert!(switched);
        assert_eq!(ws.active_pane_id, 2); // Now on right pane
    }

    #[test]
    fn test_switch_focus_left() {
        let mut ws = create_hsplit_workspace();
        ws.active_pane_id = 2; // Start on right pane

        let switched = ws.switch_focus(Direction::Left);

        assert!(switched);
        assert_eq!(ws.active_pane_id, 1); // Now on left pane
    }

    #[test]
    fn test_switch_focus_no_pane_in_direction() {
        let mut ws = create_hsplit_workspace();
        assert_eq!(ws.active_pane_id, 1); // Start on left pane

        // Try to switch left (no pane there)
        let switched = ws.switch_focus(Direction::Left);

        assert!(!switched); // No switch happened
        assert_eq!(ws.active_pane_id, 1); // Still on left pane
    }

    #[test]
    fn test_switch_focus_single_pane() {
        let ws_id = 1;
        let mut ws = Workspace::with_empty_tab(ws_id, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);

        // Get the initial pane ID (which is 0 from gen_pane_id)
        let initial_pane_id = ws.active_pane_id;

        // Single pane - no direction has a target
        let switched = ws.switch_focus(Direction::Right);

        assert!(!switched);
        assert_eq!(ws.active_pane_id, initial_pane_id);
    }

    #[test]
    fn test_move_active_tab_creates_split() {
        // Create workspace with single pane, two tabs
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        ws.add_tab(Tab::empty_file(2, TEST_LINE_HEIGHT));
        // Now pane has 2 tabs, active_tab = 1 (second tab)

        let result = ws.move_active_tab(Direction::Right);

        // Should create a new pane via split
        use crate::pane_layout::MoveResult;
        assert!(matches!(result, MoveResult::MovedToNew { .. }));

        // Layout should now have 2 panes
        assert_eq!(ws.pane_root.pane_count(), 2);

        // Focus should follow the moved tab
        let new_pane_id = match result {
            MoveResult::MovedToNew { new_pane_id, .. } => new_pane_id,
            _ => panic!("Expected MovedToNew"),
        };
        assert_eq!(ws.active_pane_id, new_pane_id);
    }

    #[test]
    fn test_move_active_tab_to_existing() {
        let mut ws = create_hsplit_workspace();
        // Left pane (1) has 2 tabs, right pane (2) has 1 tab
        // Focus on left pane, active tab is second tab

        let result = ws.move_active_tab(Direction::Right);

        use crate::pane_layout::MoveResult;
        assert!(matches!(result, MoveResult::MovedToExisting { target_pane_id: 2, .. }));

        // Focus should follow to right pane
        assert_eq!(ws.active_pane_id, 2);

        // Left pane should now have 1 tab
        assert_eq!(ws.pane_root.get_pane(1).unwrap().tab_count(), 1);

        // Right pane should now have 2 tabs
        assert_eq!(ws.pane_root.get_pane(2).unwrap().tab_count(), 2);
    }

    #[test]
    fn test_move_active_tab_single_tab_rejected() {
        // Create workspace with single pane, single tab
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);

        let result = ws.move_active_tab(Direction::Right);

        // Should be rejected (can't split single-tab pane with no existing target)
        use crate::pane_layout::MoveResult;
        assert_eq!(result, MoveResult::Rejected);

        // Layout unchanged
        assert_eq!(ws.pane_root.pane_count(), 1);
    }

    #[test]
    fn test_move_active_tab_single_tab_to_existing() {
        let mut ws = create_hsplit_workspace();
        // Switch to pane 2 which has only 1 tab
        ws.active_pane_id = 2;

        let result = ws.move_active_tab(Direction::Left);

        use crate::pane_layout::MoveResult;
        assert!(matches!(result, MoveResult::MovedToExisting { target_pane_id: 1, .. }));

        // Focus should follow to left pane
        assert_eq!(ws.active_pane_id, 1);

        // Since pane 2 is now empty, it should be cleaned up
        // The tree should collapse back to a single pane
        assert_eq!(ws.pane_root.pane_count(), 1);
    }

    // =========================================================================
    // find_fallback_focus Tests (Chunk: docs/chunks/pane_close_last_tab)
    // =========================================================================

    #[test]
    fn test_find_fallback_focus_single_pane_returns_none() {
        let ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        // Single pane has no fallback
        assert_eq!(ws.find_fallback_focus(), None);
    }

    #[test]
    fn test_find_fallback_focus_hsplit_prefers_right() {
        let ws = create_hsplit_workspace();
        // Active is pane 1 (left), should find pane 2 (right) as fallback
        // because Right is checked before Left
        let fallback = ws.find_fallback_focus();
        assert_eq!(fallback, Some(2));
    }

    #[test]
    fn test_find_fallback_focus_from_right_pane() {
        let mut ws = create_hsplit_workspace();
        ws.active_pane_id = 2; // Switch to right pane
        // Active is pane 2 (right), should find pane 1 (left) as fallback
        let fallback = ws.find_fallback_focus();
        assert_eq!(fallback, Some(1));
    }

    #[test]
    fn test_find_fallback_focus_nested_layout() {
        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        // Create HSplit(Pane[A], VSplit(Pane[B], Pane[C]))
        let mut pane_a = Pane::new(1, 1);
        pane_a.add_tab(Tab::empty_file(1, TEST_LINE_HEIGHT));

        let mut pane_b = Pane::new(2, 1);
        pane_b.add_tab(Tab::empty_file(2, TEST_LINE_HEIGHT));

        let mut pane_c = Pane::new(3, 1);
        pane_c.add_tab(Tab::empty_file(3, TEST_LINE_HEIGHT));

        ws.pane_root = PaneLayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayoutNode::Leaf(pane_a)),
            second: Box::new(PaneLayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PaneLayoutNode::Leaf(pane_b)),
                second: Box::new(PaneLayoutNode::Leaf(pane_c)),
            }),
        };
        ws.active_pane_id = 2; // Focus on pane B

        // From B, should find an adjacent pane (A or C)
        let fallback = ws.find_fallback_focus();
        assert!(fallback.is_some());
        // Direction order is Right, Left, Down, Up
        // From B: Right is no pane (edge), Left is A, Down is C
        // So should find A (Left is checked before Down)
        assert_eq!(fallback, Some(1)); // Pane A
    }

    // =========================================================================
    // Welcome Screen Tests (Chunk: docs/chunks/welcome_file_backed)
    // =========================================================================

    #[test]
    fn test_welcome_screen_not_shown_for_empty_file_backed_tab() {
        // Create an editor with a file-backed empty tab
        let mut editor = Editor::new(TEST_LINE_HEIGHT);

        // Replace the default empty tab with a file-backed empty tab
        let ws = editor.active_workspace_mut().unwrap();
        let empty_buffer = TextBuffer::new();
        let file_backed_tab = Tab::new_file(
            99,
            empty_buffer,
            "empty.txt".to_string(),
            Some(PathBuf::from("/test/empty.txt")),
            TEST_LINE_HEIGHT,
        );
        // Clear existing tabs and add our file-backed tab
        ws.close_tab(0); // Close the first tab by index
        ws.add_tab(file_backed_tab);

        // The welcome screen should NOT be shown for file-backed tabs (even if empty)
        assert!(!editor.should_show_welcome_screen());
    }

    #[test]
    fn test_welcome_screen_shown_for_empty_unassociated_tab() {
        // Create an editor with a standard empty (unassociated) tab
        let editor = Editor::new(TEST_LINE_HEIGHT);

        // The default editor creates an empty, unassociated tab
        // The welcome screen SHOULD be shown for this case
        assert!(editor.should_show_welcome_screen());
    }
}
