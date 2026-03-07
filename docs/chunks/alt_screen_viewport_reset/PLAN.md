<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a targeted bugfix in `workspace.rs` that adds a missing branch to the
terminal screen mode transition handling in `poll_standalone_terminals`.

The existing code correctly handles:
1. **alt → primary transition**: `was_alt_screen && !now_alt_screen` — snaps viewport to bottom of primary scrollback
2. **primary auto-follow**: `!now_alt_screen && was_at_bottom` — advances viewport to show new output

But it's missing:
3. **primary → alt transition**: `!was_alt_screen && now_alt_screen` — should reset viewport to 0

When entering alt-screen after scrolling in primary mode, the `scroll_offset_px` from primary
(potentially thousands of pixels due to scrollback) carries over to alt-screen. But alt-screen
has `line_count = screen_lines ≈ 40`, so the scroll position points far past the end of content.
The viewport's `visible_range` becomes empty and nothing renders.

The fix is simple: add a branch for `!was_alt_screen && now_alt_screen` that calls
`viewport.scroll_to_bottom(terminal.line_count())`. Since alt-screen's `line_count()` equals
`screen_lines` which is ≤ `visible_lines`, `scroll_to_bottom` will set `scroll_offset_px` to 0
(per the implementation at viewport.rs:231-234).

This follows the viewport_scroll subsystem's patterns: we use the existing `scroll_to_bottom`
method rather than directly manipulating `scroll_offset_px`.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll
  subsystem. Specifically, it calls `Viewport::scroll_to_bottom` which is already documented
  as the method for "snap-to-bottom for keypress and mode transition reset (terminal scrollback)".
  The fix follows the existing pattern established by the alt→primary transition handler.

## Sequence

### Step 1: Write failing test for primary→alt transition

Add a unit test to `workspace.rs` that verifies the viewport scroll position is reset when
a terminal enters alternate screen mode after scrolling.

The test should:
1. Create a terminal tab with a viewport that has non-zero scroll offset
2. Simulate a poll cycle where the terminal transitions from primary to alt screen
3. Assert that the viewport's scroll offset is reset to 0

**Note**: The terminal buffer's `is_alt_screen()` state is controlled by Alacritty's parser
processing escape sequences, which makes direct unit testing challenging. We have two options:

a) **Preferred**: Mock the screen mode by exposing test helpers on TerminalBuffer
b) **Alternative**: Test via integration test that sends actual escape sequences

Given TDD constraints, start with option (a) if feasible, otherwise document that this is
a case where TDD is impractical (per TESTING_PHILOSOPHY.md) and verify manually.

Location: `crates/editor/src/workspace.rs` (test module) or integration test

### Step 2: Add primary→alt transition branch

In `poll_standalone_terminals` (workspace.rs ~line 1392), add a new condition to handle
the primary→alt screen transition:

```rust
// Handle mode transition: alt -> primary means snap to bottom
if was_alt_screen && !now_alt_screen {
    viewport.scroll_to_bottom(terminal.line_count());
} else if !was_alt_screen && now_alt_screen {
    // Chunk: docs/chunks/alt_screen_viewport_reset
    // Primary -> alt screen: reset scroll to 0
    // Alt screen has line_count = screen_lines <= visible_lines,
    // so scroll_to_bottom sets offset to 0
    viewport.scroll_to_bottom(terminal.line_count());
} else if !now_alt_screen && was_at_bottom {
    // Primary screen auto-follow
    viewport.scroll_to_bottom(terminal.line_count());
}
```

Location: `crates/editor/src/workspace.rs` (~line 1392-1398)

### Step 3: Manual verification

Build and run the editor. Test the reproduction case:
1. Open a terminal tab
2. `cat` a large file (causes scrolling/scrollback)
3. Run `vim` (enters alt-screen)
4. Verify vim's welcome screen renders correctly
5. Exit vim (`:q`)
6. Verify primary screen content is visible at bottom

Also test:
- `htop` after scrolling
- `less <file>` after scrolling
- Fresh terminal (no prior scrolling) still works
- Alt→primary transition (exiting vim) still snaps to bottom correctly

### Step 4: Run existing tests

Ensure no regressions in terminal-related tests:

```bash
cargo test -p lite-edit-editor
cargo test -p lite-edit-terminal
```

## Risks and Open Questions

1. **Test feasibility**: The terminal's `is_alt_screen()` state depends on Alacritty's parser
   processing escape sequences. Creating a unit test may require test helpers that don't
   currently exist. If unit testing proves impractical, manual verification is acceptable
   per TESTING_PHILOSOPHY.md ("GPU rendering, macOS window management... verify visually").

2. **No race condition concerns**: `poll_standalone_terminals` runs synchronously on the
   main thread and both `was_alt_screen` and `now_alt_screen` are sampled within the same
   poll cycle, so there's no TOCTOU risk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
