// Chunk: docs/chunks/agent_lifecycle - Agent lifecycle tracking for Composer-like workflows
//! Agent handle and state machine for tracking agent lifecycle.
//!
//! This module provides `AgentHandle`, a thin wrapper around `TerminalBuffer` that
//! infers agent lifecycle state from PTY behavior. The key insight: the agent IS
//! its terminal. There's no separate agent protocol — state is inferred from
//! terminal output patterns and process lifecycle.
//!
//! # Agent State Machine
//!
//! ```text
//! Starting ──(output)──> Running ──(idle)──> NeedsInput ──(long idle)──> Stale
//!    │                      │                    │                         │
//!    │                      │                    │                         │
//!    └──────(exit)──────────┴──────(exit)────────┴────────(exit)───────────┘
//!                                    │
//!                                    v
//!                                  Exited
//! ```
//!
//! # Example
//!
//! ```no_run
//! use lite_edit_terminal::{AgentHandle, AgentConfig, AgentState};
//! use std::path::PathBuf;
//! use std::time::Duration;
//!
//! let config = AgentConfig {
//!     command: "claude".to_string(),
//!     args: vec![],
//!     cwd: PathBuf::from("/home/user/project"),
//!     needs_input_timeout: Duration::from_secs(5),
//!     stale_timeout: Duration::from_secs(60),
//! };
//!
//! let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();
//!
//! // Poll each frame
//! loop {
//!     agent.poll();
//!     match agent.state() {
//!         AgentState::NeedsInput { .. } => {
//!             // Show yellow indicator
//!         }
//!         AgentState::Exited { code } => {
//!             // Show green (code 0) or red (non-zero) indicator
//!             break;
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::TerminalBuffer;

// =============================================================================
// AgentState
// =============================================================================

/// The lifecycle state of an agent.
///
/// State is inferred from PTY behavior:
/// - Output flowing → Running
/// - Output stopped for N seconds → NeedsInput
/// - NeedsInput for M seconds → Stale
/// - Process exited → Exited with exit code
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// Process just spawned, no output yet.
    Starting,
    /// Output is flowing, agent is working autonomously.
    Running,
    /// Output has stopped but process is alive - likely waiting for user input.
    NeedsInput {
        /// When the agent transitioned to this state.
        since: Instant,
    },
    /// Agent has been waiting for input too long.
    Stale {
        /// When the agent transitioned to this state.
        since: Instant,
    },
    /// Process has exited.
    Exited {
        /// The process exit code (0 = success, non-zero = error).
        code: i32,
    },
}

impl AgentState {
    /// Returns true if this is a terminal state (Exited).
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Exited { .. })
    }

    /// Returns true if the agent is actively running (not waiting or exited).
    pub fn is_active(&self) -> bool {
        matches!(self, AgentState::Starting | AgentState::Running)
    }
}

// =============================================================================
// AgentConfig
// =============================================================================

/// Configuration for spawning an agent.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// The command to run (e.g., "claude", "aider").
    pub command: String,
    /// Arguments for the command.
    pub args: Vec<String>,
    /// Working directory for the agent.
    pub cwd: PathBuf,
    /// Duration after which output silence transitions Running → NeedsInput.
    ///
    /// Default: 5 seconds.
    pub needs_input_timeout: Duration,
    /// Duration after which NeedsInput transitions to Stale.
    ///
    /// Default: 60 seconds.
    pub stale_timeout: Duration,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            cwd: PathBuf::from("."),
            needs_input_timeout: Duration::from_secs(5),
            stale_timeout: Duration::from_secs(60),
        }
    }
}

impl AgentConfig {
    /// Creates a new agent config with the given command.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            ..Default::default()
        }
    }

    /// Sets the arguments for the command.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Sets the working directory.
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = cwd;
        self
    }

    /// Sets the needs-input timeout.
    pub fn with_needs_input_timeout(mut self, timeout: Duration) -> Self {
        self.needs_input_timeout = timeout;
        self
    }

    /// Sets the stale timeout.
    pub fn with_stale_timeout(mut self, timeout: Duration) -> Self {
        self.stale_timeout = timeout;
        self
    }
}

// =============================================================================
// AgentStateMachine (pure state logic, testable)
// =============================================================================

/// Pure state machine for agent lifecycle transitions.
///
/// This is separated from `AgentHandle` to enable isolated unit testing
/// without requiring PTY/OS interaction. All timing is injected via
/// `Instant` parameters.
#[derive(Debug)]
pub struct AgentStateMachine {
    /// Current state.
    state: AgentState,
    /// When the current state was entered.
    state_entered_at: Instant,
    /// When output was last received (None if never).
    last_output_at: Option<Instant>,
    /// Configuration for timeouts.
    config: AgentConfig,
}

