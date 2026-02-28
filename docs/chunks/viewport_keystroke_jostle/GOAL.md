---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: null
investigation: null
subsystems:
  - subsystem_id: "viewport_scroll"
    relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after: ["terminal_unicode_env", "incremental_parse", "tab_rendering"]
---

<!--
╔══════════════════════════════════════════════════════════════════════════════╗
║  DO NOT DELETE THIS COMMENT BLOCK until the chunk complete command is run.   ║
║                                                                              ║
║  AGENT INSTRUCTIONS: When editing this file, preserve this entire comment    ║
║  block. Only modify the frontmatter YAML and the content sections below      ║
║  (Minor Goal, Success Criteria, Relationship to Parent). Use targeted edits  ║
║  that replace specific sections rather than rewriting the entire file.       ║
╚══════════════════════════════════════════════════════════════════════════════╝

This comment describes schema information that needs to be adhered
to throughout the process.

STATUS VALUES:
- FUTURE: This chunk is queued for future work and not yet being implemented
- IMPLEMENTING: This chunk is in the process of being implemented.
- ACTIVE: This chunk accurately describes current or recently-merged work
- SUPERSEDED: Another chunk has modified the code this chunk governed
- HISTORICAL: Significant drift; kept for archaeology only

FUTURE CHUNK APPROVAL REQUIREMENT:
ALL FUTURE chunks require operator approval before committing or injecting.
After refining this GOAL.md, you MUST present it to the operator and wait for
explicit approval. Do NOT commit or inject until the operator approves.
This applies whether triggered by "in the background", "create a future chunk",
or any other mechanism that creates a FUTURE chunk.

COMMIT BOTH FILES: When committing a FUTURE chunk after approval, add the entire
chunk directory (both GOAL.md and PLAN.md) to the commit, not just GOAL.md. The
`ve chunk create` command creates both files, and leaving PLAN.md untracked will
cause merge conflicts when the orchestrator creates a worktree for the PLAN phase.

PARENT_CHUNK:
- null for new work
- chunk directory name (e.g., "006-segment-compaction") for corrections or modifications

CODE_PATHS:
- Populated at planning time
- List files you expect to create or modify
- Example: ["src/segment/writer.rs", "src/segment/format.rs"]

CODE_REFERENCES:
- Populated after implementation, before PR
- Uses symbolic references to identify code locations

- Format: {file_path}#{symbol_path} where symbol_path uses :: as nesting separator
- Example:
  code_references:
    - ref: src/segment/writer.rs#SegmentWriter
      implements: "Core write loop and buffer management"
    - ref: src/segment/writer.rs#SegmentWriter::fsync
      implements: "Durability guarantees"
    - ref: src/utils.py#validate_input
      implements: "Input validation logic"


NARRATIVE:
- If this chunk was derived from a narrative document, reference the narrative directory name.
- When setting this field during /chunk-create, also update the narrative's OVERVIEW.md
  frontmatter to add this chunk to its `chunks` array with the prompt and chunk_directory.
- If this is the final chunk of a narrative, the narrative status should be set to COMPLETED
  when this chunk is completed.

INVESTIGATION:
- If this chunk was derived from an investigation's proposed_chunks, reference the investigation
  directory name (e.g., "memory_leak" for docs/investigations/memory_leak/).
- This provides traceability from implementation work back to exploratory findings.
- When implementing, read the referenced investigation's OVERVIEW.md for context on findings,
  hypotheses tested, and decisions made during exploration.
- Validated by `ve chunk validate` to ensure referenced investigations exist.


SUBSYSTEMS:
- Optional list of subsystem references that this chunk relates to
- Format: subsystem_id is the subsystem directory name, relationship is "implements" or "uses"
- "implements": This chunk directly implements part of the subsystem's functionality
- "uses": This chunk depends on or uses the subsystem's functionality
- Example:
  subsystems:
    - subsystem_id: "validation"
      relationship: implements
    - subsystem_id: "frontmatter"
      relationship: uses
- Validated by `ve chunk validate` to ensure referenced subsystems exist
- When a chunk that implements a subsystem is completed, a reference should be added to
  that chunk in the subsystems OVERVIEW.md file front matter and relevant section.

FRICTION_ENTRIES:
- Optional list of friction entries that this chunk addresses
- Provides "why did we do this work?" traceability from implementation back to accumulated pain points
- Format: entry_id is the friction entry ID (e.g., "F001"), scope is "full" or "partial"
  - "full": This chunk fully resolves the friction entry
  - "partial": This chunk partially addresses the friction entry
- When to populate: During /chunk-create if this chunk addresses known friction from FRICTION.md
- Example:
  friction_entries:
    - entry_id: F001
      scope: full
    - entry_id: F003
      scope: partial
- Validated by `ve chunk validate` to ensure referenced friction entries exist in FRICTION.md
- When a chunk addresses friction entries and is completed, those entries are considered RESOLVED

