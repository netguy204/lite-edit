<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk fixes a specific transition case in the terminal viewport auto-follow logic: when switching from primary screen to alternate screen (e.g., running vim, htop, or less after scrolling through terminal output), the viewport's `scroll_offset_px` carries over inappropriately.

**The Bug:**

The existing code in `workspace.rs` (`poll_standalone_terminals`, ~line 1388-1398) handles two transitions:
1. **alt → primary** (`was_alt_screen && !now_alt_screen`): Snaps viewport to bottom ✓
2. **primary auto-follow** (`!now_alt_screen && was_at_bottom`): Advances viewport ✓

But it has no handler for:
3. **primary → alt** (`!was_alt_screen && now_alt_screen`): Missing!

When the viewport has scrolled down through large scrollback (e.g., after `cat large-file.txt`), `scroll_offset_px` might be, say, 5000px. When vim starts, the terminal enters alternate screen mode where `line_count()` drops to just `screen_lines` (~40 lines). The visible range calculation produces an empty range because `first_visible_line` far exceeds the alt-screen's minimal `line_count`. Result: nothing renders.

**The Fix:**

Add a branch for `!was_alt_screen && now_alt_screen` that calls `viewport.scroll_to_bottom(terminal.line_count())`. Since alt-screen's `line_count()` equals `screen_lines` and `screen_lines <= visible_lines` (they're matched during terminal resize), this effectively resets `scroll_offset_px` to 0.

**Implementation Strategy:**

This is a surgical, one-branch addition to the existing match block. The fix slots into the existing pattern established by the `terminal_scrollback_viewport` chunk.

**Testing per TESTING_PHILOSOPHY.md:**

The behavior involves PTY interaction and mode transitions that are difficult to unit test in isolation. However, we can:
1. Add a unit test for the state machine logic by mocking the terminal mode state
2. Verify visually: cat large file, run vim, expect full vim welcome screen

The existing test infrastructure for viewport scroll behavior (`is_at_bottom`, `scroll_to_bottom`) is already well-tested in `crates/editor/src/viewport.rs`.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll subsystem. Specifically, it calls `Viewport::scroll_to_bottom()` which is already documented in the subsystem's code references as implementing "Snap-to-bottom for keypress and mode transition reset (terminal scrollback)". This fix adds another mode transition case (primary→alt) to the existing alt→primary transition handling.

The fix follows the established patterns:
- Uses `viewport.scroll_to_bottom(terminal.line_count())` — the canonical scroll reset method
- Integrates with the existing auto-follow logic in `poll_standalone_terminals`
- Does not modify any subsystem-owned types, only adds a caller

## Sequence

### Step 1: Add primary→alt transition handler in poll_standalone_terminals

In `crates/editor/src/workspace.rs`, within the `poll_standalone_terminals` method, add a new branch to handle the primary→alt screen transition. The current code structure (approximately lines 1388-1398) is:

```rust
let now_alt_screen = terminal.is_alt_screen();

// Handle mode transition: alt -> primary means snap to bottom
if was_alt_screen && !now_alt_screen {
    viewport.scroll_to_bottom(terminal.line_count());
} else if !now_alt_screen && was_at_bottom {
    // Primary screen auto-follow
    viewport.scroll_to_bottom(terminal.line_count());
}
```

Add a branch before the existing conditions:

```rust
let now_alt_screen = terminal.is_alt_screen();

// Chunk: docs/chunks/terminal_alt_screen_viewport_reset - Reset viewport on primary→alt
if !was_alt_screen && now_alt_screen {
    // Entering alt screen: reset scroll position.
    // Alt screen line_count = screen_lines ≤ visible_lines, so this
    // effectively sets scroll_offset_px to 0.
    viewport.scroll_to_bottom(terminal.line_count());
} else if was_alt_screen && !now_alt_screen {
    // Handle mode transition: alt -> primary means snap to bottom
    viewport.scroll_to_bottom(terminal.line_count());
} else if !now_alt_screen && was_at_bottom {
    // Primary screen auto-follow
    viewport.scroll_to_bottom(terminal.line_count());
}
```

**Location:** `crates/editor/src/workspace.rs`, `poll_standalone_terminals` method

### Step 2: Verify with manual testing

Manual verification steps:
1. Open a terminal tab
2. Run `cat` on a large file (to generate scrollback and scroll the viewport)
3. Run `vim` (or `htop`, `less`, etc.)
4. Verify vim's welcome screen renders fully and correctly
5. Exit vim (`:q`)
6. Verify return to shell works correctly (existing behavior preserved)

Additional test cases:
- Fresh terminal (no prior scrolling) + vim → should work (baseline, shouldn't regress)
- `clear` followed by vim → should work
- Multiple iterations of cat-large-file → vim → exit → cat → vim

### Step 3: Add unit test for transition state machine (optional)

If testable in isolation, add a test that verifies the transition logic. This may require extracting the state machine logic into a testable helper, but given the simplicity of the fix (one additional branch), this may be overkill.

The existing tests for `Viewport::scroll_to_bottom` and `Viewport::is_at_bottom` in `crates/editor/src/viewport.rs` already provide coverage for the scroll primitives being used.

**Decision:** Given this is a single-branch addition using already-tested primitives, visual verification is sufficient per TESTING_PHILOSOPHY.md ("When TDD is impractical... verify visually, then add tests for the testable components").

## Dependencies

**Chunks that must be complete:**
- `terminal_scrollback_viewport` (ACTIVE) — Established the auto-follow behavior and mode transition handling in `poll_standalone_terminals`. This chunk adds to that implementation.

**No external dependencies.** All required infrastructure exists.

## Risks and Open Questions

**Minimal risk.** This is a targeted, single-branch addition to existing, well-tested code.

1. **Order of conditions matters**: The new branch must be checked BEFORE the existing `!now_alt_screen && was_at_bottom` branch, otherwise the logic is unchanged. The implementation adds it as the first condition.

2. **Edge case: rapid mode transitions**: If the terminal rapidly switches modes (e.g., vim exits and re-enters immediately), each transition will reset the scroll position. This is correct behavior — each alt screen session should start fresh.

3. **No race conditions**: The state (`was_alt_screen`, `now_alt_screen`) is captured synchronously within a single `poll_events` cycle. There's no window for drift.

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