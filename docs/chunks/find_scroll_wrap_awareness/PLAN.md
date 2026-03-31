

<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix threads through two layers:

1. **`viewport.rs`**: Add `ensure_visible_wrapped_with_margin` — a variant of the existing
   `ensure_visible_wrapped` that accepts a `bottom_margin_rows: usize` parameter. The margin
   reduces the effective visible height for the "is the target below the viewport?" check,
   matching the semantics of the non-wrapped `ensure_visible_with_margin` on `RowScroller`.
   Refactor `ensure_visible_wrapped` to delegate to the new method with `margin = 0`.

2. **`editor_state.rs`**: Replace the `ensure_visible_with_margin(match_line, line_count, 1)`
   call in `run_live_search()` with `ensure_visible_wrapped_with_margin(…)`. To construct the
   needed `WrapLayout`, use `self.view_width - RAIL_WIDTH` and `self.font_metrics` (the same
   fields used by the click-handling path). Collect line lengths into a `Vec<usize>` before
   calling `viewport_mut()` to satisfy the borrow checker.

No new data structures or fields are required. The change is purely a call-site swap at the
scrolling layer.

Following the project's TDD discipline, tests are written first (as failing tests) and
implementation follows.

## Subsystem Considerations

**`docs/subsystems/viewport_scroll`** (DOCUMENTED): This chunk IMPLEMENTS a new method on
`Viewport` (`ensure_visible_wrapped_with_margin`) that extends the subsystem's cursor-following
scroll with margin support. The new method follows the subsystem's established pattern:
compute absolute screen rows by iterating over buffer lines, then apply clamping. No known
deviations are introduced.

## Sequence

### Step 1: Write failing tests for `ensure_visible_wrapped_with_margin` (viewport.rs)

Add a `#[cfg(test)]` block in `viewport.rs` with the following tests.  Each test should fail
initially because the method doesn't exist yet.

Tests to write (all in the `viewport.rs` `#[cfg(test)]` block, below the existing wrapped-scroll
tests):

```
// Helper: 8px glyph width, 16px line height, 80px viewport → 10 cols/row
fn wrapped_test_metrics() -> FontMetrics { ... }

test_ensure_visible_wrapped_with_margin_margin0_same_as_no_margin
  // With margin=0 the result should equal ensure_visible_wrapped.

test_ensure_visible_wrapped_with_margin_match_below_scrolls
  // Setup: 4 lines, first 2 lines each wrap to 2 screen rows (total 4 screen rows before
  //   the match). Viewport shows 5 rows. Match is on line 2, col 0.
  // With margin=1, effective visible = 4. With scroll=0, match row 4 > 0 + 4.
  // Expect: scrolled=true, viewport moves.

test_ensure_visible_wrapped_with_margin_match_visible_no_scroll
  // Same setup but viewport already scrolled to show the match row within the effective
  // height. Expect: scrolled=false.

test_ensure_visible_wrapped_with_margin_match_above_scrolls_up
  // Viewport scrolled past the match. Expect: scrolled=true, offset moved up.
  // Margin does not affect upward scrolling (same as ensure_visible_with_margin).

test_ensure_visible_wrapped_with_margin_margin_shrinks_effective_window
  // Demonstrate that margin=1 causes scrolling 1 row earlier than margin=0.
  // Place the match row at exactly current_top + effective_visible (the partial row).
  // margin=0: no scroll (match is the partial row). margin=1: scroll (match is beyond
  // effective viewport). This directly mirrors the existing
  // test_ensure_visible_with_margin_scrolls_earlier test on RowScroller.
```

Location: `crates/editor/src/viewport.rs`, `#[cfg(test)]` module, at the end.

### Step 2: Implement `ensure_visible_wrapped_with_margin` (viewport.rs)

Add the new method to `impl Viewport`. Refactor `ensure_visible_wrapped` to delegate to it:

```rust
// Chunk: docs/chunks/find_scroll_wrap_awareness - Wrap-aware find-match scroll with margin
/// Like `ensure_visible_wrapped`, but treats the viewport as if it had
/// `bottom_margin_rows` fewer rows at the bottom.
///
/// Used by find-in-file scrolling when the find strip occludes the last visible row.
pub fn ensure_visible_wrapped_with_margin<F>(
    &mut self,
    target_line: usize,
    target_col: usize,
    line_count: usize,
    wrap_layout: &crate::wrap_layout::WrapLayout,
    bottom_margin_rows: usize,
    line_len_fn: F,
) -> bool
where
    F: Fn(usize) -> usize,
```

The implementation mirrors `ensure_visible_wrapped` but replaces the "below viewport" threshold
with an effective visible row count:

```
let effective_visible = visible_lines.saturating_sub(bottom_margin_rows).max(1);

// Upward scroll: unaffected by margin (same as ensure_visible_with_margin)
if target_abs_screen_row < current_top_screen_row { ... }
// Downward scroll: use effective_visible instead of visible_lines
else if target_abs_screen_row > current_top_screen_row + effective_visible {
    let new_top_row = target_abs_screen_row.saturating_sub(effective_visible.saturating_sub(1));
    ...
}
```

Then update `ensure_visible_wrapped` to be a zero-margin delegate:

```rust
pub fn ensure_visible_wrapped<F>(...) -> bool {
    self.ensure_visible_wrapped_with_margin(
        cursor_line, cursor_col, line_count, wrap_layout, 0, line_len_fn,
    )
}
```

Location: `crates/editor/src/viewport.rs`, `impl Viewport`.

Run tests after this step — all five new tests should now pass.

### Step 3: Write a failing integration test for find-scroll-wrap in `editor_state.rs`

Add a test inside `editor_state.rs`'s `#[cfg(test)]` module (or find the existing test module
for find behavior) that sets up an `EditorState` with:

- A narrow viewport (e.g. 10 visible rows of 16px, with 8px glyph width and a very narrow
  `view_width` so lines wrap)
- Buffer content where lines 0–3 each wrap to 2 screen rows, and the target keyword is on
  buffer line 4
- `viewport` initially scrolled to offset 0

Then simulate opening find mode (push `FindFocusTarget` or call the key sequence that opens it)
and typing the search query. Verify that after `run_live_search()` the viewport's
`scroll_offset_px()` is non-zero (i.e., the match on the far-down buffer line is now visible).

The test should fail before Step 4 because `run_live_search` still uses
`ensure_visible_with_margin` (which underestimates scroll when wrapping).

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module.

### Step 4: Update `run_live_search` in `editor_state.rs`

Replace the existing scroll call in `run_live_search()`:

```rust
// Before
let line_count = self.buffer().line_count();
let match_line = start.line;
if self.viewport_mut().ensure_visible_with_margin(match_line, line_count, 1) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

With:

```rust
// After
// Chunk: docs/chunks/find_scroll_wrap_awareness - Use wrap-aware scroll for find matches
use crate::wrap_layout::WrapLayout;

let line_count = self.buffer().line_count();
let match_line = start.line;
let match_col = start.col;

// Pre-collect line lengths to satisfy borrow checker (buffer() and viewport_mut()
// cannot coexist as borrows of self).
let line_lens: Vec<usize> = (0..line_count)
    .map(|i| self.buffer().line_len(i))
    .collect();

let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);

if self.viewport_mut().ensure_visible_wrapped_with_margin(
    match_line,
    match_col,
    line_count,
    &wrap_layout,
    1, // margin=1: find strip occludes the last visible row
    |i| line_lens.get(i).copied().unwrap_or(0),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

The `RAIL_WIDTH` constant is already imported at the top of `editor_state.rs`.

Run tests — the Step 3 integration test should now pass.

### Step 5: Update `code_paths` in GOAL.md

Update the `code_paths` field in `docs/chunks/find_scroll_wrap_awareness/GOAL.md`:

```yaml
code_paths:
  - crates/editor/src/viewport.rs
  - crates/editor/src/editor_state.rs
```

## Subsystem Invariant Notes

The subsystem's **Soft Convention 1** states: "Prefer `set_scroll_offset_px_wrapped` over
`set_scroll_offset_px` when wrapping is enabled." The new `ensure_visible_wrapped_with_margin`
internally uses `set_scroll_offset_px_direct` (unclamped) with its own max computation, matching
the existing pattern in `ensure_visible_wrapped`. No deviation from the subsystem's conventions.

## Risks and Open Questions

- **Tab characters in match position**: `ensure_visible_wrapped_with_margin` uses `target_col`
  as a visual column (via `buffer_col_to_screen_pos`), not a character column. For lines with
  tabs, the visual column may differ from the character index stored in `start.col`. The
  existing `ensure_visible_wrapped` has the same approximation. For the scope of this fix,
  this is acceptable; a separate tab-exact find-scroll chunk can address it if needed.

- **Multi-pane width**: `self.view_width - RAIL_WIDTH` is the full editor width minus the left
  rail. In a multi-pane layout, the active pane is narrower. This means `WrapLayout.cols_per_row`
  may overestimate available columns, causing the scroll position to be slightly off for panes
  narrower than the full editor width. The match will still be visible — just possibly not
  positioned at the ideal row offset. Fixing this requires passing pane geometry into
  `run_live_search`, which is out of scope here.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