impl AgentStateMachine {
    /// Creates a new state machine in the Starting state.
    pub fn new(config: AgentConfig, now: Instant) -> Self {
        Self {
            state: AgentState::Starting,
            state_entered_at: now,
            last_output_at: None,
            config,
        }
    }

    /// Returns the current state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Returns when the current state was entered.
    pub fn state_entered_at(&self) -> Instant {
        self.state_entered_at
    }

    /// Returns when output was last received.
    pub fn last_output_at(&self) -> Option<Instant> {
        self.last_output_at
    }

    /// Called when PTY output is received.
    ///
    /// Transitions:
    /// - Starting → Running
    /// - NeedsInput → Running
    /// - Stale → Running (unlikely but possible)
    pub fn on_output(&mut self, now: Instant) {
        self.last_output_at = Some(now);

        match self.state {
            AgentState::Starting | AgentState::NeedsInput { .. } | AgentState::Stale { .. } => {
                self.state = AgentState::Running;
                self.state_entered_at = now;
            }
            AgentState::Running => {
                // Stay in Running, just update last_output_at
            }
            AgentState::Exited { .. } => {
                // Cannot transition out of Exited via output
            }
        }
    }

    /// Called periodically to check for timeout-based transitions.
    ///
    /// Transitions:
    /// - Running → NeedsInput (after needs_input_timeout of silence)
    /// - NeedsInput → Stale (after stale_timeout)
    pub fn tick(&mut self, now: Instant) {
        match &self.state {
            AgentState::Starting => {
                // Check if we've been starting too long (treat as NeedsInput)
                if now.duration_since(self.state_entered_at) > self.config.needs_input_timeout {
                    self.state = AgentState::NeedsInput { since: now };
                    self.state_entered_at = now;
                }
            }
            AgentState::Running => {
                // Check for idle timeout
                if let Some(last_output) = self.last_output_at {
                    if now.duration_since(last_output) > self.config.needs_input_timeout {
                        self.state = AgentState::NeedsInput { since: now };
                        self.state_entered_at = now;
                    }
                }
            }
            AgentState::NeedsInput { since } => {
                // Check for stale timeout
                if now.duration_since(*since) > self.config.stale_timeout {
                    self.state = AgentState::Stale { since: now };
                    self.state_entered_at = now;
                }
            }
            AgentState::Stale { .. } | AgentState::Exited { .. } => {
                // No timeout transitions from these states
            }
        }
    }

    /// Called when the process exits.
    ///
    /// Always transitions to Exited.
    pub fn on_exit(&mut self, exit_code: i32, now: Instant) {
        self.state = AgentState::Exited { code: exit_code };
        self.state_entered_at = now;
    }

    /// Resets the state machine to Starting (for restart).
    pub fn reset(&mut self, now: Instant) {
        self.state = AgentState::Starting;
        self.state_entered_at = now;
        self.last_output_at = None;
    }
}

// =============================================================================
// AgentHandle
// =============================================================================

/// A handle to an agent process and its terminal.
///
/// This wraps a `TerminalBuffer` and adds lifecycle state inference.
/// Poll the handle each frame to update state based on PTY activity.
pub struct AgentHandle {
    /// The terminal buffer (PTY + emulator).
    terminal: TerminalBuffer,
    /// The state machine tracking lifecycle.
    state_machine: AgentStateMachine,
    /// Configuration for this agent.
    config: AgentConfig,
}

impl AgentHandle {
    /// Spawns a new agent with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration including command, args, and timeouts
    /// * `cols` - Terminal width in columns
    /// * `rows` - Terminal height in rows
    ///
    /// # Returns
    ///
    /// The agent handle, or an error if spawning failed.
    pub fn spawn(config: AgentConfig, cols: usize, rows: usize) -> std::io::Result<Self> {
        let mut terminal = TerminalBuffer::new(cols, rows, 5000);

        // Convert args to &str for spawn_command
        let args_refs: Vec<&str> = config.args.iter().map(|s| s.as_str()).collect();

        terminal.spawn_command(&config.command, &args_refs, &config.cwd)?;

        let state_machine = AgentStateMachine::new(config.clone(), Instant::now());

        Ok(Self {
            terminal,
            state_machine,
            config,
        })
    }

    /// Polls PTY events and updates the state machine.
    ///
    /// Call this each frame. Returns true if any PTY output was processed.
    pub fn poll(&mut self) -> bool {
        let now = Instant::now();

        // Poll for PTY events
        let had_output = self.terminal.poll_events();

        // Update state machine based on what happened
        if had_output {
            self.state_machine.on_output(now);
        }

        // Check for process exit
        if let Some(exit_code) = self.terminal.try_wait() {
            self.state_machine.on_exit(exit_code, now);
        }

        // Tick for timeout-based transitions
        self.state_machine.tick(now);

        had_output
    }

