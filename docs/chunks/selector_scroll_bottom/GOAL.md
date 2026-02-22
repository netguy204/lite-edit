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
bug_type: implementation
depends_on: []
created_after: ["tab_bar_content_clip", "click_scroll_fraction_alignment"]
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
-->

# Chunk Goal

## Minor Goal

Two related scrolling defects in the selector overlay (file picker, command
palette):

### Bug A — Picker opens showing only one item

`SelectorWidget::new()` initialises `RowScroller` with `visible_rows = 0`.
`update_visible_size` is called only inside event handlers
(`handle_key_selector`, `handle_mouse_selector`, `handle_scroll_selector`), so
the very first render — before any user interaction — uses `visible_item_range()
= 0..1`. The panel appears at full height but only the top item is drawn; the
rest of the list area is blank. After any key/scroll event `visible_rows` snaps
to the correct panel-capacity value and the list fills normally. Fix: call
`update_visible_size` (using the same geometry calculation already done in the
event handlers) inside `open_file_picker`, immediately after `set_items`.

### Bug B — Cannot arrow-key to the bottom of a long match list

Observed behaviour: when arrow-keying down through a full panel (more items
than fit), the selection highlight eventually appears partially clipped at the
bottom edge of the panel, then on the next Down press it disappears entirely,
and pressing Enter at that point confirms a different item than the one
visually highlighted.

**Suspected root cause — scissor bottom vs. logical scroll bottom mismatch.**

The scissor rect height is computed in `selector_list_scissor_rect` as:

```
height = floor(panel_y + panel_height - list_origin_y)
       = floor(visible_items * line_height + OVERLAY_PADDING_Y)
```

The scroll logic's `ensure_visible` considers the selection visible as long as
`selected_index < first_visible + visible_rows`, where
`visible_rows = visible_items` (set by `update_visible_size`). When
`ensure_visible` fires it places the selection at draw_idx `visible_rows - 1`
(the last "proper" slot). That slot renders from
`list_y + (visible_rows - 1) * line_height` to `list_y + visible_rows *
line_height`, where `list_y = list_origin_y - scroll_frac`.

The `visible_item_range()` always includes a `+1` extra row for smooth
fractional-scroll rendering. If the selection ever lands at draw_idx
`visible_rows` (the extra-row slot), `ensure_visible` would have failed to
scroll in time, producing the partially/fully off-screen highlight the user
sees.

**State staleness window**: `update_visible_size` is called at the START of
each event handler using `selector.items().len()` (the pre-event item count).
When typing changes the query, `set_items` is called AFTER `handle_key`,
leaving `visible_rows` derived from the old count for the duration of the
resulting render. If the old count is larger than the new count and
`max_visible_items` changes as a result, the scissor (computed from
`widget.items().len()` at render time) and `visible_rows` (set from the old
count) can describe different panel heights.

**Pixel truncation**: The scissor rect truncates float coordinates with
`as usize`. If `list_origin_y` or `panel_height` have fractional parts (e.g.,
from non-integer `line_height * 0.2 * view_height` arithmetic), the scissor
may be 1 pixel shorter than expected on a given frame, clipping the last
item's final pixels.

The implementer should:
1. Add `update_visible_size` to `open_file_picker` (fixes Bug A).
2. Trace the exact values of `visible_rows`, `visible_items` (from geometry),
   scissor `y + height`, and the pixel extent of the last rendered item for
   a representative long list. Assert that the scissor bottom always ≥ the last
   full item's bottom pixel, and that `ensure_visible` never leaves the
   selection at draw_idx ≥ `visible_rows`.
3. If truncation is found to be the culprit, round the scissor rect outward
   (`ceiling` for height, `floor` for y) rather than truncating both downward.
4. If the staleness window is the culprit, call `update_visible_size` a second
   time AFTER `set_items` in the typing branch of `handle_key_selector`.

## Success Criteria

- Opening the file picker immediately shows the full panel of items (up to
  `max_visible_items`), not just the first item.
- Arrow-keying from the first to the last item in a list longer than the panel
  height keeps the selection highlight fully visible on every frame — no
  bottom-edge clipping, no invisible highlight.
- Pressing Enter always confirms the visually highlighted item.
- New unit tests in `selector.rs` cover:
  - Initial `visible_item_range` matches panel capacity after `update_visible_size`
    is called (as it will be after `open_file_picker` is fixed).
  - Navigating to the last item of a list larger than `visible_rows` leaves the
    selection at draw_idx `visible_rows - 1`, fully within the scissor-clipped
    list area.
- No regressions in existing `selector.rs` or `selector_overlay.rs` tests.
