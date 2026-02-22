<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is that `pixel_to_buffer_position` and `pixel_to_buffer_position_wrapped` compute
target screen row without accounting for the renderer's `scroll_fraction_px` vertical
offset. The renderer translates all content by `-scroll_fraction_px`, but the hit-test
math assumes row 0 starts at pixel 0.

**Pattern to follow**: The `selector.rs` module already handles this correctly in
`handle_mouse`:

```rust
let frac = self.scroll.scroll_fraction_px() as f64;
let relative_y = position.1 - list_origin_y + frac;
let row = (relative_y / item_height).floor() as usize;
```

We apply the same fix: add `scroll_fraction_px` to `flipped_y` before dividing by
`line_height`. This compensates for the visual shift the renderer applies.

**Implementation strategy**:
1. Add `scroll_fraction_px: f32` as a new parameter to both hit-test functions
2. Adjust `target_screen_row` calculation: `((flipped_y + scroll_fraction_px) / line_height).floor()`
3. Update call sites in `handle_mouse` to pass `ctx.viewport.scroll_fraction_px()`
4. Add regression tests that verify clicking with non-zero `scroll_fraction_px`

The fix is minimal and localized to `buffer_target.rs`.

## Subsystem Considerations

No subsystems exist in this project yet. This chunk does not warrant creating one.

## Sequence

### Step 1: Add scroll_fraction_px parameter to pixel_to_buffer_position

**Location**: `crates/editor/src/buffer_target.rs`

Update the non-wrapped `pixel_to_buffer_position` function:

1. Add a new parameter `scroll_fraction_px: f32` after `view_height`
2. Update the screen line calculation:

```rust
// Before:
let screen_line = if flipped_y >= 0.0 && line_height > 0.0 {
    (flipped_y / line_height).floor() as usize
} else {
    0
};

// After:
let screen_line = if flipped_y >= 0.0 && line_height > 0.0 {
    ((flipped_y + scroll_fraction_px as f64) / line_height).floor() as usize
} else {
    0
};
```

3. Update the function's doc comment to explain that `scroll_fraction_px` compensates
   for the renderer's vertical translation.

This function is a legacy fallback; adding the parameter keeps its signature in sync
with the wrapped version.

### Step 2: Add scroll_fraction_px parameter to pixel_to_buffer_position_wrapped

**Location**: `crates/editor/src/buffer_target.rs`

Update the wrap-aware `pixel_to_buffer_position_wrapped` function:

1. Add a new parameter `scroll_fraction_px: f32` after `wrap_layout`
2. Update the target screen row calculation:

```rust
// Before:
let target_screen_row = if flipped_y >= 0.0 && line_height > 0.0 {
    (flipped_y / line_height as f64).floor() as usize
} else {
    0
};

// After:
let target_screen_row = if flipped_y >= 0.0 && line_height > 0.0 {
    ((flipped_y + scroll_fraction_px as f64) / line_height as f64).floor() as usize
} else {
    0
};
```

3. Update the function's doc comment to explain the scroll fraction compensation.

### Step 3: Update call sites in handle_mouse

**Location**: `crates/editor/src/buffer_target.rs`, `BufferFocusTarget::handle_mouse`

There are two calls to `pixel_to_buffer_position_wrapped` in `handle_mouse`:
1. In the `MouseEventKind::Down` arm (line ~472)
2. In the `MouseEventKind::Moved` arm (line ~501)

Update both calls to pass `ctx.viewport.scroll_fraction_px()`:

```rust
// Before:
let position = pixel_to_buffer_position_wrapped(
    event.position,
    ctx.view_height,
    &wrap_layout,
    ctx.viewport.first_visible_line(),
    ctx.buffer.line_count(),
    |line| ctx.buffer.line_len(line),
);

// After:
let position = pixel_to_buffer_position_wrapped(
    event.position,
    ctx.view_height,
    &wrap_layout,
    ctx.viewport.scroll_fraction_px(),  // NEW
    ctx.viewport.first_visible_line(),
    ctx.buffer.line_count(),
    |line| ctx.buffer.line_len(line),
);
```

### Step 4: Update existing unit tests for pixel_to_buffer_position

**Location**: `crates/editor/src/buffer_target.rs`, `mod tests`

The existing tests for `pixel_to_buffer_position` pass `0.0` as `scroll_fraction_px`
since they don't involve scrolling. Update each call to include the new parameter:

- `test_pixel_to_position_line_0` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_line_1` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_column` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_past_line_end` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_past_buffer_end` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_with_scroll` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_empty_buffer` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_negative_x` → add `0.0` for scroll_fraction_px
- `test_pixel_to_position_fractional_x` → add `0.0` for scroll_fraction_px

### Step 5: Add regression test for fractional scroll click

**Location**: `crates/editor/src/buffer_target.rs`, `mod tests`

Add a new test `test_click_with_scroll_fraction_positions_correctly` that verifies
the bug is fixed:

```rust
// Chunk: docs/chunks/click_scroll_fraction_alignment - Regression test
#[test]
fn test_click_with_scroll_fraction_positions_correctly() {
    // Setup: scroll to a fractional position (line 5 + 8 pixels)
    // scroll_fraction_px = 8.0, line_height = 16.0
    //
    // Renderer places line 5 at y_visual = -scroll_fraction_px = -8
    // (partially clipped off the top). Line 6 is at y_visual = 8, etc.
    //
    // A click at screen y = 155 (macOS coords, bottom-left origin) where
    // view_height = 160 gives:
    //   flipped_y = 160 - 155 = 5 (5 pixels from top)
    //
    // WITHOUT scroll_fraction_px compensation:
    //   target_row = floor(5 / 16) = 0 → would map to line 5 (first_visible)
    //
    // WITH scroll_fraction_px = 8 compensation:
    //   target_row = floor((5 + 8) / 16) = floor(0.8125) = 0 → still line 5
    //
    // But consider y = 150 (flipped_y = 10):
    // WITHOUT: floor(10/16) = 0 → line 5
    // WITH 8px: floor((10+8)/16) = floor(1.125) = 1 → line 6 (CORRECT)
    //
    // The visual center of line 5 (first visible) is at flipped_y = -8 + 8 = 0
    // The visual center of line 6 is at flipped_y = 8 + 8 = 16
    //
    // Clicking at flipped_y = 10 is visually in line 6's region, so we
    // expect buffer line 6.

    let content = (0..20).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let mut buffer = TextBuffer::from_str(&content);
    let mut viewport = Viewport::new(16.0);
    viewport.update_size(160.0, 100);
    let mut dirty = DirtyRegion::None;
    let mut target = BufferFocusTarget::new();

    // Scroll to line 5 + 8 pixels (fractional)
    viewport.set_scroll_offset_px(5.0 * 16.0 + 8.0, buffer.line_count());
    assert_eq!(viewport.first_visible_line(), 5);
    assert!((viewport.scroll_fraction_px() - 8.0).abs() < 0.001);

    {
        let mut ctx = EditorContext::new(
            &mut buffer,
            &mut viewport,
            &mut dirty,
            test_font_metrics(),
            160.0,
            800.0,
        );
        // Click at flipped_y = 10 (y = 150 in macOS coords)
        // This is visually in line 6's row (line 5 is partially clipped at top)
        let event = MouseEvent {
            kind: MouseEventKind::Down,
            position: (0.0, 150.0), // flipped_y = 10
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        target.handle_mouse(event, &mut ctx);
    }

    // Should select line 6, NOT line 5
    assert_eq!(
        buffer.cursor_position().line,
        6,
        "With scroll_fraction_px=8, clicking at flipped_y=10 should select line 6"
    );
}
```

### Step 6: Add unit test for pixel_to_buffer_position with scroll_fraction

**Location**: `crates/editor/src/buffer_target.rs`, `mod tests`

Add a unit test that directly tests the non-wrapped function with a non-zero
scroll_fraction_px:

```rust
// Chunk: docs/chunks/click_scroll_fraction_alignment - Unit test for scroll fraction
#[test]
fn test_pixel_to_position_with_scroll_fraction() {
    // line_height = 16, scroll_fraction_px = 8
    // flipped_y = 10, without fraction: line 0, with fraction: line 1
    let metrics = test_font_metrics();
    let position = super::pixel_to_buffer_position(
        (0.0, 150.0), // flipped_y = 160 - 150 = 10
        160.0,
        &metrics,
        8.0, // scroll_fraction_px
        0,   // scroll_offset (first_visible_line)
        5,   // line_count
        |_| 10,
    );
    // (10 + 8) / 16 = 1.125 → line 1
    assert_eq!(position.line, 1);
}
```

### Step 7: Run tests and verify

Run `cargo test -p editor` to verify:
1. All existing tests still pass (with the added scroll_fraction_px=0 parameter)
2. The new regression tests pass
3. No other tests are affected

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments to help future agents trace
code back to its governing documentation.

Add a chunk backreference near the modified calculation in each function:
```rust
// Chunk: docs/chunks/click_scroll_fraction_alignment - Account for renderer Y offset
```

## Dependencies

None. The required infrastructure (`viewport.scroll_fraction_px()`) already exists
from the `viewport_fractional_scroll` and `selector_row_scroller` chunks.

## Risks and Open Questions

- **Parameter ordering**: The new `scroll_fraction_px` parameter is inserted after
  `view_height` but before `wrap_layout` / `font_metrics`. This ordering groups the
  view-related parameters together. If this feels awkward, consider a struct for
  view parameters, but that's scope creep for this bug fix.

- **Negative flipped_y with fraction**: If `flipped_y + scroll_fraction_px` is still
  negative (click above viewport), we clamp to line 0. This matches current behavior.

## Deviations

<!-- Populate during implementation. -->