<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a **semantic bug fix** that addresses a missing integration between existing pieces. The core issue is that the terminal event polling loop (`poll_agents()`/`poll_standalone_terminals()`) is **never called** in the main event loop, so PTY output never reaches the terminal buffer for rendering.

### Root Cause Analysis

After code exploration, the following issues were identified:

1. **Missing PTY polling**: `EditorState::poll_agents()` exists and is documented, but `main.rs` never calls it. The cursor blink timer calls `toggle_cursor_blink()`, but that function doesn't poll terminals. Without polling, PTY output (shell prompts, command output) never gets processed.

2. **Input routing works correctly**: The `handle_key_buffer()` function correctly detects terminal tabs via `tab.terminal_and_viewport_mut()` and routes input through `InputEncoder::encode_key()` → `terminal.write_input()`. This part is working.

3. **Scroll routing works correctly**: The `handle_scroll()` function correctly handles terminal tabs, with primary screen viewport scrolling and alternate screen PTY passthrough.

4. **Rendering works correctly**: The renderer uses `Editor::active_buffer_view()` which correctly returns `TerminalBuffer` (implementing `BufferView`) for terminal tabs. The `render_with_editor()` → `update_glyph_buffer()` pipeline handles terminal content through the same path as file tabs.

### Fix Strategy

The fix is surgical: **integrate PTY polling into the existing timer callback**. The cursor blink timer fires every 500ms, which is also a reasonable polling interval for terminal output. Alternatively, we could add a separate higher-frequency timer for PTY polling, but using the existing timer keeps the implementation simple.

The `toggle_cursor_blink()` method in `EditorController` already handles cursor blink and picker updates. We'll extend it to also poll terminals.

### Why TDD is limited here

Per TESTING_PHILOSOPHY.md, terminal rendering involves platform code (Metal, NSWindow) that can't be unit tested. However, we can add integration tests that verify:
- PTY output appears in `TerminalBuffer` content after polling
- Key input reaches the PTY and produces echoed output

## Subsystem Considerations

No subsystems are directly relevant to this fix. The change integrates existing code paths that are already correctly designed—we're simply connecting the PTY polling loop to the main event loop.

## Sequence

### Step 1: Add PTY polling to the timer callback

**Location**: `crates/editor/src/main.rs` - `EditorController::toggle_cursor_blink()`

Modify the timer callback to poll terminal PTY events in addition to cursor blink and picker updates:

```rust
fn toggle_cursor_blink(&mut self) {
    // Toggle cursor blink (existing)
    let cursor_dirty = self.state.toggle_cursor_blink();
    if cursor_dirty.is_dirty() {
        self.state.dirty_region.merge(cursor_dirty);
    }

    // Chunk: docs/chunks/terminal_input_render_bug - Poll PTY events
    // Poll all agent and standalone terminal PTY events.
    // This processes shell output and updates TerminalBuffer content.
    let terminal_dirty = self.state.poll_agents();
    if terminal_dirty.is_dirty() {
        self.state.dirty_region.merge(terminal_dirty);
    }

    // Check for picker streaming updates (existing)
    let picker_dirty = self.state.tick_picker();
    if picker_dirty.is_dirty() {
        self.state.dirty_region.merge(picker_dirty);
    }

    // Render if anything is dirty (existing)
    self.render_if_dirty();
}
```

This ensures that every 500ms (the cursor blink interval), terminal output is processed and rendered.

### Step 2: Add PTY polling on key/mouse/scroll events

**Location**: `crates/editor/src/main.rs` - `EditorController::handle_key()`, `handle_mouse()`, `handle_scroll()`

When the user interacts with a terminal tab, we want immediate feedback. Add PTY polling to each input handler so that after sending input to the PTY, we immediately check for output:

```rust
fn handle_key(&mut self, event: KeyEvent) {
    self.state.handle_key(event);

    if self.state.should_quit {
        self.terminate_app();
        return;
    }

    // Chunk: docs/chunks/terminal_input_render_bug - Poll immediately after input
    // For terminal tabs, poll PTY output immediately after sending input
    // to ensure echoed characters appear without waiting for the next timer tick.
    let terminal_dirty = self.state.poll_agents();
    if terminal_dirty.is_dirty() {
        self.state.dirty_region.merge(terminal_dirty);
    }

    // ... rest of existing code (picker, render)
}
```

