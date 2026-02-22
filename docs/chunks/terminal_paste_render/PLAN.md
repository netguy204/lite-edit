<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a **semantic bug fix** that addresses a timing issue in the paste → render cycle for terminal tabs. The bug manifests as blank spaces appearing instead of pasted characters until Enter is pressed.

### Root Cause Analysis

After tracing the code paths, the issue is identified as follows:

**The paste flow:**
1. `handle_key` in `editor_state.rs` handles Cmd+V (lines 1140-1151)
2. Writes bytes to PTY via `terminal.write_input(&bytes)`
3. Marks `DirtyRegion::FullViewport` and returns immediately
4. `handle_key` in `main.rs` calls `poll_agents()` which invokes `poll_events()` on the terminal

**Why paste shows blank spaces:**
The immediate `FullViewport` dirty marking triggers a render *before* the shell has echoed the pasted text back through the PTY. The grid state at render time contains the **pre-paste cursor position** — spaces where characters will eventually appear. When the PTY echo arrives (possibly microseconds later), the next render shows the correct content, but the damage may not trigger a visible re-render if the system considers those lines "clean."

**Why TUI apps work (key diagnostic clue):**
TUI apps use alternate screen mode (`styled_line_alt_screen`) which directly indexes into the grid: `grid[Line(line as i32)]`. The primary screen uses `styled_line_hot()` which involves offset arithmetic with `cold_line_count` and `history_len`. The problem is specific to primary screen rendering.

**Why single-character typing works:**
For single characters, the `terminal_input_render_bug` fix ensures `poll_agents()` is called after each keystroke. A single character echo is near-instant, so the poll catches it before render. But paste sends many bytes at once — the shell may not echo all of them before the poll completes.

### Fix Strategy

The fix has two components:

1. **Remove the premature dirty marking**: The paste handler should NOT mark `FullViewport` dirty immediately. The echoed content will arrive via PTY and the normal `poll_agents()` → `update_damage()` flow will mark the correct lines dirty.

2. **Ensure poll happens before render**: The existing `poll_agents()` call in `handle_key` (main.rs) should be sufficient, but we may need a small delay or multiple poll attempts for large pastes.

**Alternative considered but rejected**: Blocking/synchronous wait for PTY echo. This would add latency and complexity. The current async flow is correct — we just need to remove the premature dirty marking that triggers a render of stale content.

### Why This Works

