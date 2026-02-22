// Chunk: docs/chunks/workspace_model - Workspace model and left rail UI
// Chunk: docs/chunks/agent_lifecycle - Agent lifecycle tracking for Composer-like workflows
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

use crate::viewport::Viewport;
use lite_edit_buffer::{BufferView, TextBuffer};
use lite_edit_terminal::{AgentConfig, AgentHandle, AgentState, TerminalBuffer};

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
/// A tab within a workspace.
///
/// Each tab owns its own buffer and viewport (for independent scroll positions).
#[derive(Debug)]
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
}

// =============================================================================
// Workspace
// =============================================================================

// Chunk: docs/chunks/content_tab_bar - Owns tab list and tab_bar_view_offset for horizontal scroll
/// A workspace containing multiple tabs.
///
/// Each workspace represents an independent working context (e.g., a worktree,
/// an agent session, or a standalone editing environment).
pub struct Workspace {
    /// Unique identifier for this workspace
    pub id: WorkspaceId,
    /// Display label (branch name, project name, etc.)
    pub label: String,
    /// The root path for this workspace (typically the worktree root)
    pub root_path: PathBuf,
    /// The tabs in this workspace
    pub tabs: Vec<Tab>,
    /// Index of the currently active tab
    pub active_tab: usize,
    /// Status indicator for the left rail
    pub status: WorkspaceStatus,
    /// The agent running in this workspace (if any).
    ///
    /// When an agent is attached, its terminal is accessible via `agent_terminal()`.
    /// The first tab is typically an `AgentTerminal` placeholder that renders from here.
    pub agent: Option<AgentHandle>,
    // Chunk: docs/chunks/content_tab_bar - Tab bar scrolling
    /// Horizontal scroll offset for tab bar overflow (in pixels)
    pub tab_bar_view_offset: f32,
}

impl Workspace {
    /// Creates a new workspace with no tabs.
    pub fn new(id: WorkspaceId, label: String, root_path: PathBuf) -> Self {
        Self {
            id,
            label,
            root_path,
            tabs: Vec::new(),
            active_tab: 0,
            status: WorkspaceStatus::Idle,
            agent: None,
            tab_bar_view_offset: 0.0,
        }
    }

    /// Creates a new workspace with a single empty tab.
    pub fn with_empty_tab(id: WorkspaceId, tab_id: TabId, label: String, root_path: PathBuf, line_height: f32) -> Self {
        let tab = Tab::empty_file(tab_id, line_height);
        Self {
            id,
            label,
            root_path,
            tabs: vec![tab],
            active_tab: 0,
            status: WorkspaceStatus::Idle,
            agent: None,
            tab_bar_view_offset: 0.0,
        }
    }

