---
description: Monitor an orchestrator chunk through to completion with periodic polling.
---


## Tips

- The ve command is an installed CLI tool, not a file in the repository. Do not
  search for it - run it directly via Bash.


## Instructions

This command monitors an orchestrator work unit through to completion by setting
up a periodic polling loop that checks status and takes action based on state
transitions.

**Chunk to monitor:** `$ARGUMENTS`

---

## Initial Setup

If no chunk name is provided in `$ARGUMENTS`, run `ve orch ps` and ask the
operator which chunk to monitor.

### Validate the chunk

```bash
ve orch work-unit show $ARGUMENTS
```

If the chunk does not exist, report the error and stop.

### Check for terminal states

If the chunk is already **DONE**, report completion and stop (no loop needed).
If the chunk is already **FAILED**, report failure details and stop (no loop needed).

### Start monitoring loop

If the chunk is in a non-terminal state, start the polling loop:

```
/loop 3m /orchestrator-monitor $ARGUMENTS
```

Confirm to the operator that monitoring is active:

```
👀 Monitoring `<chunk>` — polling every 3 minutes. I'll report status changes
and handle NEEDS_ATTENTION, DONE, and FAILED states automatically.
```

---

## Per-Tick Status Check

On each loop iteration, check the work unit status:

```bash
ve orch work-unit show $ARGUMENTS --json
```

Parse the `status` field and take action based on the current state:

| Status | Action |
|--------|--------|
| `RUNNING` | No action. Report briefly: "⏳ `<chunk>` still RUNNING (phase: `<phase>`)" |
| `READY` / `QUEUED` | No action. Report briefly: "⏳ `<chunk>` waiting to be scheduled" |
| `NEEDS_ATTENTION` | Escalate — see NEEDS_ATTENTION Resolution section below |
| `DONE` | Celebrate — see Completion Handling section below |
| `FAILED` | Report — see Failure Handling section below |

---

## NEEDS_ATTENTION Resolution

When status is NEEDS_ATTENTION:

### 1. Get attention details

```bash
ve orch work-unit show $ARGUMENTS
```

Note the `attention_reason` field.

### 2. Resolve based on attention reason

#### ASK_OPERATOR — The agent has a question

The agent is blocked waiting for operator input.

1. Show the question to the operator
2. Wait for the operator's answer
3. Submit the answer:
   ```bash
   ve orch answer $ARGUMENTS "<answer>"
   ```
4. The work unit will automatically resume

#### MERGE_CONFLICT — Post-implementation merge failed

The agent completed its work but the merge back to main failed.

1. Check if the branch has useful commits:
   ```bash
   git log --oneline orch/$ARGUMENTS..HEAD 2>/dev/null || git log --oneline orch/$ARGUMENTS -5
   ```

2. If implementation looks complete, attempt merge resolution:
   ```bash
   git merge orch/$ARGUMENTS --no-edit
   # Resolve any conflicts in the affected files
   # git add <resolved-files>
   # git commit --no-edit
   git branch -d orch/$ARGUMENTS
   ve orch work-unit status $ARGUMENTS DONE
   ```

3. If implementation is incomplete or the branch looks wrong, reset for retry:
   ```bash
   ve orch work-unit status $ARGUMENTS READY
   ```

#### AGENT_FAILED — The agent process crashed or errored

1. Check the tail of the phase log:
   ```bash
   tail -c 10000 .ve/chunks/$ARGUMENTS/log/*.txt | tail -100
   ```

2. If transient failure (timeout, resource issue), retry:
   ```bash
   ve orch work-unit retry $ARGUMENTS
   ```

3. If persistent failure, escalate to operator with a summary of the log output

#### Other / Unknown

Run the full diagnostic workflow:

```
/orchestrator-investigate $ARGUMENTS
```

### 3. Continue monitoring

After resolution, the next loop tick will pick up the new status automatically.

---

## Completion Handling (DONE)

When status is DONE:

1. Confirm completion:
   ```bash
   ve orch work-unit show $ARGUMENTS
   ```

2. Check that the branch was merged:
   ```bash
   git log --oneline -5
   ```
   Look for the chunk's commits on main.

3. Post a completion summary to the operator:
   ```
   ✅ Chunk `<chunk>` completed successfully.
   - Phase: DONE
   - Branch: merged to main
   ```

4. Cancel the monitoring loop (stop the `/loop`)

---

## Failure Handling (FAILED)

When status is FAILED:

1. Get failure details:
   ```bash
   ve orch work-unit show $ARGUMENTS
   ```

2. Check phase logs for error context:
   ```bash
   tail -c 10000 .ve/chunks/$ARGUMENTS/log/*.txt | tail -100
   ```

3. Post a failure summary to the operator:
   ```
   ❌ Chunk `<chunk>` FAILED.
   - Phase: <phase where it failed>
   - Reason: <summary from logs>
   ```

4. Suggest next steps:
   - **Retry:** `ve orch work-unit retry $ARGUMENTS`
   - **Investigate:** `/orchestrator-investigate $ARGUMENTS`
   - **Delete:** `ve orch work-unit delete $ARGUMENTS`

5. Cancel the monitoring loop (stop the `/loop`)
