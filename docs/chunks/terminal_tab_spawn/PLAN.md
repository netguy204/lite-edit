<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Wire `Cmd+Shift+T` to spawn a standalone terminal tab using the existing
infrastructure from `terminal_emulator` and `agent_lifecycle` chunks:

1. **Keybinding**: Add a handler in `EditorState::handle_key` for `Cmd+Shift+T`
   that calls a new `new_terminal_tab()` method.

2. **Terminal creation**: The `new_terminal_tab()` method will:
   - Compute terminal dimensions (cols × rows) from the current viewport size
     and font metrics
   - Create a `TerminalBuffer` with those dimensions
   - Spawn the user's default shell (`$SHELL` or `/bin/sh`)
   - Generate a label ("Terminal", "Terminal 2", etc.) based on existing
     terminal tab count
   - Create a `Tab::new_terminal()` and add it to the active workspace

3. **Label numbering**: Count existing terminal tabs in the workspace to
   generate unique labels. First terminal is "Terminal", subsequent ones are
   "Terminal 2", "Terminal 3", etc.

4. **Testing strategy**: Following the testing philosophy, we'll write tests
   that verify:
   - `Cmd+Shift+T` creates a terminal tab (update the existing "does nothing"
     test)
   - Multiple presses create multiple terminals with sequential labels
   - The new tab becomes active

The implementation reuses `Tab::new_terminal()` from workspace.rs (created in
`terminal_emulator` chunk) and `TerminalBuffer::spawn_shell()` from the
terminal crate.

## Subsystem Considerations

No subsystems exist yet. This chunk doesn't warrant creating one — it's a
small, focused feature wiring together existing components.

## Sequence

### Step 1: Write failing tests for Cmd+Shift+T

Update the existing `test_cmd_shift_t_does_not_create_tab` test to verify the
new behavior, and add new tests:

**Test 1: `test_cmd_shift_t_creates_terminal_tab`**
- Press `Cmd+Shift+T` on an empty editor state
- Assert the workspace now has 2 tabs
- Assert the active tab has `kind == TabKind::Terminal`
- Assert the tab label is "Terminal"

**Test 2: `test_cmd_shift_t_multiple_terminals_numbered`**
- Press `Cmd+Shift+T` twice
- Assert the workspace has 3 tabs
- Assert the first terminal's label is "Terminal"
- Assert the second terminal's label is "Terminal 2"

**Test 3: `test_cmd_shift_t_does_not_insert_t`**
- Ensure the keystroke doesn't insert 'T' into any buffer

Location: `crates/editor/src/editor_state.rs` (test module)

### Step 2: Add helper to count terminal tabs

Add a private helper method to count existing terminal tabs in the active
workspace. This is used to generate the numbered label.

```rust
/// Counts existing terminal tabs in the active workspace.
/// Returns 0 if no workspace is active.
fn terminal_tab_count(&self) -> usize
```

Location: `crates/editor/src/editor_state.rs` (impl EditorState)

### Step 3: Add `new_terminal_tab()` method

Add the core method that creates a terminal tab:

```rust
// Chunk: docs/chunks/terminal_tab_spawn - Cmd+Shift+T terminal spawning
/// Creates a new standalone terminal tab in the active workspace.
///
/// The terminal runs the user's default shell from `$SHELL`, falling back
/// to `/bin/sh`. Terminal dimensions are computed from the current viewport
/// size and font metrics.
///
/// Terminal tabs are labeled "Terminal", "Terminal 2", etc. based on how
/// many terminal tabs already exist in the workspace.
pub fn new_terminal_tab(&mut self)
```

Implementation details:
1. Compute content area dimensions:
   - `content_height = view_height - TAB_BAR_HEIGHT`
   - `content_width = view_width - RAIL_WIDTH`
2. Compute terminal dimensions:
   - `rows = (content_height / font_metrics.line_height).floor() as usize`
   - `cols = (content_width / font_metrics.advance_width).floor() as usize`
3. Create `TerminalBuffer::new(cols, rows, 5000)` (5000 scrollback lines)
4. Get shell from `std::env::var("SHELL")` or default to `/bin/sh`
5. Get working directory from workspace's `root_path` (or current directory)
6. Call `terminal.spawn_shell(&shell, &cwd)`
7. Generate label using `terminal_tab_count()`:
   - 0 existing terminals → "Terminal"
   - n existing terminals → "Terminal {n+1}"
