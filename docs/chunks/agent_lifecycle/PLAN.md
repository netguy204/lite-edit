<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The `AgentHandle` is a thin wrapper around `TerminalBuffer` that adds state inference from PTY behavior. The key insight from the investigation: **the agent IS its terminal**. There's no separate agent protocol — agent state is inferred from terminal output patterns and process lifecycle.

**Architecture:**

1. **AgentHandle struct**: Wraps `TerminalBuffer` + state machine + timing. Lives in the `terminal` crate (alongside `TerminalBuffer`) since the agent lifecycle is tightly coupled to PTY behavior.

2. **State inference**: The state machine is driven by three inputs:
   - PTY output events (new bytes → Running)
   - Passage of time (no output for N seconds → NeedsInput → Stale)
   - Process exit (detected via `try_wait()` → Exited with exit code)

3. **Integration with workspace**: `Workspace.agent: Option<AgentHandle>`. The agent's terminal is automatically the first (pinned) tab in the workspace. Agent state drives `WorkspaceStatus` for left rail indicators.

4. **Workspace operations**: Launch agent, restart agent (from Exited/Errored), stop agent (SIGTERM → SIGKILL).

**Testing strategy per TESTING_PHILOSOPHY.md:**

- State transitions are pure state machine logic — fully testable without PTY/OS using injected time source
- The `AgentStateMachine` is separate from `AgentHandle` for isolated testing
- Integration tests spawn real PTY processes to verify end-to-end behavior (similar to existing `crates/terminal/tests/`)

**Dependencies:**
- `TerminalBuffer`, `PtyHandle`, `TerminalEvent` from `crates/terminal` (completed in terminal_emulator chunk)
- `Workspace`, `WorkspaceStatus`, `TabBuffer`, `Tab` from `crates/editor/src/workspace.rs` (completed in workspace_model chunk)

## Sequence

### Step 1: Define AgentState enum and AgentConfig struct

Add new types to `crates/terminal/src/lib.rs` (re-export) with implementation in a new file `crates/terminal/src/agent.rs`:

```rust
// crates/terminal/src/agent.rs

pub enum AgentState {
    Starting,                       // Process just spawned, no output yet
    Running,                        // Output flowing
    NeedsInput { since: Instant },  // Output stopped, process alive
    Stale { since: Instant },       // NeedsInput for too long
    Exited { code: i32 },           // Process exited
}

pub struct AgentConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub needs_input_timeout: Duration,  // Default: 5 seconds
    pub stale_timeout: Duration,        // Default: 60 seconds
}
```

**Output**: Types that capture agent lifecycle states per the goal.

Location: `crates/terminal/src/agent.rs`

### Step 2: Implement AgentStateMachine (pure state logic)

Create a testable state machine that encapsulates transition logic without any I/O:

```rust
pub struct AgentStateMachine {
    state: AgentState,
    state_changed_at: Instant,
    last_output_at: Option<Instant>,
    config: AgentConfig,
}

impl AgentStateMachine {
    pub fn new(config: AgentConfig, now: Instant) -> Self;

    /// Called when PTY output is received
    pub fn on_output(&mut self, now: Instant);

    /// Called periodically to check for timeout transitions
    pub fn tick(&mut self, now: Instant);

    /// Called when process exits
    pub fn on_exit(&mut self, exit_code: i32, now: Instant);

    pub fn state(&self) -> &AgentState;
}
```

**State transition rules:**
- `Starting → Running`: First `on_output()` call
- `Running → NeedsInput`: `tick()` when `now - last_output_at > needs_input_timeout` AND process alive
- `NeedsInput → Running`: `on_output()` called
- `NeedsInput → Stale`: `tick()` when `now - since > stale_timeout`
- `* → Exited`: `on_exit()` called

**Tests (TDD):**
1. Test `Starting → Running` on first output
2. Test `Running → NeedsInput` after idle timeout
3. Test `NeedsInput → Running` on new output
4. Test `NeedsInput → Stale` after stale timeout
5. Test `Running → Exited(0)` on exit code 0
6. Test `Running → Exited(1)` on non-zero exit
7. Test state unchanged when `tick()` called too early

Location: `crates/terminal/src/agent.rs`

### Step 3: Implement AgentHandle struct

Create the main `AgentHandle` that wraps `TerminalBuffer` and `AgentStateMachine`:

```rust
pub struct AgentHandle {
    terminal: TerminalBuffer,
    state_machine: AgentStateMachine,
    config: AgentConfig,
}

impl AgentHandle {
    /// Creates a new agent handle and spawns the agent process.
    pub fn spawn(config: AgentConfig, cols: usize, rows: usize) -> io::Result<Self>;

    /// Polls PTY events and updates state. Call each frame.
    pub fn poll(&mut self) -> bool;

    /// Writes input to the agent's terminal stdin.
    pub fn write_input(&mut self, data: &[u8]) -> io::Result<()>;

    /// Returns current agent state.
    pub fn state(&self) -> &AgentState;

    /// Returns reference to the terminal buffer for rendering.
    pub fn terminal(&self) -> &TerminalBuffer;
    pub fn terminal_mut(&mut self) -> &mut TerminalBuffer;

    /// Resizes the terminal.
    pub fn resize(&mut self, cols: usize, rows: usize);
}
```

