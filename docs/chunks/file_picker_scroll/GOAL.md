---
status: IMPLEMENTING
ticket: null
parent_chunk: file_picker
code_paths:
- crates/editor/src/selector.rs
- crates/editor/src/selector_overlay.rs
- crates/editor/src/editor_state.rs
code_references: []
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- file_picker
created_after:
- file_save
- viewport_fractional_scroll
- word_boundary_primitives
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

# File Picker Scroll

## Minor Goal

The file picker overlay currently ignores scroll events (`handle_scroll` in
`EditorState` is a no-op when the selector is open). When a project has more
files than the overlay's visible rows, there is no way to reach items below the
fold — the user is stuck with whatever fits on screen.

This chunk adds scroll support to the file picker overlay so that trackpad and
mouse wheel scroll events pan the item list, making every matched file reachable
without having to narrow the query further.

## Success Criteria

### `SelectorWidget` gains a `view_offset` field

- `SelectorWidget` gains a `view_offset: usize` field (default `0`), representing
  the index of the first item visible in the list.
- A public `view_offset(&self) -> usize` accessor is added.

### `SelectorWidget::handle_scroll` method

- A new method is added:
  ```rust
  pub fn handle_scroll(&mut self, delta_y: f64, item_height: f64, visible_items: usize)
  ```
- `delta_y` is the raw pixel delta (positive = scroll down / content moves up,
  matching the existing `ScrollDelta` sign convention used by the buffer viewport).
- The number of rows to shift is `(delta_y / item_height).round() as isize`.
- `view_offset` is clamped so the last visible row never exceeds the last item:
  `view_offset` stays in `0..=(items.len().saturating_sub(visible_items))`.
- Scrolling on an empty list or a list that fits entirely within `visible_items`
  is a no-op.

### Arrow key navigation keeps selection visible

- `handle_key` (Up / Down arrow) updates `view_offset` after moving
  `selected_index` so the newly selected item is always within the visible window:
  - If `selected_index < view_offset`, set `view_offset = selected_index`.
  - If `selected_index >= view_offset + visible_items`, set
    `view_offset = selected_index - visible_items + 1`.
- This requires passing `visible_items: usize` to `handle_key`, or storing the
  most-recently-known `visible_items` on the widget.  
  **Preferred approach:** add a `visible_items: usize` field (default `0`)
  updated by a new `set_visible_items(n: usize)` setter, so `handle_key` can
  reference it without parameter changes.

### `set_items` clamps `view_offset`

- When `set_items` is called (e.g., after a query change narrows the list),
  `view_offset` is clamped to
  `0..=(items.len().saturating_sub(self.visible_items))` so it cannot point
  past the new end of the list.

### `handle_mouse` is offset-aware

- `handle_mouse` currently maps a clicked row index directly to `items[row]`.
  With scroll, the actual item index is `view_offset + row`.
- Update `handle_mouse` to compute the true item index as `view_offset + row`
  when setting/confirming `selected_index`.

### Renderer uses `view_offset`

- `selector_overlay.rs` `SelectorGlyphBuffer::update_from_widget` currently
  iterates `widget.items().iter().take(geometry.visible_items)`.
- Change this to:
  ```rust
  widget.items()
      .iter()
      .skip(widget.view_offset())
      .take(geometry.visible_items)
  ```
  so only the visible window of items is rendered.
- The selection highlight quad must be rendered at the row position of the
  selected item within the visible window:
  `visible_row = selected_index.wrapping_sub(view_offset)`.
  If `selected_index` is outside the visible window, omit the selection
  highlight (emit an empty quad range for `selection_range`).

### `EditorState::handle_scroll` forwards events when selector is open

- Remove the early-return no-op that ignores scroll events when the selector is
  open.
- When `focus == Selector`, forward the scroll event to the selector:
  ```rust
  let item_height = /* geometry.item_height as f64 */;
  let visible = /* geometry.visible_items */;
  self.active_selector.as_mut().unwrap()
      .handle_scroll(delta.dy as f64, item_height, visible);
  ```
- The geometry values needed (`item_height`, `visible_items`) must be derived
  from the same `OverlayGeometry` calculation already used by the renderer.
  Store the most-recently computed `OverlayGeometry` on `EditorState` (as
  `Option<OverlayGeometry>`) and update it each time the overlay is rendered, or
  recompute it on demand using the current viewport dimensions and line height.
- After forwarding, mark `DirtyRegion::FullViewport` so the updated list is
  redrawn.

### Tests

- `selector.rs` unit tests cover:
  - Scrolling down moves `view_offset` forward; clamped at max valid offset.
  - Scrolling up moves `view_offset` backward; clamped at 0.
  - Scrolling on a list that fits within `visible_items` is a no-op.
  - Arrow-key down past the bottom of the visible window increments `view_offset`.
  - Arrow-key up past the top of the visible window decrements `view_offset`.
  - `set_items` with a shorter list clamps `view_offset`.
  - `handle_mouse` click on visible row selects the correct item index
    (`view_offset + row`).
- `editor_state.rs` unit tests cover:
  - Scroll event while selector is open updates `view_offset` (not ignored).
  - Scroll event while selector is closed still scrolls the buffer viewport.

### Manual smoke test

- Open the file picker (Cmd+P) in a directory with more files than visible rows.
- Scroll down: the item list pans to reveal files not initially shown.
- Scroll up: the list pans back.
- Use arrow keys past the visible boundary: the list scrolls to keep the
  selection on screen.
- Click an item that was previously off-screen: it is selected and confirmed
  correctly.

## Relationship to Parent

The `file_picker` chunk established the Cmd+P overlay, `SelectorWidget`
interaction model, and renderer. It explicitly documented that scroll events are
**ignored** while the selector is open (`handle_scroll` early-return in
`EditorState`). This chunk lifts that restriction and wires scroll events through
to the selector, completing the interaction model for long file lists.