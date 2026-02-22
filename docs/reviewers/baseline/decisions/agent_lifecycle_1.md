---
decision: FEEDBACK
summary: "All criteria satisfied except stop() uses SIGKILL directly instead of SIGTERM then SIGKILL as specified"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `AgentHandle` correctly infers all state transitions: Starting → Running → NeedsInput → Stale → Exited/Errored

- **Status**: satisfied
- **Evidence**: `AgentStateMachine` in `crates/terminal/src/agent.rs` implements all transitions. Unit tests (lines 549-720) verify: Starting→Running, Running→NeedsInput, NeedsInput→Running, NeedsInput→Stale, *→Exited. Integration tests in `agent_integration.rs` verify end-to-end behavior with real processes.

### Criterion 2: PTY idle detection works: when a shell is waiting for input, state transitions to NeedsInput after the configured timeout

- **Status**: satisfied
- **Evidence**: `AgentStateMachine::tick()` (line 249-278) checks `now.duration_since(last_output)` against `config.needs_input_timeout`. Test `test_running_to_needs_input_after_timeout` and integration test `test_agent_needs_input_after_idle` verify this with a `cat` process.

### Criterion 3: When new output appears after NeedsInput, state transitions back to Running

- **Status**: satisfied
- **Evidence**: `on_output()` method (line 227-242) transitions NeedsInput→Running on new output. Test `test_needs_input_to_running_on_output` verifies this behavior.

### Criterion 4: Process exit is detected and state transitions to Exited with correct exit code

- **Status**: satisfied
- **Evidence**: `AgentHandle::poll()` calls `terminal.try_wait()` and `state_machine.on_exit(exit_code, now)` (lines 357-359). Tests `test_agent_exited_with_code_0` and `test_agent_exited_with_nonzero_code` verify correct exit code handling.

### Criterion 5: Agent state drives `WorkspaceStatus` — left rail indicator updates in real time

- **Status**: satisfied
- **Evidence**: `Workspace::compute_status()` in `workspace.rs` (lines 423-434) maps AgentState to WorkspaceStatus. `poll_agent()` calls `compute_status()` to update status each frame.

### Criterion 6: 🟢 while Running

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Starting | AgentState::Running` to `WorkspaceStatus::Running` (line 427)

### Criterion 7: 🟡 (pulsing) when NeedsInput

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::NeedsInput` to `WorkspaceStatus::NeedsInput` (line 428)

### Criterion 8: 🟠 when Stale

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Stale` to `WorkspaceStatus::Stale` (line 429)

### Criterion 9: ✅ when Exited(0)

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Exited { code: 0 }` to `WorkspaceStatus::Completed` (line 430)

### Criterion 10: 🔴 when Exited(non-zero)

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Exited { .. }` (non-zero catch-all) to `WorkspaceStatus::Errored` (line 431)

### Criterion 11: Launching an agent in a workspace creates an `AgentHandle` and pins the agent terminal as the first tab

- **Status**: satisfied
- **Evidence**: `Workspace::launch_agent()` (lines 462-500) calls `AgentHandle::spawn()`, creates `Tab::new_agent()`, inserts at index 0, and sets `active_tab = 0`.

### Criterion 12: Restarting a completed/errored agent re-spawns the process in the same workspace

- **Status**: satisfied
- **Evidence**: `AgentHandle::restart()` (lines 410-439) creates new terminal, spawns process with same config, resets state machine. `Workspace::restart_agent()` delegates to agent handle.

### Criterion 13: Stopping a running agent sends SIGTERM and transitions to Exited

- **Status**: gap
- **Evidence**: `AgentHandle::stop()` (lines 449-464) calls `terminal.kill()` which is SIGKILL (see `pty.rs` line 172 comment: "This sends SIGKILL"). The GOAL.md and PLAN.md specify: "Send SIGTERM, wait briefly, SIGKILL if needed." The implementation skips SIGTERM and goes directly to SIGKILL. Exit code is -9 (SIGKILL) not -15 (SIGTERM).

### Criterion 14: False positive rate for NeedsInput is low: an agent doing slow computation (no output for 10+ seconds) is an acceptable false positive, but it should be rare with a well-tuned timeout

- **Status**: satisfied
- **Evidence**: `needs_input_timeout` is configurable via `AgentConfig` (default 5 seconds per GOAL.md). The investigation noted this as an acceptable trade-off with the heuristic approach.

### Criterion 15: The agent lifecycle is fully agent-agnostic — works with Claude Code, aider, or a plain shell

- **Status**: satisfied
- **Evidence**: State inference is based solely on PTY behavior (output timing, process exit). Integration tests use generic commands (`echo`, `cat`, `true`, `false`). No agent-specific patterns in the implementation.

## Feedback Items

### Issue 1: Stop uses SIGKILL instead of SIGTERM

- **id**: issue-sigterm
- **location**: `crates/terminal/src/agent.rs:449-464`
- **concern**: The `stop()` method sends SIGKILL directly instead of following the SIGTERM→wait→SIGKILL pattern specified in GOAL.md and PLAN.md. This prevents agents (like Claude Code) from performing graceful cleanup.
- **suggestion**: Implement the specified behavior:
  1. Send SIGTERM via `nix::sys::signal::kill(pid, Signal::SIGTERM)` (requires `nix` crate)
  2. Wait briefly (e.g., 100ms) with polling for exit
  3. If still running, send SIGKILL
  4. Transition to `Exited { code: -15 }` if SIGTERM worked, or `-9` if SIGKILL was needed
- **severity**: functional
- **confidence**: high
