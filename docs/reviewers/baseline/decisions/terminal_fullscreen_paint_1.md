---
decision: APPROVE
summary: All success criteria satisfied; mode transition detection triggers full viewport repaint ensuring fullscreen apps paint immediately
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: Opening Vim (or another static fullscreen app) in a terminal tab paints its initial screen content within one render frame of the PTY output arriving

- **Status**: satisfied
- **Evidence**: `TerminalBuffer` adds `was_alt_screen` field to track previous mode state. In `poll_events()` after processing PTY output, mode transitions are detected (`if is_alt != self.was_alt_screen`) and force `DirtyLines::FromLineToEnd(0)`. This triggers a full viewport repaint when apps enter alternate screen mode (ESC[?1049h). Tests `test_alt_screen_entry_forces_full_dirty` and `test_alt_screen_with_content_draw` verify this behavior.

### Criterion 2: Opening tmux in a terminal tab paints correctly without requiring a pane move

- **Status**: satisfied
- **Evidence**: Same mechanism as criterion 1. tmux uses ESC[?1049h to enter fullscreen UI, triggering the mode transition detection. The fix is application-agnostic - it detects the terminal mode change, not specific apps.

### Criterion 3: Animated apps like htop continue to render correctly (no regression)

- **Status**: satisfied
- **Evidence**: The fix only adds logic; existing damage tracking via `update_damage()` is unchanged. The mode transition check is a single boolean comparison per `poll_events()` call - negligible overhead. All 160 terminal tests pass, confirming no regressions.

### Criterion 4: The fix does not introduce unnecessary full-viewport invalidations beyond what is already present

- **Status**: satisfied
- **Evidence**: Mode transition detection only fires when `is_alt != self.was_alt_screen`. After detecting a transition, `was_alt_screen` is updated, so subsequent output in the same mode won't trigger additional full-viewport invalidations. Test `test_no_mode_transition_normal_dirty` verifies normal typing doesn't trigger mode transition logic.

### Criterion 5: Multi-pane rendering is not regressed

- **Status**: satisfied
- **Evidence**: Fix is at `TerminalBuffer` level, not pane/renderer level. Each terminal instance has its own independent `was_alt_screen` state. Dirty region propagation mechanism unchanged. All 160 tests pass.
