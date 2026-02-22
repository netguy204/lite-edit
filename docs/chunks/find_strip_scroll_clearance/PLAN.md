# Implementation Plan

## Approach

This chunk fixes two compounding issues that prevent find-and-scroll from
landing matches in the visible area:

1. **Tab bar height not subtracted from viewport calculation**:
   `EditorState::update_viewport_dimensions` passes the raw `window_height` to
   `Viewport::update_size`, but the content area is actually `window_height -
   TAB_BAR_HEIGHT` pixels. This causes `visible_lines` to be over-counted by
   `floor(32 / 16) = 2` lines with typical metrics.

2. **Find strip occludes the last visible line**: When scrolling to reveal a
   match, `ensure_visible` places the target at `visible_lines - 1` (the last
   visible row). The find strip (~24px) renders over this row, hiding the match.

**Strategy:**

- Fix #1 by passing `window_height - TAB_BAR_HEIGHT` (i.e., `content_height`) to
  `Viewport::update_size` in `update_viewport_dimensions`. This value is already
  computed elsewhere in `editor_state.rs` for mouse/scroll handling.

- Fix #2 by introducing a `ensure_visible_with_margin` helper on `Viewport` that
  accepts a `bottom_margin_lines` parameter, then calling it from
  `run_live_search` / `advance_to_next_match` with margin=1 when find mode is
  active. This keeps the generic `ensure_visible` unchanged for normal cursor
  scrolling.

**Testing Philosophy alignment:**

- Per TESTING_PHILOSOPHY.md, we test pure state manipulation without platform
  dependencies. The viewport and scroll logic are already fully testable.
- We write failing tests first for both fixes before implementing the changes.

## Sequence

### Step 1: Add failing test for visible_lines overcount

Write a test in `editor_state.rs` that creates an `EditorState`, calls
`update_viewport_dimensions(800.0, 600.0)`, and asserts that `visible_lines`
equals `floor((600 - 32) / 16) = 35` rather than `floor(600 / 16) = 37`.

Location: `crates/editor/src/editor_state.rs` (test module)

Expected: Test fails because current code computes 37 lines.

### Step 2: Fix update_viewport_dimensions to subtract TAB_BAR_HEIGHT

In `EditorState::update_viewport_dimensions`, change the call from:

```rust
self.viewport_mut().update_size(window_height, line_count);
```

to:

```rust
let content_height = window_height - TAB_BAR_HEIGHT;
self.viewport_mut().update_size(content_height, line_count);
```

This mirrors how `content_height` is already computed for mouse event handling
in the same file (lines ~851, ~1013, ~1057).

Also update `update_viewport_size` for consistency (line ~244-248).

Location: `crates/editor/src/editor_state.rs`

Verify: The test from Step 1 now passes.

### Step 3: Add ensure_visible_with_margin method to Viewport

Add a new method to `Viewport`:

```rust
/// Ensures a buffer line is visible, with additional bottom margin.
///
/// Like `ensure_visible`, but treats the viewport as if it had
/// `bottom_margin_lines` fewer rows at the bottom. This is useful when
/// an overlay (like the find strip) occludes the bottom of the viewport.
///
/// Returns `true` if scrolling occurred.
pub fn ensure_visible_with_margin(
    &mut self,
    line: usize,
    buffer_line_count: usize,
    bottom_margin_lines: usize,
) -> bool
```

The implementation will temporarily reduce `visible_lines` by the margin when
computing whether scrolling is needed (or delegate to `RowScroller` with
adjusted parameters).

Location: `crates/editor/src/viewport.rs`

### Step 4: Add unit tests for ensure_visible_with_margin

Write tests verifying:
- `ensure_visible_with_margin(line, count, 0)` behaves identically to
  `ensure_visible(line, count)`
- `ensure_visible_with_margin(target, count, 1)` scrolls such that target ends
  up at `visible_lines - 2` (one row above the margin)
- Already-visible lines with margin still don't trigger scrolling

Location: `crates/editor/src/viewport.rs` (test module)

### Step 5: Add integration test for find scroll clearance

Write a test in `editor_state.rs` that:
1. Creates a buffer with 100 lines
2. Sets viewport dimensions such that ~10 lines are visible
3. Opens find mode (set `focus` to `FindInFile`, init `find_mini_buffer`)
4. Runs `run_live_search` for a query matching a line near the bottom
5. Asserts the match line is at or above `first_visible_line + visible_lines - 2`
   (i.e., above the find strip area)

Location: `crates/editor/src/editor_state.rs` (test module)

Expected: Test fails because current `run_live_search` uses `ensure_visible`
without margin.

### Step 6: Update run_live_search to use ensure_visible_with_margin

In `EditorState::run_live_search`, change:

```rust
if self.viewport_mut().ensure_visible(match_line, line_count) {
```

to:

```rust
// When find mode is active, use margin=1 to keep the match above the find strip
if self.viewport_mut().ensure_visible_with_margin(match_line, line_count, 1) {
```

Location: `crates/editor/src/editor_state.rs`

Note: `run_live_search` is only called when find mode is active, so we can
unconditionally apply the margin.

### Step 7: Verify all tests pass

Run the full test suite:

```bash
cargo test -p lite-edit-editor
```

All tests should pass, including the new tests from Steps 1, 4, and 5.

### Step 8: Manual verification

Open the editor, load a file with content extending past one viewport, press
Ctrl+F, and type a query that matches a line near the bottom of the file. Verify
the match is clearly visible above the find strip without requiring manual
scrolling.

## Dependencies

- **find_in_file** chunk: This chunk builds on the find-in-file functionality
  implemented in that chunk. The `run_live_search` and `advance_to_next_match`
  methods must already exist.

## Risks and Open Questions

- **Line wrap interaction**: The chunk's success criteria mention that when find
  mode is *not* active, `ensure_visible` should behave exactly as before. The
  approach of introducing a separate method (`ensure_visible_with_margin`) and
  only calling it from find-mode code paths ensures this invariant. However, we
  should verify that `ensure_visible_wrapped` (used for soft-wrapped lines) is
  not affected by this change.

- **Margin calculation edge case**: With very small viewports (e.g., 2 visible
  lines), applying a 1-line margin may behave unexpectedly. Consider clamping
  the effective visible lines to at least 1.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->