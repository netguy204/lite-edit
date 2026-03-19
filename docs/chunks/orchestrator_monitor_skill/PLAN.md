

<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Create a new `.claude/commands/orchestrator-monitor.md` skill file that defines
the `/orchestrator-monitor` slash command. This is a documentation-only chunk —
the deliverable is a markdown procedure file, not Rust code.

The skill follows the same structure as the existing
`.claude/commands/orchestrator-investigate.md` skill: YAML frontmatter with a
description, tips section, multi-phase instructions that guide the agent through
the monitoring workflow. The key difference is that this skill sets up a
recurring `/loop 3m` poll rather than a one-shot investigation.

The procedure will:
1. Accept a chunk name as `$ARGUMENTS`
2. Validate the chunk exists in the orchestrator via `ve orch work-unit show`
3. Set up `/loop 3m` to poll `ve orch ps --status` for the chunk
4. On each tick: check status, take action based on state (RUNNING → no-op,
   NEEDS_ATTENTION → investigate, DONE → summarize + cancel loop,
   FAILED → summarize)

The skill also needs to be registered in CLAUDE.md's Available Commands and
Orchestrator sections.

No automated tests apply here — this is a procedural skill file. Verification
is that the file exists, follows the established command file conventions, and
covers all status transitions described in the goal.

## Subsystem Considerations

No subsystems are relevant. This chunk creates a standalone skill file.

## Sequence

### Step 1: Create the skill file `.claude/commands/orchestrator-monitor.md`

Create the file with YAML frontmatter and the full monitoring procedure. The
file structure should mirror `orchestrator-investigate.md`:

**Frontmatter:**
```yaml
---
description: Monitor an orchestrator chunk through to completion with periodic polling.
---
```

**Tips section:** Same boilerplate as other commands (ve is a CLI tool, run directly).

**Instructions body — organized into these sections:**

#### Section: Initial Setup

- Accept `$ARGUMENTS` as the chunk name to monitor
- If no argument provided, run `ve orch ps` and ask the operator which chunk
- Validate the chunk exists: `ve orch work-unit show $ARGUMENTS`
- If the chunk is already DONE or FAILED, report status and stop (no loop needed)
- Start the polling loop: `/loop 3m /orchestrator-monitor $ARGUMENTS`
- Confirm to the operator that monitoring is active

#### Section: Per-Tick Status Check

On each loop iteration:

```bash
ve orch work-unit show $ARGUMENTS --json
```

Parse the status field and branch:

| Status | Action |
|--------|--------|
| `RUNNING` | No action. Report briefly: "⏳ `<chunk>` still RUNNING (phase: `<phase>`)" |
| `READY` / `QUEUED` | No action. Report briefly: "⏳ `<chunk>` waiting to be scheduled" |
| `NEEDS_ATTENTION` | Escalate — see NEEDS_ATTENTION Resolution section |
| `DONE` | Celebrate — see Completion Handling section |
| `FAILED` | Report — see Failure Handling section |

#### Section: NEEDS_ATTENTION Resolution

When status is NEEDS_ATTENTION:

1. Run `ve orch work-unit show $ARGUMENTS` to get the `attention_reason`
2. Based on attention_reason, provide guidance:

   **ASK_OPERATOR** — The agent has a question:
   - Show the question to the operator
   - Use `ve orch answer $ARGUMENTS "<answer>"` to respond
   - The work unit will automatically resume

   **MERGE_CONFLICT** — Post-implementation merge failed:
   - Check if the branch has useful commits: `git log --oneline orch/$ARGUMENTS..HEAD 2>/dev/null || git log --oneline orch/$ARGUMENTS -5`
   - If implementation looks complete, attempt merge resolution:
     ```bash
     git merge orch/$ARGUMENTS --no-edit
     # resolve conflicts
     git branch -d orch/$ARGUMENTS
     ve orch work-unit status $ARGUMENTS DONE
     ```
   - If implementation is incomplete, reset for retry:
     `ve orch work-unit status $ARGUMENTS READY`

   **AGENT_FAILED** — The agent process crashed or errored:
   - Check the tail of the phase log: `tail -c 10000 .ve/chunks/$ARGUMENTS/log/*.txt | tail -100`
   - If transient failure (timeout, resource issue), retry: `ve orch work-unit retry $ARGUMENTS`
   - If persistent failure, escalate to operator with log summary

   **Other / Unknown** — Run `/orchestrator-investigate $ARGUMENTS` for full
   diagnostic workflow.

3. After resolution, the next loop tick will pick up the new status.

#### Section: Completion Handling (DONE)

When status is DONE:

1. Run `ve orch work-unit show $ARGUMENTS` to confirm
2. Check that the branch was merged: `git log --oneline -5` (look for the chunk's commits)
3. Post a completion summary to the operator:
   ```
   ✅ Chunk `<chunk>` completed successfully.
   - Phase: DONE
   - Branch: merged to main
   ```
4. Cancel the monitoring loop (stop the `/loop`)

#### Section: Failure Handling (FAILED)

When status is FAILED:

1. Run `ve orch work-unit show $ARGUMENTS` for details
2. Check phase logs: `tail -c 10000 .ve/chunks/$ARGUMENTS/log/*.txt | tail -100`
3. Post a failure summary:
   ```
   ❌ Chunk `<chunk>` FAILED.
   - Phase: <phase where it failed>
   - Reason: <summary from logs>
   ```
4. Suggest next steps: retry (`ve orch work-unit retry $ARGUMENTS`), investigate
   (`/orchestrator-investigate $ARGUMENTS`), or delete (`ve orch work-unit delete $ARGUMENTS`)
5. Cancel the monitoring loop

Location: `.claude/commands/orchestrator-monitor.md`

### Step 2: Register the skill in CLAUDE.md

Add `/orchestrator-monitor` to the Orchestrator section of `CLAUDE.md` where
the existing orchestrator commands are listed (line ~90):

```
Commands: `/orchestrator-submit-future`, `/orchestrator-investigate`, `/orchestrator-monitor`
```

Location: `CLAUDE.md`

### Step 3: Update code_paths in GOAL.md frontmatter

Set `code_paths` to:
```yaml
code_paths:
  - .claude/commands/orchestrator-monitor.md
  - CLAUDE.md
```

Location: `docs/chunks/orchestrator_monitor_skill/GOAL.md`

### Step 4: Verify

- Confirm the file exists and has valid YAML frontmatter
- Confirm all status transitions from the goal's success criteria are covered:
  - ✅ Initial `/loop` setup
  - ✅ Per-tick status checks via `ve orch ps` / `ve orch work-unit show`
  - ✅ NEEDS_ATTENTION resolution with `ve orch work-unit show`, branch inspection, merge or reset
  - ✅ DONE handling with summary + loop cancel
  - ✅ FAILED handling
- Confirm key commands documented: `ve orch ps`, `ve orch work-unit show`,
  `ve orch work-unit status <chunk> DONE`, `ve orch work-unit status <chunk> READY`

## Dependencies

No dependencies. The `/loop` skill and `/orchestrator-investigate` command
already exist. This chunk creates a new file that references them.

## Risks and Open Questions

- **`/loop` cancellation mechanism**: The plan assumes `/loop` can be cancelled
  from within the looped command. If `/loop` doesn't support self-cancellation,
  the DONE/FAILED handlers will need to instruct the operator to cancel manually.
  Deviation will be noted if discovered during implementation.
- **`ve orch work-unit show --json` output schema**: The exact JSON field names
  for status, phase, and attention_reason need to be verified at implementation
  time. The plan uses names observed in `orchestrator-investigate.md`.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->