<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is a timing issue between alternate screen content arrival and the render pass consuming the dirty state. When a fullscreen terminal app (like Vim) enters alternate screen mode (`ESC[?1049h`) and draws its initial content, the render frame may run before the content is fully written, consuming the `DirtyRegion::FullViewport` too early. Static apps then go idle, producing no further output to trigger a re-render.

The `terminal_single_pane_refresh` chunk (ACTIVE) fixed a related issue by moving the glyph buffer update inside the render pass. However, that fix focused on the timing of the glyph buffer read, not on ensuring that alternate screen transitions themselves trigger a follow-up render.

**Root Cause Analysis:**

After investigating the code:

1. **PTY output flow**: PTY data → `poll_events()` → `update_damage()` → `dirty = DirtyLines::FromLineToEnd(history_len)`
2. **Render flow**: `render_if_dirty()` checks `state.is_dirty()` → `render_with_editor()` (or similar)

The issue is that when entering alternate screen mode, the terminal content changes fundamentally (from primary screen with scrollback to alternate screen with no scrollback), but this mode transition itself isn't explicitly triggering a full viewport repaint. The `update_damage()` method marks from `history_len` forward, but in alternate screen mode `history_len` is 0 and `screen_lines` is the viewport size - so it does mark the viewport dirty.

However, the likely culprit is that the **mode transition itself** (primary → alternate) may arrive in one PTY chunk, and the **actual content** may arrive in the next PTY chunk. If the first chunk triggers a render before the content arrives, we get a blank screen. Then Vim sits waiting for input, never producing more output.

**Solution Strategy:**

Add a `mode_changed` flag to `TerminalBuffer` that detects when the terminal transitions between primary and alternate screen modes. When this flag is set during `poll_events()`, ensure a `DirtyRegion::FullViewport` is signaled to force a complete repaint. This guarantees that mode transitions always trigger at least one frame with fresh content.

This approach:
1. Is minimally invasive - only touches terminal damage tracking
2. Doesn't add unnecessary repaints (only fires on actual mode transitions)
3. Aligns with how the renderer already handles full viewport invalidation
4. Works for both single-pane and multi-pane modes (the fix is at the terminal level)

**Testing Approach:**

Per TESTING_PHILOSOPHY.md, GPU rendering itself is not unit-tested. However, we can test:
1. The mode transition detection logic (`was_alt_screen`, `is_alt_screen` tracking)
2. That `poll_events()` returns appropriate dirty state on mode transition

The visual verification (Vim paints correctly) will be done manually.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem. The fix adds content-level dirty tracking, which the renderer already handles correctly via `InvalidationKind::Content(DirtyRegion::FullViewport)`.

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll subsystem indirectly. The `Viewport::scroll_to_bottom` functionality is already called when exiting alternate screen mode (existing behavior in workspace.rs), which is correct. The fix ensures the visual repaint happens at mode entry time, not just exit.

No deviations from subsystem patterns are expected.

## Sequence

### Step 1: Add mode tracking to TerminalBuffer

Add a field to track the previous alternate screen state so we can detect mode transitions.

Location: `crates/terminal/src/terminal_buffer.rs`

```rust
// In TerminalBuffer struct:
/// Previous alternate screen mode state (for detecting transitions)
was_alt_screen: bool,
```

Initialize this in `new()` to `false` (terminal starts in primary screen mode).

### Step 2: Detect mode transition in poll_events

Modify `poll_events()` to detect when the terminal transitions between primary and alternate screen modes. When a transition occurs, force a full viewport dirty.

Location: `crates/terminal/src/terminal_buffer.rs#poll_events`

After processing PTY output (after `self.processor.advance()`), check:
```rust
let is_alt = self.is_alt_screen();
if is_alt != self.was_alt_screen {
    // Mode transition detected - force full viewport dirty
    self.dirty = DirtyLines::FromLineToEnd(0);
    self.was_alt_screen = is_alt;
}
```

The reason for `FromLineToEnd(0)` is that the entire viewport content has changed - either we entered alternate screen (new content) or exited (scrollback content reappears).

### Step 3: Update existing update_damage to preserve mode transition dirty

Ensure that `update_damage()` doesn't overwrite the mode transition dirty flag. Currently `update_damage()` does:
```rust
let new_dirty = DirtyLines::FromLineToEnd(history_len);
self.dirty.merge(new_dirty);
```

When in alternate screen mode, `history_len` is 0, so this becomes `FromLineToEnd(0)` which is correct. However, we need to ensure the mode transition check happens **after** `update_damage()` so that it can override if needed.

Actually, looking at the code flow:
1. `processor.advance()` processes bytes → may trigger mode change
2. `update_damage()` marks viewport dirty based on current state
3. Check for mode transition → override dirty if transition detected

The merge semantics mean `FromLineToEnd(0)` merged with anything produces `FromLineToEnd(0)`, so the order doesn't actually matter. But for clarity, we'll do the mode transition check right after `update_damage()` in `poll_events()`.

### Step 4: Write unit tests for mode transition detection

Add tests to verify:
1. Mode transition from primary → alternate sets dirty from line 0
2. Mode transition from alternate → primary sets dirty from line 0
3. No mode transition (staying in same mode) doesn't force extra dirty

Location: `crates/terminal/src/terminal_buffer.rs#tests`

```rust
#[test]
fn test_alt_screen_entry_forces_full_dirty() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    // Clear initial dirty
    let _ = terminal.take_dirty();

    // Enter alternate screen mode
    terminal.feed_bytes(b"\x1b[?1049h");

    // Dirty should indicate full viewport
    let dirty = terminal.take_dirty();
    assert!(matches!(dirty, DirtyLines::FromLineToEnd(0)));
}

#[test]
fn test_alt_screen_exit_forces_full_dirty() {
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    // Enter alternate screen
    terminal.feed_bytes(b"\x1b[?1049h");
    let _ = terminal.take_dirty();

    // Exit alternate screen mode
    terminal.feed_bytes(b"\x1b[?1049l");

    // Dirty should indicate full viewport
    let dirty = terminal.take_dirty();
    assert!(matches!(dirty, DirtyLines::FromLineToEnd(0)));
}
```

### Step 5: Manual verification

Test the fix manually:
1. Start lite-edit
2. Open a terminal tab (Cmd+Shift+T)
3. Run `vim` (or `nvim`) - should paint immediately without needing to move pane
4. Run `htop` - should continue to work correctly (regression check)
5. Test tmux - should paint initial screen correctly

### Step 6: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched:
```yaml
code_paths:
- crates/terminal/src/terminal_buffer.rs
```

## Risks and Open Questions

1. **Feed_bytes vs poll_events**: The `feed_bytes` test helper bypasses PTY and directly feeds to the processor. Need to verify it correctly triggers `update_damage()` like `poll_events()` does. Looking at the code, `feed_bytes` does NOT call `update_damage()` - it just marks everything dirty directly. This means the test will work but tests a slightly different path. For accurate testing, we may need to extend `feed_bytes` to include the damage tracking flow, or accept this limitation.

2. **Double invalidation**: When entering alt screen, we might get both the mode transition dirty AND the content dirty from actual Vim drawing. The merge semantics handle this correctly (`FromLineToEnd(0)` merged with `FromLineToEnd(0)` = `FromLineToEnd(0)`), but this is worth noting.

3. **Performance**: We're adding a boolean comparison per `poll_events()` call. This is negligible compared to the VTE processing overhead.

4. **Edge case - rapid mode flipping**: If an app rapidly switches between modes (unlikely in practice), we'll produce multiple full viewport invalidations. This is correct behavior - each transition genuinely needs a repaint.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
