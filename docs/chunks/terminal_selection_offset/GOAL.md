---
status: HISTORICAL
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/editor_state.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_mouse_event
    implements: "Wrap-aware terminal click coordinate mapping using viewport_scroll subsystem"
narrative: null
investigation: null
subsystems:
  - subsystem_id: viewport_scroll
    relationship: uses
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- terminal_spawn_reliability
- treesitter_gotodef_type_resolution
---

# Chunk Goal

## Minor Goal

Fix terminal pane mouse selection appearing at the wrong position. Clicking on a terminal line highlights a different line, offset by the number of soft-wrapped lines above the click point. The terminal click handler uses a linear `first_visible_line() + row` mapping that assumes 1:1 screen-row-to-document-line correspondence, but the renderer correctly uses wrap-aware `first_visible_screen_row()` + `buffer_line_for_screen_row()`.

## Bug Details

**Root cause:** `editor_state.rs` line 3069:

```rust
let doc_line = viewport.first_visible_line() + row;
```

This assumes each screen row corresponds to exactly one document line. When terminal output has lines wider than the viewport (e.g., multi-column `ls` output), those lines soft-wrap to multiple screen rows. Each wrapped line adds +1 to the offset between the click's visual position and the calculated document line.

The viewport documentation (`viewport.rs:79-81`) explicitly warns about this:

> **Note**: When soft line wrapping is enabled, use `first_visible_screen_row()` and `buffer_line_for_screen_row()` instead. This method assumes a 1:1 mapping between buffer lines and screen rows, which is only correct without wrapping.

**The renderer does it correctly** (`glyph_buffer.rs:1267-1273`): it uses `first_visible_screen_row()` and `buffer_line_for_screen_row()` with wrap layout tracking via `cumulative_screen_row`.

**The file editor click handler also does it correctly** (`buffer_target.rs:885-957`): `pixel_to_buffer_position_wrapped()` uses `first_visible_screen_row` and `Viewport::buffer_line_for_screen_row()`.

Only the terminal click handler (inline in `editor_state.rs:3041-3127`) uses the wrong, non-wrap-aware mapping.

**Reproduction:**

1. Open a terminal pane in lite-edit
2. Run commands that produce output with long lines (e.g., `ls` in a directory with many entries, or git output)
3. Try to select text by clicking/dragging on a line — the selection highlight appears several lines below the click point
4. The offset equals the number of soft-wrapped screen rows above the click position

**Key code locations:**

- `crates/editor/src/editor_state.rs:3057-3070` — Terminal click coordinate calculation (WRONG)
- `crates/editor/src/editor_state.rs:3072-3122` — Terminal selection anchor/head setting (uses wrong position)
- `crates/editor/src/buffer_target.rs:885-957` — File editor click handler (CORRECT, for reference)
- `crates/editor/src/glyph_buffer.rs:1267-1273` — Terminal renderer (CORRECT, for reference)
- `crates/editor/src/viewport.rs:79-81` — Documentation warning about `first_visible_line` vs wrapping

**Fix approach:** Replace the terminal click handler's linear mapping with the same wrap-aware approach used by the file editor and renderer: compute `absolute_screen_row = first_visible_screen_row + row`, then call `Viewport::buffer_line_for_screen_row()` to get the correct document line and column offset within a wrapped line.

## Success Criteria

- Clicking on a terminal line selects text at the correct visual position, even when lines above have soft-wrapped
- Double-click word selection in the terminal works at the correct position
- Drag selection in the terminal tracks the mouse position accurately
- Terminal mouse events forwarded to the PTY (when mouse mode is active) use correct cell coordinates
- The fix follows the same pattern as `pixel_to_buffer_position_wrapped` in `buffer_target.rs`