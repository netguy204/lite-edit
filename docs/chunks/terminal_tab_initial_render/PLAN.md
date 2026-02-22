<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is a classic race condition between initial rendering and asynchronous PTY output:

1. `new_terminal_tab()` spawns the shell and marks `DirtyRegion::FullViewport`
2. The renderer processes this dirty region and renders—but the PTY hasn't produced any output yet
3. The shell's initial prompt arrives asynchronously (via the PTY reader thread sending to a channel)
4. The prompt data sits in the channel until the next `poll_agents()` call (500ms cursor blink timer)
5. Nothing triggers a re-render until a resize forces a full redraw

**Solution**: After creating a terminal tab, we need to ensure the rendering system continues to check for PTY output and re-render until the terminal has visible content. There are two viable approaches:

### Option A: Deferred Re-render via Timer (Chosen)
Schedule a one-shot "PTY readiness" check a short time after tab creation (e.g., 50-100ms). This gives the shell time to produce output and then triggers a poll + render cycle.

**Pros**: Simple, non-invasive, matches macOS cocoa patterns (deferred layout)
**Cons**: Fixed delay that may be too short/long for some systems

### Option B: Poll Until Content Appears
Mark the terminal as "awaiting initial content" and have the timer-based poll loop render more aggressively until content appears.

**Pros**: Adapts to actual shell startup time
**Cons**: More state tracking, risk of infinite polling if shell fails to start

We will implement **Option A** with a slight enhancement: instead of a fixed delay, we'll poll immediately after `new_terminal_tab()` (in case the shell is fast), then the existing timer will catch any delayed output.

The key change is that `new_terminal_tab()` should:
1. Create and add the terminal tab (existing behavior)
2. Immediately poll for PTY events (new)
3. If events were processed, the dirty region is already set
4. If no events yet, schedule a deferred poll via an existing mechanism

Since the cursor blink timer already calls `poll_agents()` every 500ms, the deferred case will resolve naturally. However, 500ms is too slow for good UX. We'll add an immediate poll in `new_terminal_tab()` with a brief sleep to give the shell startup time, or leverage the main loop's structure.

**Chosen implementation**: Add an immediate PTY poll call right after `new_terminal_tab()` returns, in the same call site where we already call `poll_agents()` after key events. The `handle_key` path in `EditorController` already does this, so the fix is minimal.

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport subsystem for dirty region tracking. The viewport's `DirtyRegion` mechanism is central to understanding why the initial render fails—the dirty region is consumed before PTY output arrives.

## Sequence

### Step 1: Add an integration test demonstrating the bug

Create a test that verifies terminal tabs render their initial content. This follows TDD methodology—the test should fail initially because the bug exists.

**Location**: `crates/terminal/tests/integration.rs`

The test should:
1. Create a `TerminalBuffer` with a shell
2. Wait briefly for shell startup
3. Poll for PTY events
4. Verify that `line_count()` shows more than the viewport rows, OR
5. Verify that `styled_line(N)` for some visible row contains non-whitespace

This test validates the PTY → TerminalBuffer flow, not the full rendering pipeline (which is platform-dependent and follows humble view principles).

### Step 2: Add a unit test for the EditorState behavior

Create a test in `editor_state.rs` that:
1. Creates an `EditorState` with `new_terminal_tab()`
2. Simulates a brief delay (or uses test timing control)
3. Verifies that `poll_agents()` produces dirty output when called after creation

**Location**: `crates/editor/src/editor_state.rs` (in the existing `#[cfg(test)]` module)

This test captures the requirement that PTY output should be available and should produce a dirty region after polling.

### Step 3: Schedule a deferred PTY poll after terminal creation

Modify `EditorState::new_terminal_tab()` to request a deferred render cycle. The simplest approach is to use a flag that the timer callback checks.

**Option A (simpler)**: Have `new_terminal_tab()` return a boolean indicating that a "needs-attention" terminal was created. The caller (`EditorController::handle_key`) already calls `poll_agents()` after key events, so the pattern exists.

However, since the shell hasn't started yet at the moment of `new_terminal_tab()` return, we need the timer to catch this case. The timer runs every 500ms which is too slow.

