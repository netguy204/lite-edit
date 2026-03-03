---
decision: APPROVE
summary: All success criteria satisfied; both viewport stability bugs fixed with tests and proper backreferences.
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Terminal tabs that have accumulated > 2000 lines of scrollback do not jostle on keystroke; `cold_line_count` is stable when no new output arrives

- **Status**: satisfied
- **Evidence**: `terminal_buffer.rs:667` - Guard condition `if history_size <= self.last_history_size { return 0; }` prevents re-capture when history hasn't grown. The fix uses `last_history_size` as a high-water mark to detect actual new scrollback, preventing the recapture loop that inflated `cold_line_count`.

### Criterion 2: A test verifies that `check_scrollback_overflow` does not increment `cold_line_count` on repeated calls when `history_size()` hasn't changed

- **Status**: satisfied
- **Evidence**: `terminal_buffer.rs:1774` - Test `test_check_scrollback_overflow_does_not_recapture_same_lines` creates a terminal with 100 lines exceeding the 50-line limit, calls `check_scrollback_overflow()` twice, and asserts `cold_line_count` is unchanged on the second call. Passes consistently.

### Criterion 3: Moving the cursor to the last rendered row in a text buffer does not trigger viewport scrolling when the row is visible on screen

- **Status**: satisfied
- **Evidence**:
  - `row_scroller.rs:206` - Changed `>=` to `>` in boundary check, making `first_row + effective_visible` (the +1 partial row) be considered visible.
  - `viewport.rs:354` - Same fix in `ensure_visible_wrapped()`, changing `if cursor_screen_row >= visible_lines` to `if cursor_screen_row > visible_lines`.

### Criterion 4: A test verifies that `ensure_visible` does not scroll when the target row is within the `visible_range` (accounting for the +1 partial row)

- **Status**: satisfied
- **Evidence**: Multiple tests verify this:
  - `row_scroller.rs:821` - `test_ensure_visible_partial_row_should_not_scroll` - Tests row 10 (partial row) does not scroll, row 11 does.
  - `row_scroller.rs:852` - `test_ensure_visible_with_margin_partial_row_should_not_scroll` - Tests with margin=1.
  - `viewport.rs:2558` - `test_ensure_visible_wrapped_partial_row_should_not_scroll` - Tests wrapped variant.
  - `selector.rs:520` - Selector widget tests updated with +1 partial row awareness.

### Criterion 5: Existing viewport tests pass (scroll clamping, visible_range, fractional scroll, pane isolation)

- **Status**: satisfied
- **Evidence**: All 80 viewport tests pass. All 49 row_scroller tests pass. Existing `test_ensure_visible_at_boundary` tests in both `row_scroller.rs:534` and `viewport.rs:944` have been updated to reflect correct +1 partial row behavior with chunk backreference comments.

## Notes

- The commit message mentions `captured_up_to_index` but implementation uses `last_history_size` - this is a minor documentation drift; the mechanism is correct.
- Flaky integration tests for shell spawning (`test_shell_prompt_appears`, etc.) are unrelated to this chunk.
- All subsystem invariants respected; Invariant 5 of `viewport_scroll` subsystem is now correctly implemented in both `ensure_visible_with_margin()` and `ensure_visible_wrapped()`.
