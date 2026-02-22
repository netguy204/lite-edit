---
status: FUTURE
ticket: null
parent_chunk: null
code_paths: []
code_references: []
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- terminal_emulator
- clipboard_operations
- mouse_drag_selection
- word_double_click_select
created_after: ["scroll_bottom_deadzone_v3", "terminal_pty_wakeup", "terminal_styling_fidelity"]
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

# Terminal Clipboard Selection: Copy/Paste with Mouse and Keyboard

## Minor Goal

Enable copy/paste in the terminal tab using mouse selection and standard macOS keyboard shortcuts. Users need to be able to select text in terminal output by clicking and dragging (or double-clicking to select a word), then copy with Cmd+C, and paste with Cmd+V. Terminal history (scrollback and already-rendered output) must not be editable — selection is read-only over the rendered terminal grid.

This is a fundamental usability feature: without it, users cannot extract text from terminal output or paste commands into the terminal. The text editor already has mouse drag selection (`mouse_drag_selection`), word double-click selection (`word_double_click_select`), and clipboard operations (`clipboard_operations`). This chunk brings equivalent capabilities to the terminal, adapted for the terminal's read-only grid model.

## Success Criteria

- **Click-and-drag selects text in terminal output**: When the user clicks and drags over terminal content (both live viewport and scrollback), the dragged region is visually highlighted as a selection. Selection operates over the terminal's character grid — it selects rendered cell content, not raw escape sequences. Selection coordinates are in terminal grid positions (column, row), not buffer byte offsets.

- **Double-click selects a word**: Double-clicking on a word in the terminal grid selects the entire word (using word boundary detection on the terminal cell content). This mirrors the existing `word_double_click_select` behavior adapted for the terminal's cell grid rather than a `TextBuffer`.

- **Cmd+C copies selection to system clipboard**: When terminal text is selected and the user presses Cmd+C, the selected text is copied to the macOS system clipboard via the existing `clipboard::copy_to_clipboard()`. The selection should be converted from grid cells to a string, joining rows with newlines. After copying, the selection may optionally be cleared (standard terminal emulator behavior).

- **Cmd+C without selection is a no-op**: When no text is selected in the terminal and the user presses Cmd+C, nothing happens. The interrupt signal is sent by Ctrl+C (which is already handled by `TerminalFocusTarget::handle_key()` as a normal key event encoded to `\x03`). Cmd+C is exclusively for clipboard copy.

- **Cmd+V pastes into terminal**: When the user presses Cmd+V, the system clipboard content is read via `clipboard::paste_from_clipboard()` and written to the terminal's PTY using `TerminalFocusTarget::write_paste()`, which already handles bracketed paste mode. The pasted text goes to the running process as input — it does not modify the terminal grid directly.

- **History is not editable**: Mouse selection and clipboard operations are strictly read-only over the terminal grid. Clicking or dragging does not move a cursor or modify cell content. The only way to "write" to the terminal is via Cmd+V paste, which sends input to the PTY (the running process decides what to do with it). There is no insert cursor in the terminal — the terminal's cursor position is controlled entirely by the running process.

- **Selection state lives on TerminalBuffer or TerminalFocusTarget**: A selection model (anchor + head grid coordinates) is maintained for the terminal. This is separate from the text editor's `TextBuffer` selection model. The selection is purely visual — it highlights cells in the rendered output for copy purposes.

- **Selection renders with highlight**: Selected cells in the terminal are rendered with a visible highlight (e.g., inverted colors or a selection background color), consistent with how the text editor renders selections. The renderer must handle terminal selection highlighting through the existing `BufferView` / styled line pipeline.

- **Selection clears on terminal output**: When new output arrives from the PTY (the terminal content changes), any active selection should be cleared to avoid stale/misaligned highlights. This is standard terminal emulator behavior.

- **Mouse events not consumed when TUI app requests mouse**: When the terminal is running a TUI app that has enabled mouse reporting (e.g., htop, vim with mouse mode), mouse events should be forwarded to the app via the PTY rather than interpreted as selection. The existing `TerminalFocusTarget::handle_mouse()` already checks for active mouse modes — selection should only engage when no mouse mode is active.

- **Unit tests**:
  - Click-and-drag over terminal grid produces correct selection range
  - Double-click selects the word at the clicked position
  - Cmd+C with selection copies the correct text to the mock clipboard
  - Cmd+C without selection is a no-op (does not write to clipboard, does not send interrupt)
  - Cmd+V reads from clipboard and calls `write_paste()`
  - Selection coordinates correctly map between pixel positions and grid cells
  - Selection is cleared when new PTY output arrives