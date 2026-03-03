---
decision: APPROVE
summary: "All success criteria satisfied; implementation adds the missing primary→alt screen transition handler exactly as planned, using documented subsystem patterns"
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: After scrolling in a terminal (e.g., `cat` a large file), opening vim renders its full welcome screen correctly

- **Status**: satisfied
- **Evidence**: The new branch at `workspace.rs:1393-1397` detects `!was_alt_screen && now_alt_screen` and calls `viewport.scroll_to_bottom(terminal.line_count())`. Since alt-screen line_count equals screen_lines ≤ visible_lines, this resets scroll_offset_px to 0, ensuring the viewport starts at the top of the alt-screen content where vim draws.

### Criterion 2: Opening htop, less, or any alt-screen program after scrolling renders correctly

- **Status**: satisfied
- **Evidence**: The fix is generic to any primary→alt screen transition, not specific to vim. Any program that enters alternate screen mode (htop, less, nano, etc.) will trigger the same `!was_alt_screen && now_alt_screen` condition and receive the viewport reset.

### Criterion 3: Existing alt→primary transition (exiting vim) continues to work — viewport snaps to bottom of primary screen

- **Status**: satisfied
- **Evidence**: The original `was_alt_screen && !now_alt_screen` branch remains unchanged at `workspace.rs:1398-1400`. The new primary→alt branch is added as the first condition in the if-else chain, so the alt→primary logic continues to execute when applicable.

### Criterion 4: Primary screen auto-follow (new output while at bottom) continues to work

- **Status**: satisfied
- **Evidence**: The `!now_alt_screen && was_at_bottom` branch remains unchanged at `workspace.rs:1401-1403`. It's the third condition in the if-else chain and executes when neither screen transition condition applies and the viewport was previously at the bottom.

### Criterion 5: Fresh terminals (no prior scrolling) continue to work unchanged

- **Status**: satisfied
- **Evidence**: Fresh terminals start with scroll_offset_px = 0. When entering alt-screen, the `scroll_to_bottom()` call computes max_offset = (line_count - visible_lines) * line_height. Since alt-screen line_count ≤ visible_lines, max_offset is 0 or negative (clamped to 0). The viewport stays at 0 — no change from current behavior.

## Subsystem Compliance

The implementation correctly uses `Viewport::scroll_to_bottom()` as documented in the `viewport_scroll` subsystem OVERVIEW.md:
- Code reference `viewport.rs#Viewport::scroll_to_bottom` states: "Snap-to-bottom for keypress and mode transition reset (terminal scrollback)"
- The fix adds another mode transition case (primary→alt) using the same canonical method

A backreference comment links the code to the chunk documentation: `// Chunk: docs/chunks/terminal_alt_screen_viewport_reset - Reset viewport on primary→alt`