When we remove the `FullViewport` marking from the paste handler:
- No immediate render is triggered (buffer isn't dirty yet)
- `poll_agents()` runs and processes the PTY echo
- `update_damage()` marks `FromLineToEnd(history_len)` which covers the viewport
- The render shows the echoed content

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport scroll subsystem's dirty region conversion. The fix aligns with the subsystem's design — `DirtyRegion` changes should flow from buffer mutations (terminal grid updates via `update_damage`), not from input handling.

No deviations discovered.

## Sequence

### Step 1: Remove premature FullViewport marking from paste handler

**Location**: `crates/editor/src/editor_state.rs` (lines ~1140-1151)

The current Cmd+V handler marks `DirtyRegion::FullViewport` immediately after writing to the PTY. This causes a render of stale content before the echo arrives.

**Change**: Remove the `self.dirty_region.merge(DirtyRegion::FullViewport)` line from the paste handler. Let the normal PTY polling flow mark dirty regions when output arrives.

```rust
// BEFORE (problematic):
Key::Char('v') | Key::Char('V') => {
    // Cmd+V: paste from clipboard
    if let Some(text) = crate::clipboard::paste_from_clipboard() {
        let modes = terminal.term_mode();
        let bytes = InputEncoder::encode_paste(&text, modes);
        if !bytes.is_empty() {
            let _ = terminal.write_input(&bytes);
        }
    }
    self.dirty_region.merge(DirtyRegion::FullViewport);  // REMOVE THIS
    return;
}

// AFTER (fixed):
Key::Char('v') | Key::Char('V') => {
    // Cmd+V: paste from clipboard
    // Chunk: docs/chunks/terminal_paste_render - Don't mark dirty before PTY echo
    if let Some(text) = crate::clipboard::paste_from_clipboard() {
        let modes = terminal.term_mode();
        let bytes = InputEncoder::encode_paste(&text, modes);
        if !bytes.is_empty() {
            let _ = terminal.write_input(&bytes);
        }
    }
    // No dirty marking here - let poll_agents() detect the PTY echo
    // and update_damage() mark the correct lines dirty.
    return;
}
```

### Step 2: Verify poll_agents flow handles paste correctly

**Location**: `crates/editor/src/main.rs` (handle_key)

The existing code already calls `poll_agents()` after every `handle_key`:

```rust
fn handle_key(&mut self, event: KeyEvent) {
    self.state.handle_key(event);
    // ... quit check ...

    // Chunk: docs/chunks/terminal_input_render_bug - Poll immediately after input
    let terminal_dirty = self.state.poll_agents();
    if terminal_dirty.is_dirty() {
        self.state.dirty_region.merge(terminal_dirty);
    }
    // ...
    self.render_if_dirty();
}
```

This should be sufficient. The `poll_agents()` → `poll_events()` → `update_damage()` chain will mark the viewport dirty when the echo arrives.

**Verification**: No code change needed here. Just confirm the flow is correct.

### Step 3: Handle potential multi-poll requirement for large pastes

For very large pastes, the shell may not have finished echoing all characters before the single `poll_agents()` call completes. The PTY wakeup mechanism (`handle_pty_wakeup`) will handle subsequent output, but there may be a brief visual gap.

**Observation**: This is acceptable behavior — the PTY wakeup fires within ~1ms of data arrival (per `terminal_pty_wakeup` chunk). For most paste operations, users won't notice the incremental appearance. If testing reveals issues with large pastes, we can add a small loop to drain all immediately-available PTY output.

**Decision**: No additional code change needed for now. Monitor in testing.

### Step 4: Add integration test for paste rendering

**Location**: `crates/terminal/tests/input_integration.rs`

Add a test that verifies paste content appears in the terminal buffer after polling:

```rust
/// Tests that pasted text appears in the terminal buffer after poll.
/// This validates the fix for terminal_paste_render - ensuring that
/// paste content doesn't render as blank spaces.
///
/// Chunk: docs/chunks/terminal_paste_render - Paste rendering test
#[test]
fn test_paste_content_appears_after_poll() {
    use lite_edit_buffer::BufferView;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    // Create a terminal with cat (echoes input)
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal.spawn_command("cat", &[], &PathBuf::from("/tmp")).unwrap();

    // Wait for cat to start
    thread::sleep(Duration::from_millis(50));
    terminal.poll_events();

    // Simulate paste (write bytes directly, no bracketed paste for simplicity)
    let paste_text = "hello world";
    terminal.write_input(paste_text.as_bytes()).unwrap();
    terminal.write_input(b"\n").unwrap(); // End with newline to complete the echo

    // Poll for output with timeout
    let mut found = false;
    for _ in 0..50 {
        if terminal.poll_events() {
            // Check if pasted text appears in buffer
            for line in 0..terminal.line_count() {
                if let Some(styled) = terminal.styled_line(line) {
                    let text: String = styled.spans.iter()
                        .map(|s| s.text.as_str())
                        .collect();
                    if text.contains("hello world") {
                        found = true;
                        break;
                    }
                }
            }
            if found { break; }
        }
        thread::sleep(Duration::from_millis(20));
    }

    assert!(found, "Pasted text 'hello world' should appear in terminal buffer after polling");
}
```

### Step 5: Manual verification

Run the editor and test paste behavior:

1. Open a terminal tab (`Cmd+Shift+T`)
2. Wait for shell prompt
3. Copy some text to clipboard (e.g., `echo hello`)
4. Press `Cmd+V` in the terminal tab
5. **Verify**: Text appears immediately (not as blank spaces)
6. Press Enter
7. **Verify**: Command executes correctly
8. Test with longer paste (a multi-line script)
9. **Verify**: All lines appear correctly

### Step 6: Update code_paths in GOAL.md

Update the chunk's GOAL.md frontmatter with the files modified:

```yaml
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/terminal/tests/input_integration.rs
```

## Dependencies

All dependencies are already complete (as indicated by `created_after` in GOAL.md frontmatter):
- `terminal_clipboard_selection` — Clipboard paste handling
- `terminal_tab_initial_render` — Terminal rendering infrastructure
- `terminal_background_box_drawing` — Terminal rendering
- `terminal_alt_backspace` — Input handling

No new libraries or infrastructure needed.

## Risks and Open Questions

### Large paste latency

For very large pastes (thousands of characters), the shell may not echo everything before the first render. The user might see incremental appearance of text. This is acceptable UX behavior — similar to how other terminals handle large pastes. The PTY wakeup mechanism ensures subsequent chunks appear promptly.

### Bracketed paste mode interaction

Shells with bracketed paste mode enabled (zsh with certain plugins) wrap paste in escape sequences. The test uses raw paste to `cat` which doesn't enable bracketed mode. If issues arise specifically with bracketed paste mode, additional investigation may be needed.

### Race condition window

There's theoretically a race between:
1. `write_input()` sending bytes to PTY
2. Shell processing and echoing
3. PTY reader thread capturing output
4. `poll_events()` draining the channel

In practice, step 1-3 happens fast enough that `poll_events()` catches most/all output. The PTY wakeup handles any stragglers. If testing reveals the race window is problematic, we could add a brief sleep or multiple poll iterations.

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