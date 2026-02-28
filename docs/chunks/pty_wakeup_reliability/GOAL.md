---
status: FUTURE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/pty_wakeup.rs
- crates/editor/src/event_channel.rs
- crates/editor/src/drain_loop.rs
code_references: []
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after: ["terminal_fullscreen_paint", "terminal_image_paste", "terminal_word_delete"]
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

Full-screen, mostly-static TUI apps like vim do not reliably paint their initial screen content when launched in a terminal tab. The terminal shows only vim's primary-screen startup message (e.g., `"file.md" [readonly][noeol] 52L, 2072B`) while the alt-screen content (file text, `~` lines, status bar) is blank. Typing keys does not trigger a repaint, but moving the tab to a split pane does (because resize forces SIGWINCH + full redraw). Continuously-refreshing apps like htop work fine because they constantly produce output.

The root cause is a fragile two-hop dispatch with double debouncing in the PTY wakeup pipeline:

```
PTY reader thread
  → PtyWakeup::signal()          [debounce #1: PtyWakeup.pending AtomicBool]
    → DispatchQueue::main().exec_async()    [GCD hop — non-deterministic timing]
      → EventSender::send_pty_wakeup()      [debounce #2: wakeup_pending AtomicBool]
        → mpsc channel + CFRunLoopSourceSignal
          → process_pending_events()
            → poll_events() [4KB byte budget]
```

Three factors combine to lose wakeups:

1. **GCD timing gap**: The `exec_async` closure runs at GCD's discretion. If the CFRunLoopSource callback fires (from a prior signal) in the same run loop iteration BEFORE the GCD block runs, the PtyWakeup event is deferred to a later iteration.

2. **Double debounce suppression**: `PtyWakeup.pending` and `EventSender.wakeup_pending` independently gate signals, cleared at different points in the event loop. A narrow race between their clearing can suppress a wakeup.

3. **Byte budget fragmentation**: Vim's initial paint (~5-20KB) exceeds the 4KB per-poll budget, requiring the follow-up wakeup chain (`send_pty_wakeup_followup`) to fire reliably for each continuation. If any link in that chain is lost, the remaining data sits unprocessed until the cursor blink timer fires (up to 500ms later).

The fix should eliminate the GCD indirection by signaling the CFRunLoopSource directly from the PTY reader thread (both `CFRunLoopSourceSignal` and `CFRunLoopWakeUp` are thread-safe), and collapse the double debounce into a single atomic flag.

## Design Constraints

### Debounce must protect against noisy PTYs

A critical function of the current debounce is preventing high-throughput terminal output (e.g., `cat /dev/urandom`, massive build logs) from flooding the main thread with wakeup signals and causing input lag. The fix must preserve at-most-one-wakeup-per-drain-cycle coalescing. The single atomic flag achieves this: many rapid `PtyWakeup::signal()` calls between drain cycles collapse into one CFRunLoopSource wake. The actual input-lag protection comes from the byte budget (`terminal_flood_starvation`) and priority partitioning (input events processed before PtyWakeup), which are unchanged.

### App Nap compatibility

The `app_nap_activity_assertions` chunk manages an `NSActivityUserInitiated` assertion held while terminals are active, released after 2s quiescence. This is managed on the main thread inside `poll_agents()`. The wakeup delivery change (GCD → direct CFRunLoopSource) does not affect App Nap interaction because:

- Both `CFRunLoopWakeUp()` and `DispatchQueue::main().exec_async()` are equally subject to App Nap throttling when the process is napped
- The activity assertion prevents napping when terminals are active, so the wakeup path is not throttled during active use
- The assertion hold/release logic in `poll_agents()` runs on the main thread inside `process_pending_events()` regardless of how the wakeup was delivered
- During idle→active transitions (first PTY data after quiescence), both mechanisms have the same wakeup latency characteristics under App Nap

## Success Criteria

- Opening vim (or another static fullscreen app like less, man) in a terminal tab paints complete initial screen content within one render frame of the PTY output arriving — no blank screen
- Typing in vim updates the display immediately (no frozen frames requiring split-pane to fix)
- The existing `terminal_fullscreen_paint` alt-screen detection continues to function correctly (it marks dirty on mode transition; this chunk ensures the wakeup that triggers it arrives reliably)
- Animated apps like htop continue to render correctly (no regression)
- The `terminal_flood_starvation` byte-budget and follow-up wakeup mechanism continues to work (the follow-up path must also use the direct CFRunLoopSource signal)
- Noisy PTY output does not cause input lag — debounce coalescing is preserved (at-most-one wakeup per drain cycle), byte budget bounds processing cost, and priority partitioning ensures input events are processed first
- App Nap behavior is unchanged — activity assertion is held during terminal activity and released during quiescence, regardless of the wakeup delivery mechanism
- The cursor blink timer backup polling (`handle_cursor_blink` calling `poll_agents`) remains as a defense-in-depth safety net but is no longer the primary recovery mechanism
- No new race conditions introduced — the wakeup is at-least-once (redundant signals are harmless; lost signals are not)