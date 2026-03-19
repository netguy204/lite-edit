---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- .claude/commands/orchestrator-monitor.md
- CLAUDE.md
code_references:
  - ref: .claude/commands/orchestrator-monitor.md
    implements: "Full orchestrator monitoring skill with polling loop, status checks, NEEDS_ATTENTION resolution, DONE/FAILED handling"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- fuzzy_finder_hidden_files
---

# Chunk Goal

## Minor Goal

Create a `/orchestrator-monitor` slash command that automates monitoring of
orchestrator chunks through to completion. When a steward injects a chunk, this
skill sets up a `/loop 3m` polling cycle that:

- Checks `ve orch ps` for the chunk's phase and status
- Takes no action while RUNNING
- Runs `/orchestrator-investigate` on NEEDS_ATTENTION, with guidance for common
  causes (review escalation, merge conflicts, agent failure)
- Posts a completion summary to the changelog on DONE and cancels the loop
- Posts a failure summary on FAILED

The skill should also include resolution procedures for NEEDS_ATTENTION states:
inspecting the attention reason via `ve orch work-unit show`, checking the
branch for unmerged commits, merging if implementation is sufficient, or
resetting to READY for retry.

This pairs with `/steward-watch` — the steward monitors orchestrator chunks
concurrently with watching the inbound channel.

## Success Criteria

- A `.claude/commands/orchestrator-monitor.md` skill file exists with the
  full monitoring procedure
- The skill is registered in CLAUDE.md's available commands section
- The procedure covers: initial `/loop` setup, per-tick status checks,
  NEEDS_ATTENTION resolution (with `ve orch work-unit show`, branch inspection,
  merge or reset), DONE handling (changelog post + loop cancel), and FAILED
  handling
- Key commands are documented: `ve orch ps`, `ve orch work-unit show`,
  `ve orch work-unit status <chunk> DONE`, `ve orch work-unit status <chunk> READY`