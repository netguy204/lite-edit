---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/terminal/src/terminal_buffer.rs
- crates/editor/src/row_scroller.rs
- crates/editor/src/viewport.rs
code_references:
- ref: crates/terminal/src/terminal_buffer.rs#TerminalBuffer::check_scrollback_overflow
  implements: "Fix terminal jostle bug - guard against recapturing same lines when history hasn't grown"
- ref: crates/editor/src/row_scroller.rs#RowScroller::ensure_visible_with_margin
  implements: "Fix off-by-one in boundary check - change >= to > to include partial row"
- ref: crates/editor/src/viewport.rs#Viewport::ensure_visible_wrapped
  implements: "Fix off-by-one in boundary check - change >= to > to include partial row"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- terminal_unicode_env
- incremental_parse
- tab_rendering
---

# Chunk Goal

## Minor Goal

Eliminate two viewport stability bugs that cause visible content shifting
during normal editing and terminal use, violating the project's low-latency
rendering goal (GOAL.md) and undermining smooth scrolling work delivered by
`viewport_fractional_scroll` and `pane_scroll_isolation`.

### Observed symptoms

1. **Terminal jostle**: ~1 in 3 terminal tabs develop a state where every
   keystroke causes the viewport to shift by ~1 line. Only terminals with
   sufficient scrollback history are affected. More noticeable in split panes.

2. **Buffer cursor jumps**: In a text buffer, moving the cursor in a way that
   should keep it visible (e.g., horizontal movement on the last rendered row)
   causes the viewport to jump as if the cursor were off-screen.

### Bug 1: Cold scrollback recapture inflates `line_count()`

`check_scrollback_overflow()` (`terminal_buffer.rs:647`) fires on every
`poll_events()` call when `history_size() > hot_scrollback_limit`. It copies
the oldest `lines_over_limit` lines to cold storage and increments
`cold_line_count`. **But it cannot remove lines from alacritty's grid**, so
`history_size()` never decreases and the condition remains true on the next
poll.

Every keystroke that produces PTY output (even a single echo character)
triggers `poll_events()` → `check_scrollback_overflow()`, which recaptures
the same lines and inflates `cold_line_count` by `lines_over_limit`. Since
`line_count() = cold_line_count + history_size() + screen_lines()`, the total
grows each time. The keystroke handler then calls
`scroll_to_bottom(line_count)`, which adjusts the viewport for the phantom
growth, shifting content by `lines_over_limit` lines.

**Why 1 in 3 terminals**: Only terminals that have produced > 2000 lines of
scrollback (`DEFAULT_HOT_SCROLLBACK_LIMIT`) trigger the overflow. Fresh or
lightly-used terminals stay under the limit.

**Key code path**:
- `terminal_buffer.rs:374` — `if processed_any` → `check_scrollback_overflow()`
- `terminal_buffer.rs:647` — `check_scrollback_overflow()`: condition
  `history_size > hot_scrollback_limit` is permanently true after first overflow
- `terminal_buffer.rs:728` — `cold_line_count += actual_count` (unbounded growth)
- `terminal_buffer.rs:874` — `line_count()` returns inflated sum
- `editor_state.rs:2192` — keystroke handler calls `scroll_to_bottom(line_count)`

**Missing guard**: `last_history_size` is tracked but never used in the
overflow condition. The fix should use it (or `cold_line_count`) to skip
recapture when no new lines have entered the hot scrollback since the
previous capture.

### Bug 2: `ensure_visible` off-by-one with partial row

`visible_range()` (`row_scroller.rs:146`) returns `first_row..(first_row +
visible_rows + 1)`, rendering `visible_rows + 1` rows to handle the
partially-visible bottom row when at a fractional scroll position. But
`ensure_visible_wrapped()` (`viewport.rs:348`) uses `visible_lines` as its
boundary:

```
if cursor_screen_row >= visible_lines {  // scrolls!
```

A cursor on screen row `visible_lines` (the +1 row) IS rendered and visible,
but `ensure_visible` considers it off-screen and scrolls. Similarly,
`ensure_visible_with_margin()` (`row_scroller.rs:206`) checks
`row >= first_row + effective_visible` which has the same off-by-one.

**Why more common in splits**: Splitting a window whose height divides evenly
by `line_height` creates panes with a fractional row, increasing the chance
the cursor lands on the +1 row.

**Key code path**:
- `row_scroller.rs:151` — `visible_range` uses `visible_rows + 1`
- `row_scroller.rs:206` — `ensure_visible` uses `visible_rows` (no +1)
- `viewport.rs:348` — `ensure_visible_wrapped` uses `visible_lines` (no +1)
- `context.rs:152` — `ensure_cursor_visible()` called after every cursor move
- `buffer_target.rs:345-398` — arrow keys, hjkl, word movement all call it

## Success Criteria

- Terminal tabs that have accumulated > 2000 lines of scrollback do not jostle
  on keystroke; `cold_line_count` is stable when no new output arrives
- A test verifies that `check_scrollback_overflow` does not increment
  `cold_line_count` on repeated calls when `history_size()` hasn't changed
- Moving the cursor to the last rendered row in a text buffer does not trigger
  viewport scrolling when the row is visible on screen
- A test verifies that `ensure_visible` does not scroll when the target row is
  within the `visible_range` (accounting for the +1 partial row)
- Existing viewport tests pass (scroll clamping, visible_range, fractional
  scroll, pane isolation)