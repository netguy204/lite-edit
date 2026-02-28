<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk fixes two viewport stability bugs using targeted condition guards:

### Bug 1: Cold scrollback recapture (terminal jostle)

The root cause is that `check_scrollback_overflow()` fires on every
`poll_events()` call when `history_size() > hot_scrollback_limit`, but alacritty
doesn't actually remove lines from its grid. The overflow condition is
permanently true after the first overflow, causing the same lines to be
recaptured on each PTY event.

**Fix strategy**: Use `last_history_size` (which is already tracked but unused)
to detect when *new* scrollback has actually arrived since the last capture.
Only capture if `history_size > last_history_size + threshold`. This prevents
recapturing the same lines.

### Bug 2: Off-by-one in ensure_visible (buffer cursor jumps)

The root cause is an inconsistency between `visible_range()` (which uses
`visible_rows + 1` to account for the partial bottom row) and `ensure_visible()`
/ `ensure_visible_wrapped()` (which use `visible_rows` or `visible_lines`
without the +1). A cursor on the +1 row IS rendered but triggers scrolling.

**Fix strategy**: Add +1 to the boundary check in both `ensure_visible_with_margin()`
and `ensure_visible_wrapped()` so they match `visible_range()`'s semantics.

Both fixes follow the testing philosophy: write failing tests first, then
implement the minimal fix. The fixes are surgical guards that don't change
the structure of existing code.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS a bug
  fix in `ensure_visible_wrapped` and the related `ensure_visible_with_margin`
  in `RowScroller`. The fix brings these methods into alignment with
  `visible_range()`, which correctly handles the +1 partial row.

  The subsystem is DOCUMENTED, so no opportunistic refactoring is required.
  The fix adheres to Invariant 5: "`visible_range` includes a +1 row for partial
  visibility." The bug was a deviation from this invariant in the ensure_visible
  methods.

## Sequence

### Step 1: Write failing test for cold scrollback recapture bug

Add a test to `crates/terminal/src/terminal_buffer.rs` that verifies
`check_scrollback_overflow` does not increment `cold_line_count` when called
multiple times with the same `history_size()`.

Test setup:
1. Create a TerminalBuffer with a small `hot_scrollback_limit` (e.g., 50)
2. Use `feed_bytes()` to populate > 50 lines of scrollback
3. Manually call `check_scrollback_overflow()` (make it pub(crate) for testing)
4. Record `cold_line_count` after first call
5. Call `check_scrollback_overflow()` again without adding new content
6. Assert `cold_line_count` is unchanged

This test will fail because the current code recaptures on every call.

Location: `crates/terminal/src/terminal_buffer.rs` (tests module)

### Step 2: Fix check_scrollback_overflow guard condition

Modify `check_scrollback_overflow()` to only capture when new scrollback has
arrived since the last capture. Change the condition from:

```rust
if history_size <= self.hot_scrollback_limit {
    self.last_history_size = history_size;
    return;
}
```

To:

```rust
// Only capture if history has grown since last capture
// This prevents re-capturing the same lines when history_size > limit
// but no new output has arrived (alacritty keeps old lines in grid)
if history_size <= self.last_history_size {
    return;
}
```

The key insight: `last_history_size` tracks the high-water mark of seen
history, not just the most recent value. Once we've captured up to a certain
history_size, we don't need to capture again until history_size increases.

Add chunk backreference comment above the method.

Location: `crates/terminal/src/terminal_buffer.rs:647`

### Step 3: Verify cold scrollback test passes

Run the test from Step 1. It should now pass because the guard prevents
recapture when history hasn't grown.

### Step 4: Write failing test for ensure_visible off-by-one

Add a test to `crates/editor/src/row_scroller.rs` that verifies `ensure_visible`
does NOT scroll when the target row equals `visible_rows` (the +1 partial row).

Test setup:
1. Create RowScroller with 10 visible rows (0..9 fully visible, row 10 partial)
2. Start at scroll position 0
3. Call `ensure_visible(10, 100)` — row 10 is the partial row
4. Assert no scrolling occurred (return value is `false`)