8. Create `Tab::new_terminal(tab_id, terminal, label, line_height)`
9. Add to workspace via `workspace.add_tab(tab)`
10. Sync viewport and mark dirty

Location: `crates/editor/src/editor_state.rs` (impl EditorState)

### Step 4: Wire keybinding in handle_key

Add the `Cmd+Shift+T` handler in `handle_key()`, near the existing `Cmd+T`
handler:

```rust
// Chunk: docs/chunks/terminal_tab_spawn - Create new terminal tab
// Cmd+Shift+T creates a new terminal tab
if let Key::Char('t') = event.key {
    if event.modifiers.shift {
        self.new_terminal_tab();
        return;
    }
}
```

Location: `crates/editor/src/editor_state.rs`, in `handle_key()` where `Cmd+T`
is handled

### Step 5: Add standalone terminal polling

Currently `poll_agents()` only polls the workspace's agent. Standalone terminal
tabs need their PTY events polled too.

Add to `Workspace`:

```rust
// Chunk: docs/chunks/terminal_tab_spawn - Poll standalone terminals
/// Polls PTY events for all standalone terminal tabs.
///
/// Returns true if any terminal had output.
pub fn poll_standalone_terminals(&mut self) -> bool {
    let mut had_events = false;
    for tab in &mut self.tabs {
        if let Some(terminal) = tab.buffer.as_terminal_buffer_mut() {
            if terminal.poll_events() {
                had_events = true;
            }
        }
    }
    had_events
}
```

Update `poll_agent()` to also call `poll_standalone_terminals()`, or add a new
method that polls both.

Location: `crates/editor/src/workspace.rs` and `crates/editor/src/editor_state.rs`

### Step 6: Verify tests pass

Run the tests to verify all criteria are met:
- `cargo test --package lite-edit-editor test_cmd_shift_t`
- Verify no regressions in related tests:
  - `cargo test --package lite-edit-editor test_cmd_t`
  - `cargo test --package lite-edit-editor test_new_tab`

---

**BACKREFERENCE COMMENTS**

Add backreference comments to new methods:
- `new_terminal_tab()`: `// Chunk: docs/chunks/terminal_tab_spawn`
- Keybinding handler: `// Chunk: docs/chunks/terminal_tab_spawn`
- `poll_standalone_terminals()`: `// Chunk: docs/chunks/terminal_tab_spawn`

## Dependencies

- **terminal_emulator**: Provides `TerminalBuffer`, `Tab::new_terminal()`, and
  the PTY spawning infrastructure. (Status: ACTIVE - already implemented)
- **agent_lifecycle**: Provides the `TabBuffer::Terminal` variant and workspace
  integration. (Status: ACTIVE - already implemented)

Both dependencies are complete. The necessary types are:
- `lite_edit_terminal::TerminalBuffer` - terminal emulator
- `crate::workspace::Tab::new_terminal()` - terminal tab constructor
- `crate::workspace::TabKind::Terminal` - tab kind enum variant

## Risks and Open Questions

**Low risk:**

1. **Shell spawning failure**: If `$SHELL` points to a non-existent binary,
   `spawn_shell()` will return an error. We should handle this gracefully —
   possibly log the error and still create the tab (showing a "shell failed to
   start" message in the terminal buffer is fine).

2. **Zero-dimension edge case**: If `view_height` or `view_width` haven't been
   set yet (initial state), terminal dimensions could be zero. The terminal
   emulator handles this, but we should guard against creating a 0×0 terminal.
   Minimum dimensions: 1 col × 1 row (or skip creation entirely).

**Requires additional work:**

3. **Standalone terminal polling**: Currently `poll_agents()` only polls the
   `Workspace.agent` field, not individual terminal tabs. Standalone terminals
   stored in `TabBuffer::Terminal` won't receive PTY events without adding a
   polling mechanism.

   **Proposed solution**: Add a `poll_standalone_terminals()` method to
   `Workspace` that iterates over tabs, finds `TabBuffer::Terminal` variants,
   and calls `terminal.poll_events()` on each. Call this from `poll_agents()`
   or from a new `poll_all_terminals()` method in `EditorState`.

   This is a minor addition but necessary for the terminals to actually work.
   Add as Step 6 in the sequence.

## Deviations

*To be populated during implementation.*