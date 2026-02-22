---
decision: APPROVE
summary: "Fix correctly removes premature dirty marking; PTY echo flow now drives rendering timing"
operator_review: null  # DO NOT SET - reserved for operator curation
---

## Criteria Assessment

### Criterion 1: Pasting text via Cmd+V into a focused terminal tab displays the pasted characters immediately (no blank spaces)

- **Status**: satisfied
- **Evidence**: The fix in `editor_state.rs:1140-1154` removes the premature `DirtyRegion::FullViewport` marking from the paste handler. The existing `poll_agents()` call in `main.rs:251` (called after every `handle_key`) processes the PTY echo and marks the correct lines dirty via `update_damage()`. This ensures rendering only happens after echoed content is available, eliminating the "blank spaces before echo arrives" bug.

### Criterion 2: Pasted multi-line text (e.g., a shell script) renders correctly line by line

- **Status**: satisfied
- **Evidence**: The fix doesn't change the PTY write path or how multi-line content is handled. Multi-line pastes are written to the PTY, the shell echoes them, and `poll_agents()` → `poll_events()` → `update_damage()` marks `FromLineToEnd(history_len)` which covers all affected viewport lines (as documented in PLAN.md). The PTY wakeup mechanism handles any output that arrives after the immediate poll.

### Criterion 3: Pasting does not break subsequent keyboard input

- **Status**: satisfied
- **Evidence**: The change is isolated to removing a single dirty region marking. The paste handler still writes to PTY correctly (`write_input(&bytes)`) and returns normally. The general keystroke handling path at `editor_state.rs:1169-1178` remains unchanged and continues to mark `FullViewport` dirty for normal input. No state is corrupted by the paste operation.

### Criterion 4: Existing single-character typing behavior is unaffected

- **Status**: satisfied
- **Evidence**: Single-character typing goes through the general key encoding path at `editor_state.rs:1159-1178`, which still marks `DirtyRegion::FullViewport` after sending input. The paste-specific early return at line 1153 only affects Cmd+V handling. The `terminal_input_render_bug` fix (chunk backreference at `main.rs:248-251`) ensures single characters are polled and rendered promptly.

### Criterion 5: The fix works for both short strings (a word) and longer pastes (a paragraph)

- **Status**: satisfied
- **Evidence**: The integration test `test_paste_content_appears_after_poll` in `input_integration.rs:301-353` verifies that "hello world" appears in the terminal buffer after polling. The test uses a 50-iteration polling loop with 20ms sleeps (total 1 second timeout), accommodating varying shell echo timing. The PLAN.md acknowledges that very large pastes may render incrementally, which is acceptable UX behavior consistent with other terminals.

## Subsystem Compliance

The implementation aligns with the `viewport_scroll` subsystem's design:
- `DirtyRegion` changes flow from buffer mutations (terminal grid updates via `update_damage()`), not from input handling
- The fix removes input-handler-initiated dirty marking, letting the PTY polling flow be the source of truth

## Notes

- The chunk backreference comment was correctly added at `editor_state.rs:1142`
- Project builds successfully (`cargo build --workspace`)
- Integration test passes (`test_paste_content_appears_after_poll`)
- No deviations from the plan documented (which is appropriate - the implementation followed the plan exactly)
