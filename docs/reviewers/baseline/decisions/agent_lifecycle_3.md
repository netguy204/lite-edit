---
decision: ESCALATE
summary: "Same issue (SIGTERM vs SIGKILL) unaddressed after 2 iterations - requires operator decision"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `AgentHandle` correctly infers all state transitions: Starting → Running → NeedsInput → Stale → Exited/Errored

- **Status**: satisfied
- **Evidence**: `AgentStateMachine` in `crates/terminal/src/agent.rs` implements all transitions (lines 195-293). Comprehensive unit tests (lines 549-720) verify each transition: Starting→Running, Running→NeedsInput, NeedsInput→Running, NeedsInput→Stale, Stale→Running, *→Exited. Integration tests in `agent_integration.rs` verify end-to-end behavior with real processes (14 tests, all passing).

### Criterion 2: PTY idle detection works: when a shell is waiting for input, state transitions to NeedsInput after the configured timeout

- **Status**: satisfied
- **Evidence**: `AgentStateMachine::tick()` (lines 249-278) checks `now.duration_since(last_output)` against `config.needs_input_timeout`. Unit test `test_running_to_needs_input_after_timeout` and integration test `test_agent_needs_input_after_idle` verify this behavior with a `cat` process.

### Criterion 3: When new output appears after NeedsInput, state transitions back to Running

- **Status**: satisfied
- **Evidence**: `on_output()` method (lines 227-242) transitions NeedsInput→Running on new output. Unit test `test_needs_input_to_running_on_output` verifies this behavior.

### Criterion 4: Process exit is detected and state transitions to Exited with correct exit code

- **Status**: satisfied
- **Evidence**: `AgentHandle::poll()` (lines 345-364) calls `terminal.try_wait()` and `state_machine.on_exit(exit_code, now)`. Integration tests `test_agent_exited_with_code_0` and `test_agent_exited_with_nonzero_code` verify correct exit code handling (both pass).

### Criterion 5: Agent state drives `WorkspaceStatus` — left rail indicator updates in real time:

- **Status**: satisfied
- **Evidence**: `Workspace::compute_status()` in `workspace.rs` (lines 423-434) maps AgentState to WorkspaceStatus. `poll_agent()` (lines 537-548) calls `compute_status()` to update status each frame. `poll_agents()` in `editor_state.rs` (lines 1082-1091) polls all workspaces.

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
- **Evidence**: `Workspace::launch_agent()` (lines 462-500) calls `AgentHandle::spawn()`, creates `Tab::new_agent()`, inserts at index 0 (pinned position), and sets `active_tab = 0`.

### Criterion 12: Restarting a completed/errored agent re-spawns the process in the same workspace

- **Status**: satisfied
- **Evidence**: `AgentHandle::restart()` (lines 410-439) creates new terminal, spawns process with same config, resets state machine. Integration test `test_agent_restart_from_exited` passes. `Workspace::restart_agent()` delegates to agent handle.

### Criterion 13: Stopping a running agent sends SIGTERM and transitions to Exited

- **Status**: gap
- **Evidence**: `AgentHandle::stop()` (lines 449-464) calls `terminal.kill()` which sends SIGKILL directly (see `pty.rs` lines 178-183 and comments: "This sends SIGKILL"). The GOAL.md criterion explicitly states "sends SIGTERM" and PLAN.md step 4 specifies: "Send SIGTERM, wait briefly, SIGKILL if needed." Implementation uses `-9` (SIGKILL exit code) not `-15` (SIGTERM).

### Criterion 14: False positive rate for NeedsInput is low: an agent doing slow computation (no output for 10+ seconds) is an acceptable false positive, but it should be rare with a well-tuned timeout

- **Status**: satisfied
- **Evidence**: `needs_input_timeout` is configurable via `AgentConfig` (default 5 seconds). The investigation documented this as an acceptable trade-off with the heuristic approach (section "Unresolved questions" point 1).

### Criterion 15: The agent lifecycle is fully agent-agnostic — works with Claude Code, aider, or a plain shell

- **Status**: satisfied
- **Evidence**: State inference is based solely on PTY behavior (output timing, process exit). Integration tests use generic commands (`echo`, `cat`, `true`, `false`). No agent-specific patterns in the implementation.

## Escalation Reason

**reason**: RECURRING_ISSUE

The SIGTERM vs SIGKILL issue has been flagged in iterations 1 and 2 (files `agent_lifecycle_1.md` and `agent_lifecycle_2.md`) but remains unaddressed. Per reviewer guidelines, recurring issues that haven't converged should escalate.

**context**:
  questions:
    - "Should the implementation be updated to use SIGTERM→wait→SIGKILL as specified in GOAL.md and PLAN.md, or should the documented behavior be revised to accept SIGKILL-only as the implementation choice?"
    - "If SIGTERM support is required, is adding the `nix` crate dependency acceptable? This is the standard approach for POSIX signal handling in Rust."
    - "Alternatively, is this a sufficient 'graceful enough' stop for the MVP scope, with SIGTERM support deferred to a follow-up chunk?"

**rationale**: 14 of 15 success criteria are satisfied and all agent integration tests pass. The implementation is functionally complete and agent-agnostic. The only gap is a specification compliance issue (SIGTERM vs SIGKILL) that the implementer has not addressed after two review cycles. This suggests either:
1. A technical blocker (e.g., `portable-pty` doesn't expose process PID for signal sending)
2. A scope/priority decision that needs operator input
3. The implementer doesn't have context from the previous review iterations

The operator should decide whether to:
- Accept the current implementation as-is and update the docs
- Require the SIGTERM implementation before approval
- Create a follow-up chunk for graceful termination
