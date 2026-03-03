<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The fix extends the `is_function_key` bypass check in `metal_view.rs:__key_down` to include navigation keys that currently fall through to `interpretKeyEvents:` and fail to reach the terminal input path reliably.

**Root cause analysis confirmed:**

The `keyDown:` method (line 312) has a bypass path for "function keys" that routes directly to `convert_key_event()` → `send_key()`. This bypass exists because certain keys don't produce useful text input through macOS's text input system and need direct handling.

The current `is_function_key` check (lines 325-329):
```rust
let is_function_key = matches!(key_code,
    0x7A..=0x7F | // F1-F4 and some system keys
    0x60..=0x6F | // F5-F12 and other function keys
    0x72         // Insert/Help
);
```

This covers:
- `0x7A-0x7F`: F1-F4, arrow keys (7B-7E), some system keys
- `0x60-0x6F`: F5-F12
- `0x72`: Insert/Help

But it misses keyCodes in the `0x73-0x79` gap:
- `0x73`: Home
- `0x74`: PageUp
- `0x75`: Forward Delete
- `0x77`: End
- `0x79`: PageDown

These keys go through `interpretKeyEvents:` → `doCommandBySelector` (e.g., `"pageUp:"`). While `doCommandBySelector` does map `"pageUp:"` to `Key::PageUp` (line 588-589), this route through the macOS text input system is unreliable in some keyboard/IME configurations and inside tmux copy mode.

**Fix strategy:**

Add explicit keyCode checks for the missing navigation keys to route them through the direct bypass path, matching how other terminal emulators (Alacritty) handle these keys. This is a minimal, low-risk change that doesn't affect the general text input path.

**Testing strategy:**

Per `docs/trunk/TESTING_PHILOSOPHY.md`, GPU rendering and macOS event handling are "humble objects" that can't be unit-tested meaningfully. The key mapping logic in `convert_key()` is already tested implicitly by existing tests (arrow keys, function keys work). The fix changes routing, not encoding.

Manual verification:
1. Open a terminal tab
2. Run `tmux`
3. Press Ctrl+B then `[` to enter copy mode
4. Press PageUp → should scroll back in scrollback buffer
5. Press PageDown → should scroll forward
6. Press Home → should jump to start of line/buffer
7. Press End → should jump to end of line/buffer

Regression check:
1. File buffer tabs should still handle PageUp/PageDown for viewport scrolling
2. Arrow keys, F1-F12, Tab, Return, Backspace should continue working

## Subsystem Considerations

No subsystems are directly relevant. This chunk modifies the macOS key event routing in `metal_view.rs`, which is a platform-specific input handling layer. The existing subsystems (`renderer`, `spatial_layout`, `viewport_scroll`) don't govern keyboard input dispatch.

## Sequence

### Step 1: Extend the bypass keyCode check to include navigation keys

**Location:** `crates/editor/src/metal_view.rs`, `__key_down` method (~line 325)

Modify the `is_function_key` variable to include the missing navigation keyCodes:
- `0x73`: Home (KEY_HOME)
- `0x74`: PageUp (KEY_PAGE_UP)
- `0x75`: Forward Delete (KEY_FORWARD_DELETE)
- `0x77`: End (KEY_END)
- `0x79`: PageDown (KEY_PAGE_DOWN)

**Option A (preferred)**: Add a separate `is_navigation_key` variable for clarity:
```rust
let is_navigation_key = matches!(key_code,
    0x73 | // Home
    0x74 | // PageUp
    0x75 | // Forward Delete
    0x77 | // End
    0x79   // PageDown
);
```

Then include it in the bypass condition:
```rust
if has_command || has_control || has_option || is_escape || is_function_key || is_navigation_key {
```

**Option B**: Extend the existing ranges, but this is less clear since these aren't function keys.

Add a chunk backreference comment before the new variable.

### Step 2: Add comment explaining why navigation keys bypass text input

Add a doc comment explaining that navigation keys (PageUp, PageDown, Home, End, Forward Delete) need to bypass `interpretKeyEvents:` because the text input system's selector-based routing (`pageUp:`, etc.) is unreliable in some keyboard configurations and terminal environments like tmux copy mode.

### Step 3: Manual verification

1. Build the project: `cargo build --release`
2. Launch the editor and open a terminal tab
3. Run `tmux` in the terminal
4. Enter copy mode with `Ctrl+B [`
5. Test PageUp → scrolls back
6. Test PageDown → scrolls forward
7. Test Home → jumps to start
8. Test End → jumps to end
9. Exit tmux and verify the same keys work in regular shell (less, man, etc.)

### Step 4: Regression check

1. Open a file buffer tab
2. Verify PageUp/PageDown scroll the viewport
3. Verify Home/End move cursor to line start/end
4. Verify arrow keys, Tab, Return, Backspace all work normally
5. Verify F1-F12 work (if testable in a TUI app)

## Dependencies

None. This chunk modifies existing code in `metal_view.rs`. The `convert_key()` function already handles these keyCodes correctly (PageUp → `Key::PageUp`, etc.), so no changes are needed to key mapping or terminal encoding.

## Risks and Open Questions

**Low risk.** This is a minimal change to routing logic:

1. **Could bypassing break something?** Unlikely. The `doCommandBySelector` path was a fallback, not the intended path. The bypass path (`convert_key_event()` → `send_key()`) is the same path used for arrow keys, F1-F12, and all modifier combinations. It's well-tested.

2. **What about file buffer behavior?** File buffers receive the same `KeyEvent` from either path. The `FocusTarget::handle_key` dispatch handles PageUp/PageDown for viewport scrolling regardless of how the event was routed through macOS.

3. **IME interference?** Navigation keys don't produce composed text, so bypassing the text input system won't affect IME composition sequences for CJK input.

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