Apply the same pattern to `handle_mouse()` and `handle_scroll()`.

### Step 3: Add integration test for PTY input/output round-trip

**Location**: `crates/terminal/tests/input_integration.rs`

Add a test that verifies the end-to-end flow: write bytes to PTY stdin, poll for events, verify content appears in buffer.

```rust
#[test]
fn test_pty_input_output_roundtrip() {
    use std::path::PathBuf;
    use std::time::Duration;
    use std::thread;
    use lite_edit_buffer::BufferView;

    // Spawn a cat process that echoes input
    let mut terminal = TerminalBuffer::new(80, 24, 1000);
    terminal.spawn_command("cat", &[], &PathBuf::from("/tmp")).unwrap();

    // Write input
    terminal.write_input(b"hello\n").unwrap();

    // Poll until we see output (with timeout)
    let mut attempts = 0;
    while attempts < 50 {
        if terminal.poll_events() {
            // Check if "hello" appears in the buffer
            for line in 0..terminal.line_count() {
                if let Some(styled) = terminal.styled_line(line) {
                    if styled.text.contains("hello") {
                        return; // Success!
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(10));
        attempts += 1;
    }

    panic!("Did not see echoed input within timeout");
}
```

### Step 4: Add integration test for shell prompt visibility

**Location**: `crates/terminal/tests/integration.rs`

Add a test that spawns a shell and verifies the prompt appears:

```rust
#[test]
fn test_shell_prompt_appears() {
    use std::path::PathBuf;
    use std::time::Duration;
    use std::thread;
    use lite_edit_buffer::BufferView;

    let mut terminal = TerminalBuffer::new(80, 24, 1000);

    // Use /bin/sh as it's always available
    terminal.spawn_shell("/bin/sh", &PathBuf::from("/tmp")).unwrap();

    // Poll until we see a prompt ($ or #)
    let mut attempts = 0;
    while attempts < 100 {
        if terminal.poll_events() {
            for line in 0..terminal.line_count() {
                if let Some(styled) = terminal.styled_line(line) {
                    let text = &styled.text;
                    // Look for common shell prompt characters
                    if text.contains('$') || text.contains('#') || text.contains('%') {
                        return; // Success!
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
        attempts += 1;
    }

    panic!("No shell prompt appeared within timeout");
}
```

### Step 5: Update code_paths in GOAL.md

Update the chunk's GOAL.md frontmatter with the files modified:

```yaml
code_paths:
  - crates/editor/src/main.rs
  - crates/terminal/tests/input_integration.rs
  - crates/terminal/tests/integration.rs
```

### Step 6: Manual verification

Run the editor and verify all success criteria:
1. Press `Cmd+Shift+T` to open a terminal tab
2. Verify shell prompt appears (e.g., `$`, `%`, or `#`)
3. Type `ls` and press Enter — verify command output appears
4. Press `Ctrl+C` — verify it interrupts (no crash, prompt returns)
5. Scroll with trackpad (if there's scrollback content)
6. Switch to a file tab and back — verify terminal state is preserved

## Dependencies

All dependencies are already complete (as indicated in GOAL.md frontmatter):
- `terminal_input_encoding` — Input encoding is already working
- `terminal_scrollback_viewport` — Scroll handling is already working
- `renderer_polymorphic_buffer` — Rendering pipeline is already working

No new libraries or infrastructure needed.

## Risks and Open Questions

### Polling frequency trade-off

The current plan uses the 500ms cursor blink timer for PTY polling. This may feel sluggish for fast-typing users. If latency is noticeable:
- **Option A**: Add a separate higher-frequency timer (e.g., 16ms / 60Hz) for PTY polling only
- **Option B**: Use macOS's `kqueue` / `kevent` for event-driven PTY notification (more complex)

The immediate polling on input events (Step 2) should mitigate most perceived latency for interactive use. If issues persist, consider Option A as a follow-up.

### Test flakiness

Integration tests that spawn real processes (`cat`, `/bin/sh`) may be flaky on CI due to:
- Process startup time variability
- Resource contention
- Sandboxing restrictions

The tests use generous timeouts (500ms-2s) to reduce flakiness. If they still fail intermittently, consider:
- Using `BASH_ENV` or `ENV` to disable shell rc files
- Mocking at a higher level (not feasible given the goal is testing the real PTY integration)

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->