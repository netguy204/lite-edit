---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/viewport.rs
- crates/editor/src/row_scroller.rs
code_references:
  - ref: crates/editor/src/row_scroller.rs#RowScroller
    implements: "RowScroller struct with 13 uniform-row scroll methods (new, row_height, visible_rows, first_visible_row, scroll_fraction_px, scroll_offset_px, set_scroll_offset_px, update_size, visible_range, scroll_to, ensure_visible, row_to_visible_offset, visible_offset_to_row)"
  - ref: crates/editor/src/viewport.rs#Viewport
    implements: "Viewport refactored to contain and delegate to RowScroller, preserving existing public API while adding row_scroller() accessor"
  - ref: crates/editor/src/main.rs
    implements: "Module declaration and pub use export of RowScroller from editor crate"
narrative: file_picker_viewport
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- renderer_styled_content
- terminal_emulator
- terminal_file_backed_scrollback
- workspace_model
- file_picker_mini_buffer
- mini_buffer_model
---

# Chunk Goal

## Minor Goal

Extract a `RowScroller` struct from `Viewport` that encapsulates the fractional
pixel scroll arithmetic for uniform-height rows. This is a pure structural
refactor — no behavior changes, no new features — that creates a reusable
primitive for any scrollable list surface (starting with the file picker
selector).

A method-by-method analysis of `Viewport` shows that thirteen of its methods are
pure arithmetic over three fields (`scroll_offset_px: f32`, `visible_rows:
usize`, `row_height: f32`) with no dependency on buffer or text concepts. Only
two methods are buffer-specific:

- `dirty_lines_to_region` — takes `DirtyLines` and returns `DirtyRegion`, both
  types from the buffer crate.
- `ensure_visible_wrapped` — takes a `WrapLayout` and a line-length closure to
  handle text lines that span multiple screen rows.

`RowScroller` gets the thirteen shared methods. `Viewport` is refactored to
contain a `RowScroller` and delegate to it, retaining the two buffer-specific
methods as `Viewport`-only additions.

The shared methods, with names appropriate for the general row domain:

| `RowScroller` method | Formula / behaviour |
|---|---|
| `new(row_height)` | Initialise with zero scroll, zero visible rows |
| `row_height()` | Getter |
| `visible_rows()` | Getter |
| `first_visible_row()` | `(scroll_offset_px / row_height).floor()` |
| `scroll_fraction_px()` | `scroll_offset_px % row_height` |
| `scroll_offset_px()` | Getter |
| `set_scroll_offset_px(px, row_count)` | Clamp to `(row_count - visible) * height` |
| `update_size(height_px)` | `visible_rows = floor(height_px / row_height)` |
| `visible_range(row_count)` | `first..min(first + visible + 1, row_count)` |
| `scroll_to(row, row_count)` | `row * height`, then clamp |
| `ensure_visible(row, row_count)` | Scroll up/down if row outside window |
| `row_to_visible_offset(row)` | `row - first_visible_row()` if in range |
| `visible_offset_to_row(offset)` | `first_visible_row() + offset` |

`Viewport` keeps its existing public API intact — all method names and signatures
stay the same, delegating to the inner `RowScroller`. The only public addition is
that `Viewport` exposes `row_scroller()` returning `&RowScroller` so downstream
code can pass a `RowScroller` reference without going through `Viewport`.

## Success Criteria

- `RowScroller` exists in `crates/editor/src/row_scroller.rs` with the thirteen
  methods listed above, all fully documented and unit-tested.
- `Viewport` contains a `RowScroller` field and delegates each of the thirteen
  shared methods to it. All existing `Viewport` public method signatures are
  unchanged.
- All existing `Viewport` tests pass without modification.
- `dirty_lines_to_region` and `ensure_visible_wrapped` remain on `Viewport` only
  and are not part of `RowScroller`.
- `RowScroller` has no dependency on the buffer crate, `DirtyLines`,
  `DirtyRegion`, or `WrapLayout`.
- `RowScroller` is exported from the editor crate's public surface so
  `selector.rs` can use it in the next chunk.
