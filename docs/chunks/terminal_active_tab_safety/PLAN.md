<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The crash occurs because `EditorState::buffer()` and `buffer_mut()` unconditionally call `.expect("active tab is not a file tab")` on the result of `as_text_buffer()`. When a terminal tab is active, this expectation fails.

The strategy is **Option-returning helpers with guarded call sites**:

1. **Change `buffer()` / `buffer_mut()` to return `Option`** rather than panicking. This is the safest approach because it makes every call site explicitly handle the terminal-tab case.

2. **Add `try_buffer()` / `try_buffer_mut()` helper methods** that return `Option<&TextBuffer>` for call sites that can gracefully no-op when no text buffer is available.

3. **Guard all existing call sites** to either:
   - Early-return/no-op when the active tab is not a file tab (keyboard handling, cursor blink, search, etc.)
   - Skip the operation (viewport sync, dirty region calculation)

4. **Terminal tab keyboard handling** should delegate to `TerminalFocusTarget` when the active tab is a terminal. The existing `terminal_target.rs` module provides this capability.

This follows the Humble View architecture from TESTING_PHILOSOPHY.md: all decision logic stays in testable pure Rust code, and we push the "what kind of tab is this?" question to the call sites.

**No changes to the rendering path**: The renderer already handles terminal tabs separately via `BufferView` trait dispatch. This chunk focuses purely on the event-handling/state-management side.

## Sequence

### Step 1: Add `try_buffer()` and `try_buffer_mut()` to EditorState

Add new Option-returning accessor methods that don't panic:

```rust
/// Returns a reference to the active tab's TextBuffer, if it's a file tab.
pub fn try_buffer(&self) -> Option<&TextBuffer> { ... }

/// Returns a mutable reference to the active tab's TextBuffer, if it's a file tab.
pub fn try_buffer_mut(&mut self) -> Option<&mut TextBuffer> { ... }
```

Leave the existing `buffer()` and `buffer_mut()` methods unchanged for now to avoid breaking all call sites at once. They will be deprecated after migration.

Location: `crates/editor/src/editor_state.rs`

### Step 2: Add `active_tab_is_file()` helper

Add a helper method to check if the active tab is a file tab without accessing the buffer:

```rust
/// Returns true if the active tab is a file tab (has a TextBuffer).
pub fn active_tab_is_file(&self) -> bool { ... }
```

This provides a cheap check for code paths that need to early-return.

Location: `crates/editor/src/editor_state.rs`

### Step 3: Guard `update_viewport_size()` and `update_viewport_dimensions()`

These methods call `self.buffer().line_count()` which panics on terminal tabs.

Change to:
```rust
pub fn update_viewport_size(&mut self, window_height: f32) {
    let line_count = self.try_buffer().map(|b| b.line_count()).unwrap_or(0);
    // ... rest unchanged
}
```

Terminal tabs don't use the Viewport in the same way, so a line_count of 0 is harmless.

Location: `crates/editor/src/editor_state.rs` (lines 254-277)

### Step 4: Guard `handle_cmd_f()` find-in-file

The find strip should only open when a file tab is active. Terminal tabs use the shell's search.

Change to early-return if `!self.active_tab_is_file()`.

Location: `crates/editor/src/editor_state.rs` (line 508-530)

### Step 5: Guard `run_live_search()` and `advance_to_next_match()`

These methods use `self.buffer()` and `self.buffer_mut()`. Guard them with early returns.

Location: `crates/editor/src/editor_state.rs` (lines 693-764)

### Step 6: Guard `handle_key_buffer()` to route terminal tabs separately

This is the key method. When `focus == Buffer` and the active tab is a terminal, keyboard input should go to `TerminalFocusTarget` instead of `BufferFocusTarget`.

