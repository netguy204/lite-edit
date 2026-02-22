---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/buffer_target.rs
- crates/editor/src/context.rs
code_references:
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position
    implements: "Scroll fraction compensation in non-wrapped click-to-cursor mapping"
  - ref: crates/editor/src/buffer_target.rs#pixel_to_buffer_position_wrapped
    implements: "Scroll fraction compensation in wrap-aware click-to-cursor mapping"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- scroll_max_last_line
- tab_click_cursor_placement
---

# Chunk Goal

## Minor Goal

Fix a bug where clicking to position the cursor near the bottom of a large file
places the cursor approximately two lines below the clicked position.

### Symptom

After scrolling to or near the bottom of a large file, clicking in the editor
to position the cursor results in the cursor appearing roughly two lines below
(further down in the document than) the line that was visually clicked.

### Root Cause

The click-to-cursor mapping in `pixel_to_buffer_position_wrapped`
(`crates/editor/src/buffer_target.rs`) computes the target screen row as:

```rust
let target_screen_row = (flipped_y / line_height).floor() as usize;
```

It then walks buffer lines from `first_visible_line` with
`cumulative_screen_row = 0`, treating the top of the viewport as exactly the
start of `first_visible_line`.

However, the renderer applies `viewport.scroll_fraction_px()` as a Y translation
to all content (`-scroll_fraction_px` upward), which partially clips the topmost
visible line. When `scroll_fraction_px > 0`, the visual start of each line on
screen is shifted relative to where the click math assumes it is. The hit-test
function must account for this fractional offset when computing `target_screen_row`.

The corrected formula is:

```rust
let target_screen_row =
    ((flipped_y + scroll_fraction_px as f64) / line_height).floor() as usize;
```

This same mismatch likely exists in the non-wrapped `pixel_to_buffer_position`
fallback and should be fixed there too.

The bug is most noticeable near the bottom of large files because smooth scrolling
tends to leave a non-zero `scroll_fraction_px` at typical resting positions, and
the discrepancy accumulates across the line-walking loop in the wrapped path.

## Success Criteria

- Clicking on a line when the viewport has a non-zero `scroll_fraction_px`
  positions the cursor on the visually-clicked line, not an offset line.
- The fix is applied to both `pixel_to_buffer_position_wrapped` and the legacy
  non-wrapped `pixel_to_buffer_position`.
- `scroll_fraction_px` is threaded through the call site so the click handler
  has access to it; the click handler lives in `buffer_target.rs` which goes
  through `EditorContext` — confirm `EditorContext` or its call site can supply
  the fraction.
- A regression test is added in `buffer_target.rs`: simulate a scroll to a
  fractional position (`scroll_fraction_px > 0`), then verify that a click at
  the middle of a visually-rendered line maps to the correct buffer line, not
  an adjacent one.
- Existing click-positioning tests continue to pass.
- After the fix, clicking anywhere in the file (top, middle, bottom) reliably
  places the cursor on the intended line regardless of the current
  `scroll_fraction_px`.