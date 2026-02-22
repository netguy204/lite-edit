---
decision: APPROVE
summary: "All 15 success criteria satisfied after implementing SIGTERM→wait→SIGKILL pattern per operator guidance"
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: `AgentHandle` correctly infers all state transitions: Starting → Running → NeedsInput → Stale → Exited/Errored

- **Status**: satisfied
- **Evidence**: `AgentStateMachine` in `crates/terminal/src/agent.rs` implements all transitions (lines 195-293). Comprehensive unit tests verify each transition. Integration tests in `agent_integration.rs` verify end-to-end behavior with real processes (14 tests, all passing).

### Criterion 2: PTY idle detection works: when a shell is waiting for input, state transitions to NeedsInput after the configured timeout

- **Status**: satisfied
- **Evidence**: `AgentStateMachine::tick()` (lines 249-278) checks `now.duration_since(last_output)` against `config.needs_input_timeout`. Test `test_agent_needs_input_after_idle` verifies this with a `cat` process.

### Criterion 3: When new output appears after NeedsInput, state transitions back to Running

- **Status**: satisfied
- **Evidence**: `on_output()` method (lines 227-242) transitions NeedsInput→Running on new output. Unit test `test_needs_input_to_running_on_output` verifies this behavior.

### Criterion 4: Process exit is detected and state transitions to Exited with correct exit code

- **Status**: satisfied
- **Evidence**: `AgentHandle::poll()` (lines 345-364) calls `terminal.try_wait()` and `state_machine.on_exit(exit_code, now)`. Tests `test_agent_exited_with_code_0` and `test_agent_exited_with_nonzero_code` pass.

### Criterion 5: Agent state drives `WorkspaceStatus` — left rail indicator updates in real time:

- **Status**: satisfied
- **Evidence**: `Workspace::compute_status()` in `workspace.rs` maps AgentState to WorkspaceStatus. `poll_agent()` updates status each frame.

### Criterion 6: 🟢 while Running

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Starting | AgentState::Running` to `WorkspaceStatus::Running`

### Criterion 7: 🟡 (pulsing) when NeedsInput

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::NeedsInput` to `WorkspaceStatus::NeedsInput`

### Criterion 8: 🟠 when Stale

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Stale` to `WorkspaceStatus::Stale`

### Criterion 9: ✅ when Exited(0)

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Exited { code: 0 }` to `WorkspaceStatus::Completed`

### Criterion 10: 🔴 when Exited(non-zero)

- **Status**: satisfied
- **Evidence**: `compute_status()` maps `AgentState::Exited { .. }` (non-zero) to `WorkspaceStatus::Errored`

### Criterion 11: Launching an agent in a workspace creates an `AgentHandle` and pins the agent terminal as the first tab

- **Status**: satisfied
- **Evidence**: `Workspace::launch_agent()` (lines 462-500) calls `AgentHandle::spawn()`, creates `Tab::new_agent()`, inserts at index 0, sets `active_tab = 0`.

### Criterion 12: Restarting a completed/errored agent re-spawns the process in the same workspace

- **Status**: satisfied
- **Evidence**: `AgentHandle::restart()` (lines 410-439) creates new terminal, spawns process with same config, resets state machine. Test `test_agent_restart_from_exited` passes.

### Criterion 13: Stopping a running agent sends SIGTERM and transitions to Exited

- **Status**: satisfied
- **Evidence**: `AgentHandle::stop()` (lines 442-502) now implements SIGTERM→wait→SIGKILL pattern per operator guidance:
  1. Gets process ID via `terminal.process_id()` (exposed via new `PtyHandle::process_id()` method)
  2. Sends SIGTERM via `libc::kill(pid, libc::SIGTERM)`
  3. Waits ~100ms (10 iterations × 10ms) polling `try_wait()`
  4. If still alive, falls back to SIGKILL via `terminal.kill()`
  5. Exit code reflects the signal: -15 if SIGTERM succeeded, -9 if SIGKILL was required

  Added `libc` as direct dependency in `Cargo.toml` per operator guidance.

### Criterion 14: False positive rate for NeedsInput is low: an agent doing slow computation (no output for 10+ seconds) is an acceptable false positive, but it should be rare with a well-tuned timeout

- **Status**: satisfied
- **Evidence**: `needs_input_timeout` is configurable via `AgentConfig` (default 5 seconds). Investigation documented this as an acceptable trade-off.

### Criterion 15: The agent lifecycle is fully agent-agnostic — works with Claude Code, aider, or a plain shell

- **Status**: satisfied
- **Evidence**: State inference is based solely on PTY behavior (output timing, process exit). Integration tests use generic commands (`echo`, `cat`, `true`, `false`). No agent-specific patterns.
