---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/wrap_layout.rs
- crates/editor/src/glyph_buffer.rs
- crates/editor/src/buffer_target.rs
- crates/editor/src/viewport.rs
- crates/editor/src/renderer.rs
- crates/editor/src/lib.rs
- crates/editor/tests/wrap_test.rs
code_references: []
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
- word_double_click_select
- word_forward_delete
- word_jump_navigation
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

Lines longer than the viewport width currently overflow horizontally and are not
fully visible without horizontal scrolling. This chunk adds soft (visual) line
wrapping so that every character is always reachable within the viewport without
horizontal navigation.

Wrapping is purely a rendering concern — the underlying buffer stores lines as
they were entered and the cursor model continues to think in buffer columns. The
renderer splits a long buffer line into multiple screen rows on the fly, fitting
as many characters per screen row as the viewport width allows.

To make the boundary between a buffer line and its continuation rows immediately
legible, wrapped continuation rows receive a distinct left-edge treatment: a
solid black border drawn flush against the leftmost pixel of the row, covering
the full row height. No whitespace is introduced — the border sits inside the
existing content area and the first glyph of the continuation row starts
immediately to its right. The effect reads as a subtle indent marker without
consuming any horizontal space or altering the character-column mapping.

## Success Criteria

- Every character on every buffer line is visible without horizontal scrolling.
  A line whose glyph count exceeds `floor(viewport_width / glyph_width)` is
  split into multiple screen rows.
- The split is character-column exact: the first `N` characters occupy screen row
  0, the next `N` occupy screen row 1, and so on, where `N` is the number of
  fixed-width glyphs that fit in the viewport width at the current font size.
- Continuation rows (screen rows 2..k produced by a single buffer line) each
  render a solid black left-edge border one or two pixels wide, running the full
  height of the row, with no gap between the border and the first glyph.
- The first screen row of every buffer line (including lines that do not wrap)
  has no left-edge border, preserving the visual distinction between "this is a
  new line" and "this is a continuation of the previous line."
- Cursor rendering is correct: the cursor appears at the screen row and column
  that correspond to the cursor's buffer column after splitting.
- Selection rendering is correct: highlighted spans cross wrap boundaries
  naturally, colouring the appropriate portion of each screen row.
- The viewport's visible-line count and scroll arithmetic account for the
  expanded screen-row count so that `ensure_visible` and fractional scroll both
  operate correctly after wrapping is introduced.
- Mouse click hit-testing is correct throughout. A click on a continuation
  screen row resolves to the buffer line that owns that row and the buffer column
  derived from the click's X position plus the column offset of that screen row's
  first character. A click on any screen row that follows a wrapped buffer line
  (i.e. the first screen row of the next buffer line) resolves to that next buffer
  line at column 0 plus the X-derived offset — not to the tail of the preceding
  wrapped line.
- Wrapping must not change the time complexity of any buffer or viewport
  operation. Operations that were O(1) before (cursor movement, single-line
  lookup, hit-test for a given screen position) must remain O(1). The
  screen-row count for a single buffer line is derivable in O(1) from its
  character count and the viewport column width, so no full-buffer scan is
  needed to map a buffer position to a screen row or vice versa, and no
  such scan should be introduced.
- No horizontal scroll offset exists or is reachable; the editor is
  viewport-width constrained at all times once this chunk is implemented.

## Implementation Notes

### Logical lines vs visual lines

The established vocabulary for this problem distinguishes *logical lines* (buffer
lines, what the buffer model sees) from *visual lines* (screen rows, what the
renderer draws). The invariant that preserves time complexity is: **all buffer
operations stay in logical-line coordinates**. Only the renderer and hit-tester
ever translate between the two. Nothing in the edit path should touch visual-line
arithmetic.

### Why this editor has it easy: monospace arithmetic

The general version of this problem (tackled at length in Raph Levien's xi-editor
"rope science" series) is hard because variable-width fonts require pixel-level
layout measurement to determine where lines break. xi solves this with a b-tree
storing cached break positions and incremental invalidation — O(log n) operations
throughout.

This editor uses a fixed-width font. That reduces every coordinate mapping to
integer arithmetic:

```
cols_per_row   = floor(viewport_width_px / glyph_width_px)
screen_rows(line)   = ceil(line.char_count / cols_per_row)     // O(1)
screen_pos(buf_col) = divmod(buf_col, cols_per_row)            // O(1) → (row_offset, col)
buffer_col(row_off, col) = row_off * cols_per_row + col        // O(1)
```

No cache, no data structure, no invalidation. Introduce a small `WrapLayout`
struct (or module-level helpers) holding `cols_per_row` that exposes these three
functions. It becomes the single source of truth for all coordinate mapping in
rendering, cursor placement, selection, and hit-testing.

### The rendering loop

The existing loop iterates `viewport.visible_range()` (a range of buffer line
indices) and emits one screen row per buffer line. Change it to: start at
`first_visible_line`, iterate buffer lines, for each emit
`screen_rows(line.char_count)` screen rows, and stop when the accumulated screen
row count fills the viewport. No global index is needed — only the lines
currently being rendered are ever examined.

### Hit-testing (screen → buffer)

Given a click at `(x_px, y_px)`:

1. `click_screen_row = floor((y_px + scroll_fraction_px) / line_height_px)`
2. Walk forward from `first_visible_line`, subtracting `screen_rows(line)` from
   a counter until the counter would go negative. The current line owns the click.
3. `row_offset_within_line = remaining_counter`
4. `buffer_col = buffer_col(row_offset_within_line, floor(x_px / glyph_width_px))`

This is O(visible_lines), which is a fixed constant (~30–80 rows), not O(document
length). No global data structure is required or appropriate here.

### Scroll arithmetic

`scroll_offset_px` is already pixel-based (from the `viewport_fractional_scroll`
chunk). Pixel space is neutral with respect to wrapping — the viewport scrolls
through a continuous run of screen rows, each `line_height_px` tall, regardless
of whether adjacent screen rows belong to the same logical line or different ones.
The only change needed is that `ensure_visible` must compute the pixel offset of
the *first screen row* of the target buffer line, which requires summing
`screen_rows(line)` for all buffer lines before it — but only for lines from
`first_visible_line` to the target, a bounded scan.

### Do not reach for a global wrap index

Any approach that pre-computes or caches a document-wide array of cumulative
screen-row offsets is over-engineered for this case and requires complex
invalidation on every edit. The viewport bound makes it unnecessary: you only
ever need to resolve screen rows within the visible window.

## Rejected Ideas

### Left-padding / indentation on continuation rows

Adding actual whitespace or an indented left margin would shift the column origin
of continuation rows relative to their buffer columns, complicating the
character-to-screen mapping and wasting horizontal space. The black left-border
treatment achieves the same perceptual goal (obvious wrap indicator) with zero
layout impact.

### Incremental wrap cache (xi-editor style)

xi-editor maintains a b-tree of cached line-break positions with incremental
invalidation to handle variable-width fonts at O(log n). This editor's fixed-width
font makes per-line wrap counts pure O(1) arithmetic with no state to maintain,
so an incremental cache would add complexity with no benefit.

---