The `poll()` method:
1. Calls `terminal.poll_events()` to process PTY output
2. If output was received, calls `state_machine.on_output(Instant::now())`
3. Calls `state_machine.tick(Instant::now())` for timeout checks
4. Checks `terminal.try_wait()` for process exit → `state_machine.on_exit()`

Location: `crates/terminal/src/agent.rs`

### Step 4: Add restart and stop operations to AgentHandle

Implement agent lifecycle management:

```rust
impl AgentHandle {
    /// Stops the running agent (SIGTERM, then SIGKILL after timeout).
    /// Transitions state to Exited.
    pub fn stop(&mut self) -> io::Result<()>;

    /// Restarts the agent from Exited state.
    /// Preserves terminal scrollback with a separator.
    /// Resets state to Starting.
    pub fn restart(&mut self) -> io::Result<()>;

    /// Returns true if the agent can be restarted (Exited state only).
    pub fn can_restart(&self) -> bool;
}
```

**Stop behavior:**
1. Send SIGTERM to the process
2. Wait briefly (100ms)
3. Check if process exited; if not, SIGKILL
4. Transition to `Exited { code: -15 }` (SIGTERM exit code)

**Restart behavior:**
1. Assert state is `Exited`
2. Insert visual separator in terminal scrollback (e.g., "─── Session restarted ───")
3. Spawn new process with same config
4. Reset state to `Starting`

Location: `crates/terminal/src/agent.rs`

### Step 5: Extend TabBuffer enum to support Terminal variant

Currently `TabBuffer` only has `File(TextBuffer)`. Add `Terminal(TerminalBuffer)` variant:

```rust
// In crates/editor/src/workspace.rs

pub enum TabBuffer {
    File(TextBuffer),
    Terminal(TerminalBuffer),  // NEW
}
```

Update all match arms in `TabBuffer` impl:
- `as_buffer_view()` / `as_buffer_view_mut()`: Both `TextBuffer` and `TerminalBuffer` impl `BufferView`
- `as_text_buffer()` / `as_text_buffer_mut()`: Return `None` for Terminal variant
- Add new `as_terminal_buffer()` / `as_terminal_buffer_mut()` methods

Location: `crates/editor/src/workspace.rs`

### Step 6: Add agent field to Workspace and derive WorkspaceStatus

Extend `Workspace` to hold an optional agent:

```rust
// In crates/editor/src/workspace.rs

use lite_edit_terminal::AgentHandle;

pub struct Workspace {
    // ... existing fields ...
    pub agent: Option<AgentHandle>,  // NEW
}

impl Workspace {
    /// Computes workspace status from agent state.
    /// Returns Idle if no agent attached.
    pub fn compute_status(&self) -> WorkspaceStatus {
        match &self.agent {
            None => WorkspaceStatus::Idle,
            Some(agent) => match agent.state() {
                AgentState::Starting => WorkspaceStatus::Running,
                AgentState::Running => WorkspaceStatus::Running,
                AgentState::NeedsInput { .. } => WorkspaceStatus::NeedsInput,
                AgentState::Stale { .. } => WorkspaceStatus::Stale,
                AgentState::Exited { code: 0 } => WorkspaceStatus::Completed,
                AgentState::Exited { code: _ } => WorkspaceStatus::Errored,
            }
        }
    }

    /// Launches an agent in this workspace.
    /// Creates a pinned terminal tab for the agent.
    pub fn launch_agent(&mut self, config: AgentConfig, tab_id: TabId, cols: usize, rows: usize) -> io::Result<()>;

    /// Restarts the agent if in Exited state.
    pub fn restart_agent(&mut self) -> io::Result<()>;

    /// Stops the running agent.
    pub fn stop_agent(&mut self) -> io::Result<()>;
}
```

**launch_agent behavior:**
1. Assert no agent currently attached
2. Create `AgentHandle::spawn(config, cols, rows)`
3. Create `Tab` with `TabBuffer::Terminal(agent.terminal().clone())` — wait, we can't clone TerminalBuffer
4. **Revised**: The agent owns the terminal, but the tab needs to reference it. Solutions:
   - Store `AgentHandle` in workspace, and have tab reference it (lifetime complexity)
   - Store terminal in tab, have agent reference it (inverted ownership)
   - **Best**: `AgentHandle` owns `TerminalBuffer`, and when rendering, we get `&dyn BufferView` from `workspace.agent.as_ref().unwrap().terminal()`

Actually, re-examining: the tab system in workspace_model stores `TabBuffer` which owns the buffer. For agent terminals, we need a different approach. Options:

**Option A**: `TabBuffer::AgentTerminal` variant that stores nothing — rendering comes from `workspace.agent.terminal()`.