    /// Returns the current agent state.
    pub fn state(&self) -> &AgentState {
        self.state_machine.state()
    }

    /// Returns a reference to the terminal buffer.
    pub fn terminal(&self) -> &TerminalBuffer {
        &self.terminal
    }

    /// Returns a mutable reference to the terminal buffer.
    pub fn terminal_mut(&mut self) -> &mut TerminalBuffer {
        &mut self.terminal
    }

    /// Writes input to the agent's terminal stdin.
    pub fn write_input(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.terminal.write_input(data)
    }

    /// Resizes the terminal.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.terminal.resize(cols, rows);
    }

    /// Returns the agent configuration.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Returns true if the agent can be restarted (must be in Exited state).
    pub fn can_restart(&self) -> bool {
        matches!(self.state(), AgentState::Exited { .. })
    }

    /// Restarts the agent from Exited state.
    ///
    /// This preserves the terminal scrollback and inserts a visual separator,
    /// then spawns a new process with the same configuration.
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an error if not in Exited state or spawn fails.
    pub fn restart(&mut self) -> std::io::Result<()> {
        if !self.can_restart() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Agent must be in Exited state to restart",
            ));
        }

        // Insert a visual separator into the terminal
        // We do this by writing escape sequences to create a separator line
        // Note: We can't write to the PTY since it's closed, but we can
        // process the bytes directly through the terminal emulator
        // For now, we just reset and spawn fresh
        // TODO: Preserve scrollback with separator (would require feeding bytes
        // directly to the terminal emulator)

        // Create a new terminal and spawn the process
        let (cols, rows) = self.terminal.size();
        let mut new_terminal = TerminalBuffer::new(cols, rows, 5000);

        let args_refs: Vec<&str> = self.config.args.iter().map(|s| s.as_str()).collect();
        new_terminal.spawn_command(&self.config.command, &args_refs, &self.config.cwd)?;

        // Replace our terminal with the new one
        self.terminal = new_terminal;

        // Reset the state machine
        self.state_machine.reset(Instant::now());

        Ok(())
    }

    /// Stops the running agent.
    ///
    /// Kills the process (SIGKILL) and transitions state to Exited.
    ///
    /// # Returns
    ///
    /// Ok(()) if the agent was stopped, or an error if already exited.
    pub fn stop(&mut self) -> std::io::Result<()> {
        if matches!(self.state(), AgentState::Exited { .. }) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Agent is already exited",
            ));
        }

        // Kill the process
        self.terminal.kill()?;

        // Mark as exited with SIGKILL exit code (-9)
        self.state_machine.on_exit(-9, Instant::now());

        Ok(())
    }
}