**Option B (chosen)**: Add a "pending initial render" flag to terminals, checked by `poll_agents()`. When set, `poll_agents()` returns `FullViewport` even if no PTY events arrived yet. This triggers re-renders until the shell prompt appears.

Actually, upon review, the problem is simpler: the *first* render happens *before* the shell has had time to output anything. The shell needs milliseconds to start, but the render happens synchronously.

**Revised approach**: In `EditorController::handle_key`, after `new_terminal_tab()` is called (via `handle_key` → `new_terminal_tab()`), add a brief spin-poll loop that tries to get PTY output:

```rust
// After detecting Cmd+Shift+T and calling new_terminal_tab():
for _ in 0..10 {
    std::thread::sleep(Duration::from_millis(10));
    let dirty = self.state.poll_agents();
    if dirty.is_dirty() {
        self.state.dirty_region.merge(dirty);
        break;
    }
}
```

This gives up to 100ms for the shell to produce output, polling every 10ms.

**Location**: `crates/editor/src/main.rs#EditorController::handle_key`

This is simple, targeted, and doesn't require architectural changes.

### Step 4: Alternative approach - Add `needs_initial_render` flag

If the spin-poll in Step 3 causes UI jank (blocking the main thread), use a flag-based approach instead:

1. Add `needs_initial_render: bool` to the `Tab` struct (or just check if it's a fresh terminal)
2. In `poll_agents()`, after polling a terminal, check if it produced any visible output
3. If a terminal tab is active, has PTY, but hasn't rendered content yet, return `FullViewport` to force re-polls

**Location**:
- `crates/editor/src/workspace.rs#Tab`
- `crates/editor/src/workspace.rs#Workspace::poll_standalone_terminals`
- `crates/editor/src/editor_state.rs#EditorState::poll_agents`

### Step 5: Verify fix and clean up

1. Run the tests from Steps 1 and 2 to verify they pass
2. Manually test by pressing Cmd+Shift+T and confirming the shell prompt appears immediately
3. Verify no flicker or double-render artifacts
4. Verify switching tabs and back still works
5. Run the full test suite to check for regressions

### Step 6: Add backreference comments

Add chunk backreference comments to the modified code:

```rust
// Chunk: docs/chunks/terminal_tab_initial_render - Deferred PTY poll for initial content
```

**Locations**:
- The poll loop or flag check added in Step 3 or Step 4
- Any new test methods

## Dependencies

- **terminal_tab_spawn** (ACTIVE): Provides `new_terminal_tab()` and `poll_agents()` infrastructure
- **terminal_input_render_bug** (ACTIVE): Established the PTY polling pattern in timer and input handlers

## Risks and Open Questions

1. **Spin-poll blocking main thread**: The 10ms × 10 iterations = 100ms max block could cause noticeable UI pause. If this is unacceptable, fall back to the flag-based approach (Step 4).

2. **Shell startup time variance**: Some shells (zsh with plugins, fish) take longer to start than others. 100ms may not be enough for all configurations. The fallback is the existing 500ms timer which will eventually render the content.

3. **Testing shell-dependent behavior**: Integration tests that spawn real shells can be flaky in CI environments. Consider mocking the PTY or using a predictable command like `echo` instead of a full shell.

4. **Race between poll and render**: Even with the fix, there's a theoretical race where render happens just before PTY output arrives. The 500ms timer serves as the backstop, making this a UX issue rather than a correctness bug.

## Deviations

- Step 3: Instead of inlining the spin-poll loop directly in `EditorController::handle_key`,
  we added a `pending_terminal_created: bool` flag to `EditorState` and a dedicated
  `spin_poll_terminal_startup()` method. This approach:
  - Encapsulates the spin-poll logic within EditorState where it belongs
  - Makes the intent clearer (method name documents what's happening)
  - Allows the flag to be set in `new_terminal_tab()` without changing its return type
  - Keeps the EditorController simple (just calls the method after each key event)

- Step 4: The flag-based approach was combined with Step 3 rather than being a fallback.
  The "pending_terminal_created" flag serves the same purpose as the "needs_initial_render"
  flag mentioned in Step 4, but triggers a spin-poll rather than aggressive timer polling.
  This is simpler and achieves the same goal without modifying the timer callback behavior.