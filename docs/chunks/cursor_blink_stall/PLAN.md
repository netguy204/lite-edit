<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The cursor blink stall occurs when `dirty_lines_to_region_wrapped()` returns `DirtyRegion::None` even though the cursor is on screen. The root cause is a boundary condition: when `visible_lines() == 0`, the check `line_start_screen_row >= visible_end_screen_row` becomes true for ALL positions (since `visible_end_screen_row == first_visible_screen_row`), causing the method to incorrectly classify visible cursor positions as "below the viewport."

**When `visible_lines() == 0`:**
- `visible_end_screen_row = first_visible_screen_row + 0 = first_visible_screen_row`
- Any cursor at or after `first_visible_screen_row` satisfies `>= visible_end_screen_row`
- Result: `DirtyRegion::None` even when the cursor IS visible

This condition can occur when:
1. The viewport has never had `update_size()` called (initial state)
2. `sync_pane_viewports()` early-returns because `view_width == 0.0 || view_height == 0.0`
3. A tab switch activates a tab whose viewport was never sized

**The fix**: In `dirty_lines_to_region_wrapped()`, treat `visible_lines() == 0` as an uninitialized or degenerate viewport state and return `DirtyRegion::FullViewport`. This is safe because:
- It's the correct conservative choice (ensures repaint)
- It matches the existing `FullViewport` behavior used for scroll events
- The performance cost is negligible (one extra branch, only hit in the degenerate case)

Additionally, investigate whether the viewport initialization path has a gap that allows this state to persist, and add a guard in `cursor_dirty_region()` as defense-in-depth.

**Testing approach** (per TESTING_PHILOSOPHY.md):
- Write failing tests first that reproduce the bug: create a viewport with `visible_lines() == 0`, verify that `dirty_lines_to_region_wrapped()` incorrectly returns `None`, then fix and verify it returns `FullViewport`.
- Add regression test for the cursor dirty region method.
- Verify the fix doesn't break existing tests that rely on correct `None` returns for out-of-viewport regions.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport_scroll subsystem. The fix will be in `Viewport::dirty_lines_to_region_wrapped()`, which is a core method of this subsystem. The change is consistent with subsystem invariant #4: "DirtyRegion::merge is associative and commutative with None as identity. FullViewport absorbs everything." Using `FullViewport` as a fallback for the degenerate case is the safe, consistent choice.

No deviations from subsystem patterns discovered — the bug is a missing edge case, not a pattern violation.

## Sequence

### Step 1: Write failing test for zero-visible-lines edge case

Add a test to `crates/editor/src/viewport.rs` that reproduces the bug:

1. Create a `Viewport` with `line_height > 0` but never call `update_size()` (so `visible_lines() == 0`)
2. Create a `WrapLayout` and a simple line length function
3. Call `dirty_lines_to_region_wrapped()` with `DirtyLines::Single(0)`
4. Assert that the result is `DirtyRegion::FullViewport` (currently fails, returns `None`)

This test verifies the fix for the zero-visible-lines edge case.

Location: `crates/editor/src/viewport.rs` test module

### Step 2: Fix `dirty_lines_to_region_wrapped` to handle zero visible lines

At the start of `dirty_lines_to_region_wrapped()`, add a guard:

```rust
// Handle uninitialized or degenerate viewport (no visible lines)
// Return FullViewport as the safe conservative choice — a repaint will
// happen, which is correct behavior even if slightly inefficient.
if self.visible_lines() == 0 {
    return DirtyRegion::FullViewport;
}
```