impl std::fmt::Debug for AgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHandle")
            .field("state", self.state())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // AgentState tests
    // =========================================================================

    #[test]
    fn test_state_is_terminal() {
        assert!(!AgentState::Starting.is_terminal());
        assert!(!AgentState::Running.is_terminal());
        assert!(!AgentState::NeedsInput { since: Instant::now() }.is_terminal());
        assert!(!AgentState::Stale { since: Instant::now() }.is_terminal());
        assert!(AgentState::Exited { code: 0 }.is_terminal());
        assert!(AgentState::Exited { code: 1 }.is_terminal());
    }

    #[test]
    fn test_state_is_active() {
        assert!(AgentState::Starting.is_active());
        assert!(AgentState::Running.is_active());
        assert!(!AgentState::NeedsInput { since: Instant::now() }.is_active());
        assert!(!AgentState::Stale { since: Instant::now() }.is_active());
        assert!(!AgentState::Exited { code: 0 }.is_active());
    }

    // =========================================================================
    // AgentConfig tests
    // =========================================================================

    #[test]
    fn test_config_builder() {
        let config = AgentConfig::new("claude")
            .with_args(vec!["--model".into(), "opus".into()])
            .with_cwd(PathBuf::from("/home/user"))
            .with_needs_input_timeout(Duration::from_secs(10))
            .with_stale_timeout(Duration::from_secs(120));

        assert_eq!(config.command, "claude");
        assert_eq!(config.args, vec!["--model", "opus"]);
        assert_eq!(config.cwd, PathBuf::from("/home/user"));
        assert_eq!(config.needs_input_timeout, Duration::from_secs(10));
        assert_eq!(config.stale_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_config_default() {
        let config = AgentConfig::default();
        assert!(config.command.is_empty());
        assert!(config.args.is_empty());
        assert_eq!(config.needs_input_timeout, Duration::from_secs(5));
        assert_eq!(config.stale_timeout, Duration::from_secs(60));
    }

    // =========================================================================
    // AgentStateMachine tests (pure state logic)
    // =========================================================================

    fn test_config() -> AgentConfig {
        AgentConfig {
            command: "test".into(),
            args: vec![],
            cwd: PathBuf::from("/tmp"),
            needs_input_timeout: Duration::from_millis(100),
            stale_timeout: Duration::from_millis(200),
        }
    }

    #[test]
    fn test_initial_state_is_starting() {
        let now = Instant::now();
        let sm = AgentStateMachine::new(test_config(), now);
        assert!(matches!(sm.state(), AgentState::Starting));
    }

    #[test]
    fn test_starting_to_running_on_output() {
        let now = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), now);

        sm.on_output(now);

        assert!(matches!(sm.state(), AgentState::Running));
    }

    #[test]
    fn test_running_to_needs_input_after_timeout() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        // Transition to Running
        sm.on_output(start);
        assert!(matches!(sm.state(), AgentState::Running));

        // Tick before timeout - should stay Running
        let before_timeout = start + Duration::from_millis(50);
        sm.tick(before_timeout);
        assert!(matches!(sm.state(), AgentState::Running));

        // Tick after timeout - should transition to NeedsInput
        let after_timeout = start + Duration::from_millis(150);
        sm.tick(after_timeout);
        assert!(matches!(sm.state(), AgentState::NeedsInput { .. }));
    }

    #[test]
    fn test_needs_input_to_running_on_output() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        // Get to NeedsInput state
        sm.on_output(start);
        let later = start + Duration::from_millis(150);
        sm.tick(later);
        assert!(matches!(sm.state(), AgentState::NeedsInput { .. }));

        // New output should transition back to Running
        sm.on_output(later);
        assert!(matches!(sm.state(), AgentState::Running));
    }

    #[test]
    fn test_needs_input_to_stale_after_timeout() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        // Get to NeedsInput state
        sm.on_output(start);
        let needs_input_time = start + Duration::from_millis(150);
        sm.tick(needs_input_time);
        assert!(matches!(sm.state(), AgentState::NeedsInput { .. }));

        // Tick after stale timeout
        let stale_time = needs_input_time + Duration::from_millis(250);
        sm.tick(stale_time);
        assert!(matches!(sm.state(), AgentState::Stale { .. }));
    }

    #[test]
    fn test_running_to_exited_on_exit() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        sm.on_output(start);
        assert!(matches!(sm.state(), AgentState::Running));

        sm.on_exit(0, start);
        assert!(matches!(sm.state(), AgentState::Exited { code: 0 }));
    }

    #[test]
    fn test_exited_with_nonzero_code() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        sm.on_output(start);
        sm.on_exit(1, start);

        assert!(matches!(sm.state(), AgentState::Exited { code: 1 }));
    }

    #[test]
    fn test_state_unchanged_when_tick_too_early() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        sm.on_output(start);
        assert!(matches!(sm.state(), AgentState::Running));

        // Tick immediately - should not transition
        sm.tick(start);
        assert!(matches!(sm.state(), AgentState::Running));

        // Tick 10ms later - still before timeout
        sm.tick(start + Duration::from_millis(10));
        assert!(matches!(sm.state(), AgentState::Running));
    }

    #[test]
    fn test_cannot_transition_from_exited() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        sm.on_exit(0, start);
        assert!(matches!(sm.state(), AgentState::Exited { code: 0 }));

        // Output should not change state
        sm.on_output(start);
        assert!(matches!(sm.state(), AgentState::Exited { code: 0 }));

        // Tick should not change state
        sm.tick(start + Duration::from_secs(100));
        assert!(matches!(sm.state(), AgentState::Exited { code: 0 }));
    }

    #[test]
    fn test_reset_returns_to_starting() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        // Get to Exited state
        sm.on_output(start);
        sm.on_exit(0, start);
        assert!(matches!(sm.state(), AgentState::Exited { .. }));

        // Reset should return to Starting
        sm.reset(start);
        assert!(matches!(sm.state(), AgentState::Starting));
        assert!(sm.last_output_at().is_none());
    }

    #[test]
    fn test_starting_to_needs_input_if_no_output() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        // If we never get output and timeout elapses, should go to NeedsInput
        // (This handles agents that produce no initial output, like `cat`)
        let after_timeout = start + Duration::from_millis(150);
        sm.tick(after_timeout);

        assert!(matches!(sm.state(), AgentState::NeedsInput { .. }));
    }

    #[test]
    fn test_stale_to_running_on_output() {
        let start = Instant::now();
        let mut sm = AgentStateMachine::new(test_config(), start);

        // Get to Stale state
        sm.on_output(start);
        sm.tick(start + Duration::from_millis(150)); // NeedsInput
        sm.tick(start + Duration::from_millis(400)); // Stale
        assert!(matches!(sm.state(), AgentState::Stale { .. }));

        // Output should transition back to Running
        sm.on_output(start + Duration::from_millis(500));
        assert!(matches!(sm.state(), AgentState::Running));
    }
}
