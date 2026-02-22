# Implementation Plan

## Approach

The root cause is clear from the investigation: terminal tab viewports are created with `visible_rows=0`, and `sync_active_tab_viewport` explicitly skips non-file tabs. When `poll_standalone_terminals` calls `scroll_to_bottom` with `visible_rows=0`, it computes `max_offset = line_count * line_height`, scrolling the viewport past all content.

The fix has two parts:

1. **Initialize viewport dimensions in `new_terminal_tab`**: After creating and adding the terminal tab, call `viewport.update_size(content_height, line_count)` on the new tab's viewport. The `content_height` and terminal `line_count` are already known at this point (rows is computed from `content_height / line_height`).

2. **Remove the spin-poll workaround**: Delete `pending_terminal_created` flag, `spin_poll_terminal_startup` method, and the call site in `main.rs`. The existing PTY wakeup mechanism (`dispatch_async` → `handle_pty_wakeup`) already handles async shell output rendering correctly.

This follows the existing viewport initialization pattern used by `update_viewport_dimensions` during window resize.

## Sequence

### Step 1: Add viewport initialization to `new_terminal_tab`

After the tab is added to the workspace (`workspace.add_tab(new_tab)`), get a mutable reference to the active tab's viewport and call `update_size()` with the content height and terminal line count.

**Location**: `crates/editor/src/editor_state.rs` in `new_terminal_tab()` (around line 2180-2185)

**Change**:
```rust
// After: workspace.add_tab(new_tab);
// Get the newly added tab's viewport and initialize its dimensions
// so scroll_to_bottom computes correct offsets
if let Some(workspace) = self.editor.active_workspace_mut() {
    if let Some(tab) = workspace.active_tab_mut() {
        let line_count = tab.buffer().line_count();
        tab.viewport.update_size(content_height, line_count);
    }
}
```

**Note**: Must do this AFTER `add_tab` because `add_tab` switches to the new tab. The `content_height` variable is already computed earlier in the function.

### Step 2: Remove `pending_terminal_created` field from `EditorState`

Remove the field declaration and initialization.

**Location**: `crates/editor/src/editor_state.rs`
- Remove field at line 122: `pending_terminal_created: bool,`
- Remove initialization at line 293: `pending_terminal_created: false,`

### Step 3: Remove `spin_poll_terminal_startup` method

Delete the entire method (~20 lines).

**Location**: `crates/editor/src/editor_state.rs`, lines 1674-1695

### Step 4: Remove the flag-setting line in `new_terminal_tab`

Remove the line that sets `pending_terminal_created = true`.

**Location**: `crates/editor/src/editor_state.rs`, line 2194 (approximately, after Step 1 changes)

### Step 5: Remove spin-poll call site in `main.rs`

Remove the call to `spin_poll_terminal_startup()` and the surrounding code block.

**Location**: `crates/editor/src/main.rs`, lines 256-263

**Remove**:
```rust
// Chunk: docs/chunks/terminal_tab_initial_render - Deferred PTY poll for initial content
// When a terminal tab was just created, spin-poll to capture the shell's
// initial prompt output before rendering. This gives the shell up to 100ms
// to start and produce its prompt.
let startup_dirty = self.state.spin_poll_terminal_startup();
if startup_dirty.is_dirty() {
    self.state.dirty_region.merge(startup_dirty);
}
```

### Step 6: Update chunk backreference comment

Change the backreference comment in `new_terminal_tab` from the parent chunk to this chunk.

**Location**: `crates/editor/src/editor_state.rs` in `new_terminal_tab()`

**Change**: Replace the existing `terminal_tab_initial_render` backreference comments with `terminal_viewport_init` backreferences.

### Step 7: Write a test for viewport initialization

Add a test that verifies the terminal viewport has correct `visible_rows` immediately after creation.

**Location**: `crates/editor/src/editor_state.rs`, in the `#[cfg(test)]` module

**Test**: Create a terminal tab and assert that `viewport.visible_lines() > 0` immediately after creation (before any polling or resizing). This validates the root fix.

### Step 8: Run existing tests

Run `cargo test -p lite-edit` to ensure:
- All existing terminal tests pass
- No regressions in file tab behavior
- The viewport initialization doesn't break other functionality

### Step 9: Manual verification

Test manually:
1. Launch the editor
2. Press Cmd+Shift+T to create a terminal tab
3. Verify the shell prompt appears immediately without requiring a window resize
4. Verify no flicker or double-render artifacts
5. Test terminal input/output, scrollback, and resize behavior

## Risks and Open Questions

1. **Timing of `active_tab_mut()` after `add_tab()`**: Need to verify that `add_tab()` immediately switches the active tab index so `active_tab_mut()` returns the newly added terminal tab. Review of `Workspace::add_tab` confirms it sets `self.active_tab = new_index`.

2. **Content height reuse**: The `content_height` variable is computed at the start of `new_terminal_tab()`. We reuse it for viewport initialization. This should be fine since no resize can occur during `new_terminal_tab()` execution (single-threaded main loop).

3. **Test adaptation**: The existing test `test_terminal_viewport_is_at_bottom_initial` may need updating since the comment says "viewport is uninitialized (visible_lines=0) with scroll_offset=0" which will no longer be true after this fix.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->