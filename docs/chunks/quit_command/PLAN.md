<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Implement Cmd+Q as a quit action that cleanly terminates the macOS application. The design follows the existing input handling architecture established in `editable_buffer` and `line_nav_keybindings` chunks.

**Architecture Decision**: Handle quit at the `EditorState`/`EditorController` level rather than in `BufferFocusTarget.resolve_command()`. Rationale:

1. **Quit is an app-level concern**, not a buffer command. The `Command` enum in `buffer_target.rs` contains editing operations (insert, delete, move). Adding `Quit` there would mix concerns.

2. **Signal propagation is cleaner**. The `EditorState.handle_key()` method returns `void` today, while `FocusTarget.handle_key()` returns `Handled`. We have two options:

   - **Option A (chosen)**: Intercept Cmd+Q in `EditorState.handle_key()` *before* forwarding to the focus target. When detected, set a flag (e.g., `should_quit: bool`) that the `EditorController` checks after each key event. This keeps the focus target's responsibility clear (buffer editing) and handles quit as a global shortcut.

   - **Option B**: Add a `Quit` variant to the `Handled` enum (e.g., `Handled::Quit`). This requires changing the return signature and checking for quit at every call site. More invasive.

3. **`NSApplication::terminate:`** is called from the Objective-C side. The quit flag bridges the Rust signal to the Cocoa termination. We already have `NSApplication::sharedApplication(mtm)` accessible in `main.rs`.

**Testing Strategy** (per TESTING_PHILOSOPHY.md):

- Unit test that Cmd+Q sets the quit flag in `EditorState`
- Unit test that other Cmd+key combinations (e.g., Cmd+Z) do NOT set the quit flag
- The actual `NSApplication::terminate:` call is platform code (humble view) and cannot be unit tested

## Sequence

### Step 1: Add `should_quit` flag to `EditorState`

Add a `pub should_quit: bool` field to `EditorState`, initialized to `false`. This flag signals that the app should terminate.

**Location**: `crates/editor/src/editor_state.rs`

### Step 2: Intercept Cmd+Q in `EditorState.handle_key()`

At the top of `EditorState.handle_key()`, before forwarding to the focus target, check for Cmd+Q:

```rust
// Check for app-level shortcuts before delegating to focus target
if event.modifiers.command && !event.modifiers.control {
    if let Key::Char('q') = event.key {
        self.should_quit = true;
        return;
    }
}
```

This intercepts the quit shortcut globally, regardless of which focus target is active.

**Location**: `crates/editor/src/editor_state.rs`

### Step 3: Add unit tests for quit flag behavior

Write tests verifying:

1. `Cmd+Q` sets `should_quit` to `true`
2. `Cmd+Q` does NOT modify the buffer (the key event is consumed)
3. `Ctrl+Q` does NOT set the quit flag (Ctrl+Q is a different binding)
4. `Cmd+Z` does NOT set the quit flag (other Cmd+ combinations are unaffected)

**Location**: `crates/editor/src/editor_state.rs` (in the `#[cfg(test)]` module)

### Step 4: Check quit flag in `EditorController.handle_key()`

After `self.state.handle_key(event)`, check `self.state.should_quit` and call `NSApplication::terminate:` if true:

```rust
fn handle_key(&mut self, event: KeyEvent) {
    self.state.handle_key(event);

    // Check for quit request
    if self.state.should_quit {
        self.terminate_app();
        return;
    }

    self.render_if_dirty();
}
```

Implement `terminate_app()` as a helper method that calls `NSApplication::sharedApplication(mtm).terminate(None)`. Getting the `MainThreadMarker` requires passing it in or obtaining it fresh—since `EditorController` runs on the main thread, `MainThreadMarker::new()` should succeed.

**Location**: `crates/editor/src/main.rs` (in the `EditorController` impl)

### Step 5: Verify clean shutdown

Manually test (cargo run) that:
- Cmd+Q closes the window and terminates the process
- No warnings or errors appear in the console
- The behavior matches clicking the window's close button

This is visual/manual verification since `NSApplication::terminate:` cannot be exercised in a unit test context.

## Dependencies

- `line_nav_keybindings` chunk: Establishes the pattern of intercepting modifier+key combinations in `resolve_command`. We diverge slightly by handling quit in `EditorState` rather than `BufferFocusTarget`, but the modifier check pattern (`mods.command && !mods.control`) is consistent.
- `editable_buffer` chunk: Provides the `EditorState`, `EditorController`, and key event flow that this chunk extends.

## Risks and Open Questions

1. **MainThreadMarker in `terminate_app()`**: We need a `MainThreadMarker` to call `NSApplication::sharedApplication(mtm)`. Since `EditorController` is only accessed from the main thread (via callbacks from the run loop), `MainThreadMarker::new().unwrap()` should work. If not, we may need to store the marker or pass it through the callback chain.

2. **Unsaved changes warning**: Currently there's no dirty-file tracking, so quit always succeeds immediately. A future chunk might add "save changes?" confirmation. The quit flag approach supports this—instead of calling `terminate_app()` immediately, we could show a dialog first.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
