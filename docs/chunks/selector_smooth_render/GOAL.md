---
status: IMPLEMENTING
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/selector_overlay.rs
code_references: []
narrative: file_picker_viewport
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on:
- selector_row_scroller
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

Update `SelectorGlyphBuffer` to consume `scroll_fraction_px()` from
`SelectorWidget` when placing item quads, giving the file picker list the same
smooth sub-row glide as the main buffer.

Currently `SelectorGlyphBuffer::update_from_widget` places item rows at integer
multiples of `item_height` relative to `list_origin_y`:

```rust
let y = geometry.list_origin_y + i as f32 * geometry.item_height;
```

Because the scroll state (after the `selector_row_scroller` chunk) is a
fractional pixel offset, the integer `first_visible_item()` boundary may sit
partway through a physical row. Without subtracting `scroll_fraction_px()`, the
list jumps by a full row each time `first_visible_item()` increments — identical
to the choppy behaviour of the old integer `view_offset`.

The fix mirrors exactly what the main renderer does for the text viewport:
subtract the fractional remainder from the starting Y so the top item is
partially clipped by `scroll_fraction_px()` pixels, and include one extra item
in the render loop so the partially-visible bottom item is always drawn.

**Changes to `SelectorGlyphBuffer::update_from_widget`:**

1. Read `scroll_fraction_px` from the widget:
   ```rust
   let frac = widget.scroll_fraction_px();
   ```

2. Offset the list start Y by the fraction:
   ```rust
   let list_y = geometry.list_origin_y - frac;
   ```

3. Use `widget.visible_item_range(widget.items().len())` for the item loop
   bounds instead of `widget.view_offset()..widget.view_offset() +
   geometry.visible_items`. This delegates range calculation to `RowScroller`,
   which already adds the +1 extra row for partial bottom visibility.

4. Compute each item's Y from the loop index (not the absolute item index):
   ```rust
   for (draw_idx, item) in items[range].iter().enumerate() {
       let y = list_y + draw_idx as f32 * geometry.item_height;
       ...
   }
   ```

The selection highlight quad must receive the same treatment: compute its Y from
the visible offset of `selected_index` relative to `first_visible_item()`,
offset by `list_y`, so the highlight tracks the item as it glides.

No changes to `calculate_overlay_geometry`, `OverlayGeometry`, or the panel
background/separator rendering — only the item list and selection highlight
positioning.

## Success Criteria

- Trackpad scrolling in the file picker produces smooth, continuous motion with
  no visible full-row snapping between scroll events.
- The selection highlight moves with its item as the list scrolls fractionally.
- The first visible item is partially clipped at the top when `scroll_fraction_px
  > 0`, matching the behaviour of the main buffer viewport.
- A partially-visible item is always drawn at the bottom of the list when the
  list is scrolled to a fractional position.
- `calculate_overlay_geometry` and `OverlayGeometry` are unchanged.
- All existing `SelectorGlyphBuffer` geometry tests pass; no rendering tests are
  broken.