This goes before the `match dirty` statement, after the `visible_end_screen_row` computation (or at the very start, before any computation, since we're returning early).

Location: `crates/editor/src/viewport.rs`, method `dirty_lines_to_region_wrapped`

### Step 3: Add chunk backreference comment

Add a chunk backreference at the guard to document why this case exists:

```rust
// Chunk: docs/chunks/cursor_blink_stall - Guard against zero visible lines
if self.visible_lines() == 0 {
    return DirtyRegion::FullViewport;
}
```

Location: `crates/editor/src/viewport.rs`

### Step 4: Verify existing tests still pass

Run the existing dirty region tests to ensure the fix doesn't break valid behavior:

```bash
cargo test -p lite-edit-editor dirty_lines
cargo test -p lite-edit-editor viewport
```

The existing tests use properly-initialized viewports with `visible_lines() > 0`, so they should continue to pass.

### Step 5: Add defense-in-depth guard in `cursor_dirty_region`

In `EditorState::cursor_dirty_region()`, add a fallback that returns `FullViewport` if the viewport's `visible_lines()` is 0. This provides defense-in-depth even if the viewport method fix is bypassed somehow:

```rust
fn cursor_dirty_region(&self) -> DirtyRegion {
    if let Some(buffer) = self.try_buffer() {
        // Defense-in-depth: if viewport not properly sized, force full repaint
        // Chunk: docs/chunks/cursor_blink_stall - Defense against uninitialized viewport
        if self.viewport().visible_lines() == 0 {
            return DirtyRegion::FullViewport;
        }

        // ... existing wrap-aware conversion code ...
    } else {
        DirtyRegion::FullViewport
    }
}
```

Location: `crates/editor/src/editor_state.rs`, method `cursor_dirty_region`

### Step 6: Add test for cursor_dirty_region with uninitialized viewport

Add a test in `editor_state.rs` that:

1. Creates an `EditorState` with a buffer but without calling `update_viewport_size()`
2. Calls `cursor_dirty_region()`
3. Asserts the result is `DirtyRegion::FullViewport`

This verifies the defense-in-depth works.

Location: `crates/editor/src/editor_state.rs` test module

### Step 7: Investigate viewport initialization gap (optional investigation)

Examine the code paths that lead to cursor blinking without viewport sizing:

1. Trace from app startup to first blink timer event
2. Check if there's a window where `toggle_cursor_blink()` can be called before `update_viewport_size()` or `sync_pane_viewports()`
3. If found, document the initialization gap and consider whether to add an initialization guard

This step is optional — the fix already handles the symptom. Understanding the root cause is valuable for preventing similar bugs but may not reveal actionable fixes.

Location: Analysis of `crates/editor/src/drain_loop.rs` and `crates/editor/src/editor_state.rs`

### Step 8: Update GOAL.md code_paths

Update the chunk's GOAL.md to list the files touched:

```yaml
code_paths:
  - crates/editor/src/viewport.rs
  - crates/editor/src/editor_state.rs
```

Location: `docs/chunks/cursor_blink_stall/GOAL.md`

### Step 9: Run full test suite and manual verification

1. Run all editor tests: `cargo test -p lite-edit-editor`
2. Build and run the app: `cargo run`
3. Open a file, wait for cursor blink to stabilize
4. Verify cursor continues blinking normally
5. Try edge cases: window resize, tab switch, opening selector (Cmd+P) then canceling

## Risks and Open Questions

1. **Root cause vs symptom**: The fix handles the symptom (`visible_lines() == 0` causing wrong dirty region) but doesn't prevent the state from occurring. The initialization gap (if any) remains. This is acceptable because:
   - The fix is safe (FullViewport is always correct, just potentially inefficient)
   - The degenerate state is rare (only during initialization edge cases)
   - Fixing initialization ordering may have broader implications

2. **Performance**: Returning `FullViewport` when `visible_lines() == 0` could cause unnecessary full repaints during app startup before the viewport is sized. This is likely imperceptible since:
   - It only happens during a brief initialization window
   - The app isn't rendering much content yet anyway

3. **Other callers of `dirty_lines_to_region_wrapped`**: The `EditorContext::mark_dirty()` method also calls this. Verify that callers can handle `FullViewport` correctly (they should, since it's a valid return value).

4. **Non-file tabs**: `cursor_dirty_region()` already returns `FullViewport` for terminal tabs, so this fix is consistent with that pattern.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->