BUG_TYPE:
- Optional field for bug fix chunks that guides agent behavior at completion
- Values: semantic | implementation | null (for non-bug chunks)
  - "semantic": The bug revealed new understanding of intended behavior
    - Code backreferences REQUIRED (the fix adds to code understanding)
    - On completion, search for other chunks that may need updating
    - Status → ACTIVE (the chunk asserts ongoing understanding)
  - "implementation": The bug corrected known-wrong code
    - Code backreferences MAY BE SKIPPED (they don't add semantic value)
    - Focus purely on the fix
    - Status → HISTORICAL (point-in-time correction, not an ongoing anchor)
- Leave null for feature chunks and other non-bug work

CHUNK ARTIFACTS:
- Single-use scripts, migration tools, or one-time utilities created for this chunk
  should be stored in the chunk directory (e.g., docs/chunks/foo/migrate.py)
- These artifacts help future archaeologists understand what the chunk did
- Unlike code in src/, chunk artifacts are not expected to be maintained long-term
- Examples: data migration scripts, one-time fixups, analysis tools used during implementation

CREATED_AFTER:
- Auto-populated by `ve chunk create` - DO NOT MODIFY manually
- Lists the "tips" of the chunk DAG at creation time (chunks with no dependents yet)
- Tips must be ACTIVE chunks (shipped work that has been merged)
- Example: created_after: ["auth_refactor", "api_cleanup"]

IMPORTANT - created_after is NOT implementation dependencies:
- created_after tracks CAUSAL ORDERING (what work existed when this chunk was created)
- It does NOT mean "chunks that must be implemented before this one can work"
- FUTURE chunks can NEVER be tips (they haven't shipped yet)

COMMON MISTAKE: Setting created_after to reference FUTURE chunks because they
represent design dependencies. This is WRONG. If chunk B conceptually depends on
chunk A's implementation, but A is still FUTURE, B's created_after should still
reference the current ACTIVE tips, not A.

WHERE TO TRACK IMPLEMENTATION DEPENDENCIES:
- Investigation proposed_chunks ordering (earlier = implement first)
- Narrative chunk sequencing in OVERVIEW.md
- Design documents describing the intended build order
- The `created_after` field will naturally reflect this once chunks ship

DEPENDS_ON:
- Declares explicit implementation dependencies that affect orchestrator scheduling
- Format: list of chunk directory name strings, or null
- Default: [] (empty list - explicitly no dependencies)

VALUE SEMANTICS (how the orchestrator interprets this field):

| Value             | Meaning                              | Oracle behavior   |
|-------------------|--------------------------------------|-------------------|
| `null` or omitted | "I don't know my dependencies"       | Consult oracle    |
| `[]` (empty list) | "I explicitly have no dependencies"  | Bypass oracle     |
| `["chunk_a"]`     | "I depend on these specific chunks"  | Bypass oracle     |

CRITICAL: The default `[]` means "I have analyzed this chunk and it has no dependencies."
This is an explicit assertion, not a placeholder. If you haven't analyzed dependencies yet,
change the value to `null` (or remove the field entirely) to trigger oracle consultation.

WHEN TO USE EACH VALUE:
- Use `[]` when you have analyzed the chunk and determined it has no implementation dependencies
  on other chunks in the same batch. This tells the orchestrator to skip conflict detection.
- Use `null` when you haven't analyzed dependencies yet and want the orchestrator's conflict
  oracle to determine if this chunk conflicts with others.
- Use `["chunk_a", "chunk_b"]` when you know specific chunks must complete before this one.

WHY THIS MATTERS:
The orchestrator's conflict oracle adds latency and cost to detect potential conflicts.
When you declare `[]`, you're asserting independence and enabling the orchestrator to
schedule immediately. When you declare `null`, you're requesting conflict analysis.

PURPOSE AND BEHAVIOR:
- When a list is provided (empty or not), the orchestrator uses it directly for scheduling
- When null, the orchestrator consults its conflict oracle to detect dependencies heuristically
- Dependencies express order within a single injection batch (intra-batch scheduling)
- The chunks listed in depends_on will be scheduled to complete before this chunk starts

CONTRAST WITH created_after:
- `created_after` tracks CAUSAL ORDERING (what work existed when this chunk was created)
- `depends_on` tracks IMPLEMENTATION DEPENDENCIES (what must complete before this chunk runs)
- `created_after` is auto-populated at creation time and should NOT be modified manually
- `depends_on` is agent-populated based on design requirements and may be edited

WHEN TO DECLARE EXPLICIT DEPENDENCIES:
- When you know chunk B requires chunk A's implementation to exist before B can work
- When the conflict oracle would otherwise miss a subtle dependency
- When you want to enforce a specific execution order within a batch injection
- When a narrative or investigation explicitly defines chunk sequencing

EXAMPLE:
  # Chunk has no dependencies (explicit assertion - bypasses oracle)
  depends_on: []

  # Chunk dependencies unknown (triggers oracle consultation)
  depends_on: null

  # Chunk B depends on chunk A completing first
  depends_on: ["auth_api"]

  # Chunk C depends on both A and B completing first
  depends_on: ["auth_api", "auth_client"]

VALIDATION:
- `null` is valid and triggers oracle consultation
- `[]` is valid and means "explicitly no dependencies" (bypasses oracle)
- Referenced chunks should exist in docs/chunks/ (warning if not found)
- Circular dependencies will be detected at injection time
- Dependencies on ACTIVE chunks are allowed (they've already completed)
-->

# Chunk Goal

## Minor Goal

Eliminate two viewport stability bugs that cause visible content shifting
during normal editing and terminal use, violating the project's low-latency
rendering goal (GOAL.md) and undermining smooth scrolling work delivered by
`viewport_fractional_scroll` and `pane_scroll_isolation`.

### Observed symptoms

1. **Terminal jostle**: ~1 in 3 terminal tabs develop a state where every
   keystroke causes the viewport to shift by ~1 line. Only terminals with
   sufficient scrollback history are affected. More noticeable in split panes.

2. **Buffer cursor jumps**: In a text buffer, moving the cursor in a way that
   should keep it visible (e.g., horizontal movement on the last rendered row)
   causes the viewport to jump as if the cursor were off-screen.

### Bug 1: Cold scrollback recapture inflates `line_count()`

`check_scrollback_overflow()` (`terminal_buffer.rs:647`) fires on every
`poll_events()` call when `history_size() > hot_scrollback_limit`. It copies
the oldest `lines_over_limit` lines to cold storage and increments
`cold_line_count`. **But it cannot remove lines from alacritty's grid**, so
`history_size()` never decreases and the condition remains true on the next
poll.

Every keystroke that produces PTY output (even a single echo character)
triggers `poll_events()` → `check_scrollback_overflow()`, which recaptures
the same lines and inflates `cold_line_count` by `lines_over_limit`. Since
`line_count() = cold_line_count + history_size() + screen_lines()`, the total
grows each time. The keystroke handler then calls
`scroll_to_bottom(line_count)`, which adjusts the viewport for the phantom
growth, shifting content by `lines_over_limit` lines.

**Why 1 in 3 terminals**: Only terminals that have produced > 2000 lines of
scrollback (`DEFAULT_HOT_SCROLLBACK_LIMIT`) trigger the overflow. Fresh or
lightly-used terminals stay under the limit.

**Key code path**:
- `terminal_buffer.rs:374` — `if processed_any` → `check_scrollback_overflow()`
- `terminal_buffer.rs:647` — `check_scrollback_overflow()`: condition
  `history_size > hot_scrollback_limit` is permanently true after first overflow
- `terminal_buffer.rs:728` — `cold_line_count += actual_count` (unbounded growth)
- `terminal_buffer.rs:874` — `line_count()` returns inflated sum
- `editor_state.rs:2192` — keystroke handler calls `scroll_to_bottom(line_count)`

**Missing guard**: `last_history_size` is tracked but never used in the
overflow condition. The fix should use it (or `cold_line_count`) to skip
recapture when no new lines have entered the hot scrollback since the
previous capture.

### Bug 2: `ensure_visible` off-by-one with partial row

`visible_range()` (`row_scroller.rs:146`) returns `first_row..(first_row +
visible_rows + 1)`, rendering `visible_rows + 1` rows to handle the
partially-visible bottom row when at a fractional scroll position. But
`ensure_visible_wrapped()` (`viewport.rs:348`) uses `visible_lines` as its
boundary:

```
if cursor_screen_row >= visible_lines {  // scrolls!
```

A cursor on screen row `visible_lines` (the +1 row) IS rendered and visible,
but `ensure_visible` considers it off-screen and scrolls. Similarly,
`ensure_visible_with_margin()` (`row_scroller.rs:206`) checks
`row >= first_row + effective_visible` which has the same off-by-one.

**Why more common in splits**: Splitting a window whose height divides evenly
by `line_height` creates panes with a fractional row, increasing the chance
the cursor lands on the +1 row.

**Key code path**:
- `row_scroller.rs:151` — `visible_range` uses `visible_rows + 1`
- `row_scroller.rs:206` — `ensure_visible` uses `visible_rows` (no +1)
- `viewport.rs:348` — `ensure_visible_wrapped` uses `visible_lines` (no +1)
- `context.rs:152` — `ensure_cursor_visible()` called after every cursor move
- `buffer_target.rs:345-398` — arrow keys, hjkl, word movement all call it

## Success Criteria

- Terminal tabs that have accumulated > 2000 lines of scrollback do not jostle
  on keystroke; `cold_line_count` is stable when no new output arrives
- A test verifies that `check_scrollback_overflow` does not increment
  `cold_line_count` on repeated calls when `history_size()` hasn't changed
- Moving the cursor to the last rendered row in a text buffer does not trigger
  viewport scrolling when the row is visible on screen
- A test verifies that `ensure_visible` does not scroll when the target row is
  within the `visible_range` (accounting for the +1 partial row)
- Existing viewport tests pass (scroll clamping, visible_range, fractional
  scroll, pane isolation)