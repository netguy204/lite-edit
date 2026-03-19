---
decision: APPROVE
summary: "All success criteria satisfied — skill file exists with complete monitoring procedure, registered in CLAUDE.md, covers all status transitions, and documents all required commands"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: A `.claude/commands/orchestrator-monitor.md` skill file exists with the full monitoring procedure

- **Status**: satisfied
- **Evidence**: File exists at `.claude/commands/orchestrator-monitor.md` (207 lines) with proper YAML frontmatter (`description: Monitor an orchestrator chunk through to completion with periodic polling.`) and comprehensive multi-section procedure covering setup, polling, resolution, completion, and failure flows.

### Criterion 2: The skill is registered in CLAUDE.md's available commands section

- **Status**: satisfied
- **Evidence**: CLAUDE.md line 90: `Commands: /orchestrator-submit-future, /orchestrator-investigate, /orchestrator-monitor` — added to the existing Orchestrator section.

### Criterion 3: The procedure covers: initial `/loop` setup, per-tick status checks, NEEDS_ATTENTION resolution (with `ve orch work-unit show`, branch inspection, merge or reset), DONE handling (changelog post + loop cancel), and FAILED handling

- **Status**: satisfied
- **Evidence**:
  - Initial Setup section (lines 22-53): validates chunk, checks terminal states, starts `/loop 3m /orchestrator-monitor $ARGUMENTS`
  - Per-Tick Status Check section (lines 58-74): status table with RUNNING, READY/QUEUED, NEEDS_ATTENTION, DONE, FAILED
  - NEEDS_ATTENTION Resolution section (lines 77-152): covers ASK_OPERATOR (with `ve orch answer`), MERGE_CONFLICT (branch inspection via `git log`, merge or reset via `ve orch work-unit status`), AGENT_FAILED (log inspection, retry), and Other/Unknown (delegates to `/orchestrator-investigate`)
  - Completion Handling section (lines 155-178): confirms via `ve orch work-unit show`, checks merge with `git log`, posts summary, cancels loop
  - Failure Handling section (lines 181-207): gets details, checks logs, posts summary with next steps, cancels loop

### Criterion 4: Key commands are documented: `ve orch ps`, `ve orch work-unit show`, `ve orch work-unit status <chunk> DONE`, `ve orch work-unit status <chunk> READY`

- **Status**: satisfied
- **Evidence**:
  - `ve orch ps`: Initial Setup section (line 26) for chunk discovery when no argument provided
  - `ve orch work-unit show`: Used throughout — validation (line 30), NEEDS_ATTENTION (line 84), DONE (line 161), FAILED (line 187)
  - `ve orch work-unit status $ARGUMENTS DONE`: MERGE_CONFLICT resolution (line 119)
  - `ve orch work-unit status $ARGUMENTS READY`: MERGE_CONFLICT reset (line 123)