```rust
fn handle_key_buffer(&mut self, event: KeyEvent) {
    // Check if active tab is a terminal
    let ws = self.editor.active_workspace_mut().expect("no active workspace");
    let tab = ws.active_tab_mut().expect("no active tab");

    if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut() {
        // Existing file-tab handling path
        // ...
    } else if let Some(terminal) = tab.buffer.as_terminal_buffer_mut() {
        // Terminal tab: encode and send to PTY
        // Use InputEncoder or TerminalFocusTarget pattern
        // ...
    }
    // Other tab types (AgentOutput, Diff): no-op
}
```

Location: `crates/editor/src/editor_state.rs` (lines 899-954)

### Step 7: Guard `handle_mouse_buffer()` for terminal tabs

Similar to Step 6, route mouse events to the terminal when a terminal tab is active.

Location: `crates/editor/src/editor_state.rs` (lines 1060-1119)

### Step 8: Guard `handle_scroll()` for terminal tabs

Scroll events on terminal tabs should scroll the terminal's viewport, not the text buffer viewport.

Location: `crates/editor/src/editor_state.rs` (lines 1122-1163)

### Step 9: Guard `cursor_dirty_region()` and `toggle_cursor_blink()`

These use `self.buffer().cursor_position()`. For terminal tabs, return `DirtyRegion::FullViewport` since the cursor is part of the terminal grid.

Location: `crates/editor/src/editor_state.rs` (lines 1289-1317)

### Step 10: Guard `associate_file()` and `save_file()`

These should no-op for terminal tabs. Add early-return guards.

Location: `crates/editor/src/editor_state.rs` (lines 1343-1403)

### Step 11: Add tests for terminal tab safety

Write tests that:
1. Create an EditorState with a file tab
2. Add a terminal tab and switch to it
3. Simulate key events, mouse events, scroll events
4. Verify no panics occur and state remains consistent
5. Switch back to file tab and verify normal operation

Follow TESTING_PHILOSOPHY.md patterns:
- Use `EditorState::default()` for setup
- Test boundary conditions (switch to terminal, type, switch back)
- Assert semantic properties (cursor position preserved, no crash)

Location: `crates/editor/src/editor_state.rs` (tests module)

### Step 12: Migrate call sites from `buffer()` to `try_buffer()`

After all guards are in place, audit remaining uses of `buffer()` / `buffer_mut()`:
- Change them to `try_buffer()` / `try_buffer_mut()` with appropriate guards
- Or verify they're only reachable when active tab is known to be a file tab

Location: `crates/editor/src/editor_state.rs` (throughout)

### Step 13: Update `buffer()` / `buffer_mut()` to be safe

Option A: Change signatures to return `Option<&TextBuffer>` (breaking change but complete safety)

Option B: Keep existing signatures but add `debug_assert!(self.active_tab_is_file())` to catch misuse in tests

This step depends on how many external callers exist. For an internal-only API, Option A is cleaner.

Location: `crates/editor/src/editor_state.rs`

## Dependencies

- **terminal_tab_spawn** (complete): Provides `Tab::new_terminal()`, `TerminalBuffer`, and the `Cmd+Shift+T` keybinding that triggers the crash.
- **terminal_input_encoding** (complete): Provides `InputEncoder` and `TerminalFocusTarget` for routing keyboard input to terminal tabs.

## Risks and Open Questions

1. **Terminal scroll state**: The current `Viewport` is designed for text buffers with line counts. Terminal scrollback works differently (fixed screen + scrollback history). We may need to handle viewport operations as no-ops for terminals or introduce a separate viewport concept.

2. **Dirty region tracking**: Terminal tabs may need different dirty region semantics. For now, treating all terminal activity as `FullViewport` is conservative but correct.

3. **Focus target architecture**: The existing `BufferFocusTarget` field on `EditorState` assumes a single focus target. With terminal tabs, we need conditional dispatch. The plan uses inline checks rather than a polymorphic focus target to avoid larger refactoring.

4. **Test infrastructure**: Creating `TerminalBuffer` instances for tests may require mocking PTY operations. We may need to test at a higher level (e.g., verify no panic rather than verify specific behavior).

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