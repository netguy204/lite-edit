---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/selector.rs
code_references: []
narrative: file_picker_viewport
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- selector_coord_flip
- row_scroller_extract
created_after:
- renderer_styled_content
- terminal_emulator
- terminal_file_backed_scrollback
- workspace_model
- file_picker_mini_buffer
- mini_buffer_model
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

Replace `SelectorWidget`'s broken integer scroll model with `RowScroller`,
giving the file picker the same fractional-pixel scroll behaviour as the main
buffer viewport.

`SelectorWidget` currently tracks scroll position as `view_offset: usize` — an
integer row index — and computes scroll steps by rounding raw pixel deltas:
`(delta_y / item_height).round() as isize`. This discards the fractional
remainder on every event, producing choppy snapping to row boundaries instead of
smooth glide.

After this chunk `SelectorWidget` will contain a `RowScroller` as its
authoritative scroll state. The following internal changes are required:

**Field changes:**
- Remove `view_offset: usize` and `visible_items: usize`.
- Add `scroll: RowScroller`.

**`handle_scroll` — accumulate raw deltas without rounding:**
```rust
// Before
let rows = (delta_y / item_height).round() as isize;
self.view_offset = (self.view_offset as isize + rows).clamp(...) as usize;

// After
let new_px = self.scroll.scroll_offset_px() + delta_y as f32;
self.scroll.set_scroll_offset_px(new_px, self.items.len());
```
The `item_height` and `visible_items` parameters are removed from the public
`handle_scroll` signature; the scroller already knows `row_height` and
`visible_rows` from its own state. Callers update via `set_item_height` /
`update_visible_size` (see below).

**`set_visible_items` → `update_visible_size(height_px)`:**
Replace the integer setter with one that forwards to
`RowScroller::update_size(height_px)`, keeping the scroller's `visible_rows`
derived from the pixel height rather than stored as a raw count.

**`set_items` — re-clamp scroll after list changes:**
```rust
// Re-clamp scroll to the new item count without resetting position
let px = self.scroll.scroll_offset_px();
self.scroll.set_scroll_offset_px(px, self.items.len());
```

**Arrow-key navigation — use `ensure_visible`:**
```rust
// After moving selected_index, keep it in view:
self.scroll.ensure_visible(self.selected_index, self.items.len());
```
Remove the manual `view_offset` adjustment that currently duplicates this logic.

**`handle_mouse` hit-testing — account for fractional offset:**
```rust
let first = self.scroll.first_visible_row();
let frac  = self.scroll.scroll_fraction_px() as f64;
let relative_y = position.1 - list_origin_y + frac;
let row = (relative_y / item_height).floor() as usize;
let item_index = first + row;
```
The `frac` term corrects for the sub-row scroll position so that the clicked
pixel maps to the same item the renderer draws at that position.

**New public accessors for the renderer:**
- `first_visible_item() -> usize` — delegates to `scroll.first_visible_row()`
- `scroll_fraction_px() -> f32` — delegates to `scroll.scroll_fraction_px()`
- `visible_item_range(item_count: usize) -> Range<usize>` — delegates to
  `scroll.visible_range(item_count)`

The existing `view_offset()` accessor is removed; callers migrate to
`first_visible_item()`.

## Success Criteria

- `SelectorWidget` contains a `RowScroller` field with no remaining `view_offset`
  or `visible_items` fields.
- `handle_scroll` accumulates raw `f32` pixel deltas via
  `RowScroller::set_scroll_offset_px`; the `item_height` and `visible_items`
  parameters are removed from its signature.
- Arrow-key navigation calls `scroll.ensure_visible` instead of manually
  adjusting an integer offset.
- `set_items` re-clamps the scroll offset to the new item count without
  resetting it to zero.
- `handle_mouse` hit-testing adds `scroll_fraction_px()` to `relative_y` before
  dividing by `item_height`.
- `first_visible_item()`, `scroll_fraction_px()`, and `visible_item_range()` are
  public on `SelectorWidget`.
- The old `view_offset()` accessor is removed; no external callers remain.
- All existing `SelectorWidget` tests are updated to use the new API and pass.
  The scroll-behaviour tests (smooth accumulation, no rounding) are extended to
  assert that sub-row deltas are preserved across multiple scroll events.
