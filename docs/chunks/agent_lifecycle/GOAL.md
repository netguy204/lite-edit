---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/terminal/src/agent.rs
  - crates/terminal/src/lib.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/terminal/tests/agent_integration.rs
code_references:
  - ref: crates/terminal/src/agent.rs#AgentState
    implements: "Agent lifecycle states (Starting, Running, NeedsInput, Stale, Exited)"
  - ref: crates/terminal/src/agent.rs#AgentConfig
    implements: "Agent configuration (command, args, cwd, timeouts)"
  - ref: crates/terminal/src/agent.rs#AgentStateMachine
    implements: "Pure state machine logic for lifecycle transitions"
  - ref: crates/terminal/src/agent.rs#AgentStateMachine::on_output
    implements: "Starting→Running and NeedsInput→Running transitions"
  - ref: crates/terminal/src/agent.rs#AgentStateMachine::tick
    implements: "Running→NeedsInput and NeedsInput→Stale timeout transitions"
  - ref: crates/terminal/src/agent.rs#AgentStateMachine::on_exit
    implements: "Process exit detection and Exited state transition"
  - ref: crates/terminal/src/agent.rs#AgentHandle
    implements: "Wrapper around TerminalBuffer with lifecycle state inference"
  - ref: crates/terminal/src/agent.rs#AgentHandle::spawn
    implements: "Agent process spawning"
  - ref: crates/terminal/src/agent.rs#AgentHandle::poll
    implements: "Per-frame PTY polling and state machine updates"
  - ref: crates/terminal/src/agent.rs#AgentHandle::restart
    implements: "Agent restart from Exited state"
  - ref: crates/terminal/src/agent.rs#AgentHandle::stop
    implements: "Graceful agent stop (SIGTERM→SIGKILL pattern)"
  - ref: crates/editor/src/workspace.rs#TabBuffer
    implements: "Extended with Terminal and AgentTerminal variants"
  - ref: crates/editor/src/workspace.rs#Workspace::agent
    implements: "Optional AgentHandle field on workspace"
  - ref: crates/editor/src/workspace.rs#Workspace::compute_status
    implements: "AgentState to WorkspaceStatus mapping"
  - ref: crates/editor/src/workspace.rs#Workspace::launch_agent
    implements: "Agent spawning and pinned tab creation"
  - ref: crates/editor/src/workspace.rs#Workspace::restart_agent
    implements: "Workspace-level agent restart"
  - ref: crates/editor/src/workspace.rs#Workspace::stop_agent
    implements: "Workspace-level agent stop"
  - ref: crates/editor/src/workspace.rs#Workspace::poll_agent
    implements: "Per-frame agent polling with status update"
  - ref: crates/editor/src/editor_state.rs#EditorState::poll_agents
    implements: "Main loop integration for agent polling"
narrative: null
investigation: hierarchical_terminal_tabs
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_emulator
- workspace_model
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_forward_delete
- word_jump_navigation
---

# Chunk Goal

## Minor Goal

Implement `AgentHandle` — the thin wrapper around a `TerminalBuffer` that infers agent lifecycle state from PTY behavior and drives workspace status indicators. This is what makes lite-edit a Composer-like tool: you can run multiple AI coding agents in parallel workspaces and get notified when any of them needs your attention.

**Core insight from the investigation: the agent IS its terminal.** There's no separate agent protocol. An agent (Claude Code, aider, or any interactive CLI tool) is a process in a PTY. All interaction happens through the terminal. Agent state is *inferred* from terminal behavior.

**`AgentHandle`:**

```rust
struct AgentHandle {
    terminal: TerminalBuffer,     // the agent's PTY + terminal emulator
    state: AgentState,
    state_changed_at: Instant,
    config: AgentConfig,
}

enum AgentState {
    Starting,                     // process just spawned
    Running,                      // output flowing
    NeedsInput { since: Instant },// output stopped, process alive
    Stale { since: Instant },     // NeedsInput for too long
    Exited { code: i32 },         // process exited
}
```

**State inference logic:**
- **Starting → Running**: First PTY output received
- **Running → NeedsInput**: Output stops for >N seconds AND process is still alive (PTY idle detection). Configurable timeout (default: 5-10 seconds).
- **NeedsInput → Running**: New output appears (user provided input, agent resumed)
- **NeedsInput → Stale**: NeedsInput for >M seconds without user response (default: 60 seconds)
- **Running/NeedsInput → Exited**: Process exits (detected via `waitpid` or PTY EOF). Exit code determines Completed (code 0) vs Errored (code ≠ 0).

**Integration with workspace:**
- `Workspace.agent: Option<AgentHandle>`
- Agent state drives `WorkspaceStatus` which drives left rail indicator colors
- The agent's terminal is always the first (pinned) tab in the workspace

**Workspace operations:**
- **Launch agent**: User specifies command (e.g., `claude`, `aider`), agent is spawned in the workspace's worktree root, `AgentHandle` is created and attached
- **Restart agent**: From Exited/Errored state, re-spawn the process. Old scrollback is preserved (separator between sessions). State resets to Starting.
- **Stop agent**: Send SIGTERM, wait briefly, SIGKILL if needed. State → Exited.

## Success Criteria

- `AgentHandle` correctly infers all state transitions: Starting → Running → NeedsInput → Stale → Exited/Errored
- PTY idle detection works: when a shell is waiting for input, state transitions to NeedsInput after the configured timeout
- When new output appears after NeedsInput, state transitions back to Running
- Process exit is detected and state transitions to Exited with correct exit code
- Agent state drives `WorkspaceStatus` — left rail indicator updates in real time:
  - 🟢 while Running
  - 🟡 (pulsing) when NeedsInput
  - 🟠 when Stale
  - ✅ when Exited(0)
  - 🔴 when Exited(non-zero)
- Launching an agent in a workspace creates an `AgentHandle` and pins the agent terminal as the first tab
- Restarting a completed/errored agent re-spawns the process in the same workspace
- Stopping a running agent sends SIGTERM and transitions to Exited
- False positive rate for NeedsInput is low: an agent doing slow computation (no output for 10+ seconds) is an acceptable false positive, but it should be rare with a well-tuned timeout
- The agent lifecycle is fully agent-agnostic — works with Claude Code, aider, or a plain shell