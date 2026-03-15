---
status: ACTIVE
ticket: null
parent_chunk: cursor_wrap_scroll_alignment
code_paths:
- crates/editor/src/viewport.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/context.rs
code_references:
  - ref: crates/editor/src/viewport.rs#Viewport::ensure_visible_wrapped
    implements: "Fix coordinate space: always compute absolute cursor screen row from buffer line 0 instead of caller-provided first_visible_line"
  - ref: crates/editor/src/editor_state.rs#EditorState::ensure_cursor_visible_in_active_tab
    implements: "Updated call site: removed first_visible_line() argument from ensure_visible_wrapped call"
  - ref: crates/editor/src/context.rs#EditorContext::ensure_cursor_visible
    implements: "Updated call site: removed first_visible_line() argument from ensure_visible_wrapped call"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_scroll_leak
- tsx_goto_import
---

# Chunk Goal

## Minor Goal

When editing a soft-wrapped file, scrolling to the cursor (e.g. after keystroke,
goto-definition, or any action that triggers `ensure_cursor_visible_in_active_tab`)
jumps the viewport to the wrong position—frequently far above the actual cursor
location.

The root cause is a coordinate-space confusion in the call chain. In wrapped mode,
`scroll_offset_px` is measured in **screen row** units (set by
`ensure_visible_wrapped`). `Viewport::first_visible_line()` computes
`floor(scroll_offset_px / line_height)`, which returns a **screen row index**, not
a buffer line index. But `ensure_cursor_visible_in_active_tab` (editor_state.rs:4341)
passes `first_visible_line()` to `ensure_visible_wrapped` as the `first_visible_line`
parameter. The function then uses this value as a buffer line index in its
accumulation loop (viewport.rs:319):

```rust
for buffer_line in first_visible_line..cursor_line.min(line_count) {
```

When wrapped lines are present, the screen row number is larger than the
corresponding buffer line index. The loop therefore starts too late, under-counts
cumulative screen rows, and produces a wrong scroll target.

The parent chunk `cursor_wrap_scroll_alignment` fixed the rendering path
(`GlyphBuffer::update_from_buffer_with_wrap` correctly uses
`buffer_line_for_screen_row`), but the scroll-to-cursor path still feeds a screen
row through `first_visible_line()` into `ensure_visible_wrapped`.

## Success Criteria

- `ensure_visible_wrapped` receives an actual buffer line index (or computes one
  internally via `buffer_line_for_screen_row`), never a raw screen row number
  masquerading as a buffer line.
- After any cursor movement in a wrapped file, `ensure_cursor_visible_in_active_tab`
  scrolls the viewport so the cursor is visible and at the correct position—not
  above or below the actual cursor line.
- Scrolling to a cursor that is on a continuation row (second or later screen row
  of a wrapped buffer line) places the viewport correctly.
- Scrolling to a cursor with many wrapped lines above it does not overshoot.
- No regression to non-wrapped scrolling behavior.
- Tests covering:
  - Cursor below viewport with wrapped lines above: viewport scrolls to correct
    position.
  - Cursor above viewport with wrapped lines below: viewport scrolls to correct
    position.
  - Cursor on a continuation row of a wrapped line.
  - Cursor in a non-wrapped document (existing behavior preserved).

## Relationship to Parent

`cursor_wrap_scroll_alignment` fixed the rendering coordinate space: the glyph
buffer now correctly uses `buffer_line_for_screen_row` to map from scroll position
to buffer line for rendering. However, it did not fix the upstream scroll
calculation in `ensure_visible_wrapped`, which still receives a screen-row-derived
value from `first_visible_line()` and treats it as a buffer line index. This chunk
completes the fix by ensuring the scroll-to-cursor path also operates in the
correct coordinate space.