    /// Adds a tab to the workspace.
    pub fn add_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        // Optionally switch to the new tab
        self.active_tab = self.tabs.len() - 1;
    }

    /// Closes a tab at the given index, returning the removed tab.
    ///
    /// Returns `None` if the index is out of bounds.
    /// After closing, the active tab is adjusted to remain valid.
    pub fn close_tab(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }

        let removed = self.tabs.remove(index);

        // Adjust active_tab to remain valid
        if !self.tabs.is_empty() {
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > index {
                self.active_tab = self.active_tab.saturating_sub(1);
            }
        } else {
            self.active_tab = 0;
        }

        Some(removed)
    }

    /// Returns a reference to the active tab, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    /// Returns a mutable reference to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Switches to the tab at the given index.
    ///
    /// Does nothing if the index is out of bounds. When switching to a new tab,
    /// clears its unread state.
    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
            // Chunk: docs/chunks/content_tab_bar - Clear unread when switching tabs
            self.tabs[index].clear_unread();
        }
    }

    /// Returns the number of tabs in this workspace.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
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
    /// Creates an `AgentHandle` and adds an `AgentTerminal` tab as the first tab.
    /// The agent terminal is pinned (always at index 0).
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

        // Insert at the beginning (pinned position)
        self.tabs.insert(0, agent_tab);

        // Adjust active_tab index if needed
        if !self.tabs.is_empty() && self.active_tab > 0 {
            self.active_tab += 1;
        }

        // Switch to the agent tab
        self.active_tab = 0;

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
    /// Polls PTY events for all standalone terminal tabs.
    ///
    /// This method also handles auto-follow behavior: when the viewport is at
    /// the bottom before polling, new output will advance the viewport to keep
    /// showing the latest content.
    ///
    /// Returns true if any terminal had output.
    pub fn poll_standalone_terminals(&mut self) -> bool {
        use lite_edit_buffer::BufferView;

        let mut had_events = false;
        for tab in &mut self.tabs {
            if let Some((terminal, viewport)) = tab.terminal_and_viewport_mut() {
                // Track if we're at bottom before polling (for auto-follow)
                // Also track if we're in alt screen (no auto-follow in alt screen)
                let was_at_bottom = viewport.is_at_bottom(terminal.line_count());
                let was_alt_screen = terminal.is_alt_screen();

                if terminal.poll_events() {
                    had_events = true;

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
            }
        }
        had_events
    }
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("root_path", &self.root_path)
            .field("tabs", &self.tabs)
            .field("active_tab", &self.active_tab)
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
#[derive(Debug)]
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
        };

        // Create an initial empty workspace
        let root_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        editor.new_workspace_internal("untitled".to_string(), root_path, true);

        editor
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

    /// Internal method to create a workspace.
    fn new_workspace_internal(&mut self, label: String, root_path: PathBuf, with_tab: bool) -> WorkspaceId {
        let ws_id = self.gen_workspace_id();
        let workspace = if with_tab {
            let tab_id = self.gen_tab_id();
            Workspace::with_empty_tab(ws_id, tab_id, label, root_path, self.line_height)
        } else {
            Workspace::new(ws_id, label, root_path)
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

    // Chunk: docs/chunks/welcome_screen - Welcome screen visibility check
    /// Returns true if the welcome screen should be shown for the active tab.
    ///
    /// The welcome screen is displayed when:
    /// - There is an active workspace with an active tab
    /// - The active tab is a File tab (not Terminal or AgentOutput)
    /// - The tab's TextBuffer is empty
    ///
    /// This provides a Vim-style welcome/intro screen on initial launch and
    /// when creating new empty tabs.
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
    // Workspace Tests
    // =========================================================================

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));

        assert_eq!(ws.id, 1);
        assert_eq!(ws.label, "test");
        assert_eq!(ws.root_path, PathBuf::from("/test"));
        assert!(ws.tabs.is_empty());
        assert_eq!(ws.status, WorkspaceStatus::Idle);
    }

    #[test]
    fn test_workspace_with_empty_tab() {
        let ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);

        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active_tab, 0);
        assert!(ws.active_tab().is_some());
    }

    #[test]
    fn test_workspace_add_tab() {
        let mut ws = Workspace::new(1, "test".to_string(), PathBuf::from("/test"));
        let tab = Tab::empty_file(1, TEST_LINE_HEIGHT);

        ws.add_tab(tab);

        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active_tab, 0);
    }

    #[test]
    fn test_workspace_close_tab() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        let tab2 = Tab::empty_file(2, TEST_LINE_HEIGHT);
        ws.add_tab(tab2);

        assert_eq!(ws.tabs.len(), 2);
        assert_eq!(ws.active_tab, 1); // add_tab switches to new tab

        let removed = ws.close_tab(1);
        assert!(removed.is_some());
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.active_tab, 0);
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
        assert_eq!(ws.active_tab, 1);
    }

    #[test]
    fn test_workspace_switch_tab() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        ws.add_tab(Tab::empty_file(2, TEST_LINE_HEIGHT));
        ws.switch_tab(0);

        assert_eq!(ws.active_tab, 0);
    }

    #[test]
    fn test_workspace_switch_tab_invalid() {
        let mut ws = Workspace::with_empty_tab(1, 1, "test".to_string(), PathBuf::from("/test"), TEST_LINE_HEIGHT);
        ws.switch_tab(10); // Out of bounds

        assert_eq!(ws.active_tab, 0); // Unchanged
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
        assert_eq!(ws.tabs.len(), 1);
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
        assert!(ws.tabs[1].unread); // Still unread before switch
        ws.switch_tab(1);
        assert!(!ws.tabs[1].unread); // Cleared after switch
    }
}