**Option B**: Agent stores `TerminalBuffer`, tab stores index/reference indicating "this is the agent tab".

**Option C**: Separate the terminal from the agent. Agent just manages state; terminal lives in the tab.

Going with **Option A** for simplicity:

```rust
pub enum TabBuffer {
    File(TextBuffer),
    Terminal(TerminalBuffer),      // Standalone terminal (no agent)
    AgentTerminal,                 // Placeholder — actual buffer is workspace.agent
}
```

When rendering a tab with `AgentTerminal`, the render loop checks `workspace.agent.terminal()`.

Location: `crates/editor/src/workspace.rs`

### Step 7: Wire agent polling into the main loop

Add agent polling to the per-frame update in `EditorState`:

```rust
// In crates/editor/src/editor_state.rs or wherever the main loop lives

impl EditorState {
    pub fn poll_agents(&mut self) {
        for workspace in &mut self.editor.workspaces {
            if let Some(ref mut agent) = workspace.agent {
                let had_events = agent.poll();
                if had_events {
                    // Terminal buffer is dirty — mark for redraw
                    // (Handled automatically via BufferView::take_dirty)
                }
                // Update workspace status from agent state
                workspace.status = workspace.compute_status();
            }
        }
    }
}
```

This should be called each frame before rendering.

Location: `crates/editor/src/editor_state.rs`

### Step 8: Add integration tests

Create integration tests that spawn real agent processes:

```rust
// crates/terminal/tests/agent_integration.rs

#[test]
fn test_agent_state_starting_to_running() {
    // Spawn an echo command
    let config = AgentConfig {
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        cwd: PathBuf::from("/tmp"),
        needs_input_timeout: Duration::from_secs(5),
        stale_timeout: Duration::from_secs(60),
    };
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Initial state is Starting
    assert!(matches!(agent.state(), AgentState::Starting));

    // Poll until we get output
    for _ in 0..100 {
        agent.poll();
        thread::sleep(Duration::from_millis(10));
        if matches!(agent.state(), AgentState::Running | AgentState::Exited { .. }) {
            break;
        }
    }

    // Should have transitioned through Running to Exited
    assert!(matches!(agent.state(), AgentState::Exited { code: 0 }));
}

#[test]
fn test_agent_needs_input_after_idle() {
    // Spawn cat (waits for input)
    let config = AgentConfig {
        command: "cat".to_string(),
        args: vec![],
        cwd: PathBuf::from("/tmp"),
        needs_input_timeout: Duration::from_millis(100),  // Short for testing
        stale_timeout: Duration::from_secs(60),
    };
    let mut agent = AgentHandle::spawn(config, 80, 24).unwrap();

    // Wait for Starting → Running (cat prints nothing, so may stay Starting)
    // Actually cat produces no output, so Starting → NeedsInput after timeout
    thread::sleep(Duration::from_millis(200));
    agent.poll();

    // Should be NeedsInput (or Stale if more time passed)
    assert!(matches!(agent.state(), AgentState::NeedsInput { .. } | AgentState::Stale { .. }));
}
```

Location: `crates/terminal/tests/agent_integration.rs`

### Step 9: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files we expect to create/modify:

```yaml
code_paths:
  - crates/terminal/src/agent.rs
  - crates/terminal/src/lib.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/terminal/tests/agent_integration.rs
```

Location: `docs/chunks/agent_lifecycle/GOAL.md`

## Dependencies

- **terminal_emulator chunk** (ACTIVE): Provides `TerminalBuffer`, `PtyHandle`, `TerminalEvent`. This chunk builds directly on top.
- **workspace_model chunk** (ACTIVE): Provides `Workspace`, `WorkspaceStatus`, `Tab`, `TabBuffer`. This chunk extends these types.
- **portable-pty**: Already a dependency of the terminal crate. Needed for process signaling (stop).
- **std::time::Instant**: For timing state transitions. No external crate needed.

## Risks and Open Questions

1. **NeedsInput false positives**: An agent doing slow computation (no output for 10+ seconds) will be incorrectly marked as NeedsInput. The investigation acknowledged this as an acceptable trade-off with the heuristic approach. Mitigation: make `needs_input_timeout` configurable and default to a reasonable value (5-10 seconds).

2. **Cross-platform SIGTERM**: `portable-pty` should handle process termination, but we need to verify the `Child::kill()` method works correctly on both macOS and Linux.

3. **Terminal scrollback separator**: When restarting an agent, we want to insert a visual separator in the scrollback. This may require extending `TerminalBuffer` with a method to inject styled content into the scrollback, or simply writing escape sequences to the PTY before respawning.

4. **AgentTerminal tab rendering**: The `TabBuffer::AgentTerminal` variant introduces indirection — the tab doesn't own its buffer. Need to ensure the rendering path handles this correctly, possibly by passing `Option<&dyn BufferView>` or similar.

5. **State machine timing source**: For testability, the state machine uses `Instant` parameters rather than calling `Instant::now()` internally. This is correct for unit tests but means callers must pass accurate timestamps.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
