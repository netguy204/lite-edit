---
decision: APPROVE
summary: "All success criteria satisfied; implementation correctly adds primary→alt screen transition handler following viewport_scroll subsystem patterns"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: After scrolling in a terminal (e.g., `cat` a large file), opening vim renders its full welcome screen correctly

- **Status**: satisfied
- **Evidence**: The new branch at `workspace.rs:1395-1400` adds handling for `!was_alt_screen && now_alt_screen` transition, calling `viewport.scroll_to_bottom(terminal.line_count())`. Since alt-screen's `line_count()` equals `screen_lines` which is ≤ `visible_lines`, this resets `scroll_offset_px` to 0 (per viewport.rs:231-234), ensuring the alt-screen content renders from the top.

### Criterion 2: Opening htop, less, or any alt-screen program after scrolling renders correctly

- **Status**: satisfied
- **Evidence**: The fix applies to ALL primary→alt screen transitions detected via `terminal.is_alt_screen()`. Any program that enters alt-screen mode (vim, htop, less, etc.) triggers the same viewport reset logic.

### Criterion 3: Existing alt→primary transition (exiting vim) continues to work — viewport snaps to bottom of primary screen

- **Status**: satisfied
- **Evidence**: The existing branch at `workspace.rs:1392-1394` (`was_alt_screen && !now_alt_screen`) remains unchanged. The new primary→alt branch is added as an `else if`, preserving the original alt→primary behavior.

### Criterion 4: Primary screen auto-follow (new output while at bottom) continues to work

- **Status**: satisfied
- **Evidence**: The existing branch at `workspace.rs:1401-1403` (`!now_alt_screen && was_at_bottom`) remains unchanged. The conditional chain `if / else if / else if` ensures mutually exclusive handling - primary auto-follow only triggers when neither transition case applies.

### Criterion 5: Fresh terminals (no prior scrolling) continue to work unchanged

- **Status**: satisfied
- **Evidence**: Fresh terminals have `scroll_offset_px = 0`. When entering alt-screen, `scroll_to_bottom` is still called, but since alt-screen `line_count <= visible_lines`, it sets offset to 0 (which is already the current value). No functional change for fresh terminals.

## Subsystem Compliance

The implementation correctly uses `Viewport::scroll_to_bottom` per the viewport_scroll subsystem (docs/subsystems/viewport_scroll/OVERVIEW.md:28-29), which documents this method as the canonical approach for "snap-to-bottom for keypress and mode transition reset (terminal scrollback)". The implementation follows the existing pattern established by the alt→primary transition handler at line 1394.

## Code Quality

- **Backreference**: Includes proper chunk backreference comment at line 1396
- **Comments**: Clear explanation of why `scroll_to_bottom` resets to 0 for alt-screen
- **No regressions**: All tests pass (excluding pre-existing flaky performance tests in syntax crate, which is unmodified by this chunk)