This test will fail because the current code scrolls when `row >= first_row + visible_rows`.

Location: `crates/editor/src/row_scroller.rs` (tests module)

### Step 5: Fix RowScroller::ensure_visible_with_margin boundary

Modify `ensure_visible_with_margin()` to include the +1 row in the visible
region. Change:

```rust
} else if row >= first_row + effective_visible {
```

To:

```rust
// +1 accounts for partially visible bottom row (matching visible_range semantics)
// Subsystem: docs/subsystems/viewport_scroll - Invariant 5
} else if row > first_row + effective_visible {
```

The change from `>=` to `>` means row `first_row + effective_visible` is now
considered visible (it's the partial row).

Location: `crates/editor/src/row_scroller.rs:206`

### Step 6: Verify RowScroller test passes and existing tests still pass

Run the new test from Step 4 and all existing `ensure_visible` tests.

### Step 7: Write failing test for ensure_visible_wrapped off-by-one

Add a test to verify the same fix is needed in `Viewport::ensure_visible_wrapped`.
This method has the same bug but in wrapped-line context.

Test setup:
1. Create Viewport with 10 visible lines
2. Set up a buffer with unwrapped lines (1 screen row = 1 buffer line for simplicity)
3. Position cursor on screen row `visible_lines` (the +1 partial row)
4. Call `ensure_visible_wrapped()`
5. Assert no scrolling occurred

Location: `crates/editor/src/viewport.rs` (tests module)

### Step 8: Fix Viewport::ensure_visible_wrapped boundary

Modify `ensure_visible_wrapped()` to include the +1 row. Change:

```rust
if cursor_screen_row >= visible_lines {
```

To:

```rust
// +1 accounts for partially visible bottom row (matching visible_range semantics)
// Chunk: docs/chunks/viewport_keystroke_jostle - Fix off-by-one
if cursor_screen_row > visible_lines {
```

Location: `crates/editor/src/viewport.rs:348`

### Step 9: Verify all viewport tests pass

Run all tests in `crates/editor/` to ensure the fix doesn't break existing
behavior. Key tests:
- `test_ensure_visible_scroll_down` — should still scroll when row is truly
  beyond the partial row
- `test_ensure_visible_at_boundary` — may need updating if it asserts row 10
  causes scrolling (it should not, now)
- All `visible_range` tests — should be unchanged

### Step 10: Run full test suite and verify no regressions

```bash
cargo test --workspace
```

Ensure all existing tests pass, including viewport scrolling, terminal
scrollback, and dirty region tests.

## Dependencies

No external dependencies. This chunk builds on existing code:
- `terminal_file_backed_scrollback` chunk (introduced `check_scrollback_overflow`,
  `cold_line_count`, `last_history_size`)
- `viewport_scroll` subsystem (`RowScroller`, `Viewport`, `visible_range`,
  `ensure_visible_*`)

## Risks and Open Questions

1. **Existing test assumptions**: The test `test_ensure_visible_at_boundary`
   asserts that row 10 (the first row beyond `visible_rows=10`) causes scrolling.
   After the fix, row 10 should NOT cause scrolling (it's the +1 partial row).
   Row 11 should be the first row that causes scrolling. This test may need
   updating — verify intent before modifying.

2. **Interaction with bottom_margin_rows**: `ensure_visible_with_margin` subtracts
   `bottom_margin_rows` from `visible_rows` to create `effective_visible`. Need
   to verify the +1 fix interacts correctly when margin is non-zero. If margin
   is 2 and visible_rows is 10, effective_visible is 8, and the partial row
   should be row 8 (screen row 9), not row 10.

3. **Terminal scrollback initial state**: Need to verify that `last_history_size`
   is initialized correctly. If it starts at 0, the first overflow after fresh
   terminal creation should work. If it starts at some other value, there may
   be edge cases.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?

Minor deviations (renamed a function, used a different helper) don't need
documentation. Significant deviations (changed the approach, skipped a step,
added steps) do.

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->