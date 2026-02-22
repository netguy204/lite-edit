<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk fixes two bugs in workspace switching:

1. **Y-coordinate flip bug in left rail hit-testing**: The `handle_mouse` method in
   `EditorState` passes raw NSView coordinates (y=0 at bottom) to
   `calculate_left_rail_geometry`, which produces tile rects in top-down screen
   space (y=0 at top, starts at `TOP_MARGIN`). The fix is to flip the y-coordinate
   before hit-testing: `let flipped_y = self.view_height - mouse_y as f32`.

2. **Missing Cmd+[/] workspace cycling shortcuts**: Following the existing pattern
   for `Cmd+Shift+[/]` tab cycling (`prev_tab`/`next_tab`), we add `Cmd+[` and
   `Cmd+]` (without Shift) to cycle through workspaces.

Both fixes are straightforward and follow existing patterns in the codebase:
- The y-flip pattern is already used in `handle_mouse_selector` (line 1032)
- The bracket key handling is already in place for tab cycling (lines 373-387)
- The `prev_tab`/`next_tab` pattern (lines 1518-1540) is directly applicable

Tests will follow TDD per TESTING_PHILOSOPHY.md: write failing tests first, then
implement the fix, then verify the tests pass.

## Subsystem Considerations

No subsystems are relevant to this chunk. This is a localized bug fix in
`editor_state.rs` that doesn't touch cross-cutting patterns.

## Sequence

### Step 1: Add unit test for y-coordinate flip in left rail hit-testing

Write a test that verifies clicking on a workspace tile correctly switches
workspaces. The test should:
- Create an EditorState with multiple workspaces
- Set a known view_height
- Simulate a mouse click at coordinates that should hit workspace 1 (not 0)
- Assert that the active workspace changes

The test will fail initially because the y-coordinate is not being flipped.

Location: `crates/editor/src/editor_state.rs` in the `#[cfg(test)]` module

### Step 2: Fix the y-coordinate flip bug in handle_mouse

In `EditorState::handle_mouse`, before checking `tile_rect.contains()`, flip the
y-coordinate to convert from NSView coordinates (bottom-left origin) to the
top-down screen space used by `calculate_left_rail_geometry`:

```rust
// Current (buggy):
if tile_rect.contains(mouse_x as f32, mouse_y as f32) {

// Fixed:
let flipped_y = self.view_height - mouse_y as f32;
if tile_rect.contains(mouse_x as f32, flipped_y) {
```

Location: `crates/editor/src/editor_state.rs`, in `handle_mouse` method around
lines 973-974

### Step 3: Add unit tests for prev_workspace and next_workspace methods

Write tests for workspace cycling:
- `test_next_workspace_cycles_forward` - cycles 0→1→2→0
- `test_prev_workspace_cycles_backward` - cycles 2→1→0→2
- `test_next_workspace_single_workspace_is_noop` - no change with 1 workspace
- `test_prev_workspace_single_workspace_is_noop` - no change with 1 workspace

These tests mirror the existing `test_next_tab_cycles_forward` and
`test_prev_tab_cycles_backward` tests.

Location: `crates/editor/src/editor_state.rs` in the `#[cfg(test)]` module

### Step 4: Implement prev_workspace and next_workspace methods

Add two new public methods to `EditorState`:

```rust
/// Cycles to the next workspace (wraps from last to first).
///
/// Does nothing if there's only one workspace.
pub fn next_workspace(&mut self) {
    let count = self.editor.workspace_count();
    if count > 1 {
        let next = (self.editor.active_workspace + 1) % count;
        self.switch_workspace(next);
    }
}

/// Cycles to the previous workspace (wraps from first to last).
///
/// Does nothing if there's only one workspace.
pub fn prev_workspace(&mut self) {
    let count = self.editor.workspace_count();
    if count > 1 {
        let prev = if self.editor.active_workspace == 0 {
            count - 1
        } else {
            self.editor.active_workspace - 1
        };
        self.switch_workspace(prev);
    }
}
```

Location: `crates/editor/src/editor_state.rs`, near the existing `switch_workspace`
method (around line 1439)

### Step 5: Add keyboard shortcut tests for Cmd+[ and Cmd+]

Write tests that verify the keyboard shortcuts trigger workspace cycling:
- `test_cmd_right_bracket_next_workspace` - Cmd+] cycles to next workspace
- `test_cmd_left_bracket_prev_workspace` - Cmd+[ cycles to previous workspace

These tests mirror `test_cmd_shift_right_bracket_next_tab` and
`test_cmd_shift_left_bracket_prev_tab`.

Location: `crates/editor/src/editor_state.rs` in the `#[cfg(test)]` module

### Step 6: Add Cmd+[ and Cmd+] keyboard shortcuts

In `handle_key`, add handlers for `Cmd+[` and `Cmd+]` (without Shift) to call
`prev_workspace` and `next_workspace`. Add these right after the existing
`Cmd+Shift+[/]` handlers:

```rust
// Cmd+] (without Shift) cycles to next workspace
if let Key::Char(']') = event.key {
    if !event.modifiers.shift {
        self.next_workspace();
        return;
    }
}

// Cmd+[ (without Shift) cycles to previous workspace
if let Key::Char('[') = event.key {
    if !event.modifiers.shift {
        self.prev_workspace();
        return;
    }
}
```

Location: `crates/editor/src/editor_state.rs`, in `handle_key` after the
`Cmd+Shift+[/]` handlers (around line 387)

### Step 7: Run tests and verify

Run `cargo test` to verify all new and existing tests pass. Specifically:
- All new workspace switching tests pass
- Existing tab cycling tests (`test_cmd_shift_right_bracket_next_tab`, etc.) still pass
- Existing workspace tests still pass

## Dependencies

This chunk depends on the `workspace_model` chunk, which is already ACTIVE.
No external dependencies need to be added.

## Risks and Open Questions

- **Keyboard shortcut conflict**: Cmd+[ and Cmd+] may conflict with other macOS
  text editing conventions. However, these are standard browser/IDE shortcuts for
  navigation (back/forward in Safari, indent/outdent in some editors), so using
  them for workspace cycling is a reasonable choice. The existing Cmd+Shift+[/]
  shortcuts for tab cycling suggest this pattern is intentional.

- **Y-flip correctness verification**: The fix assumes `view_height` is always
  correctly updated before `handle_mouse` is called. This is already the case for
  other y-flip operations in the codebase (e.g., `handle_mouse_selector`).
