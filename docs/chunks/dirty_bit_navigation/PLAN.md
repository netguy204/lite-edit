<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is in `editor_state.rs` lines 1717-1728, where the `handle_key_buffer()` function uses `dirty_region.is_dirty()` as a heuristic to decide whether to set `tab.dirty = true`. This heuristic is flawed because `dirty_region` is a **rendering** concept — it tracks which screen regions need redrawing — not a **content mutation** concept.

Non-mutating operations (arrow keys, selection, scrolling) set `dirty_region` via `mark_cursor_dirty()` to trigger cursor/selection repaint, but they don't modify buffer content. The current code incorrectly treats this rendering-dirty state as evidence of content mutation.

**Fix strategy**: Track whether a **content-mutating** command was executed, rather than relying on the `dirty_region` heuristic.

Looking at `buffer_target.rs`, the `execute_command()` method already has clear separation:
- **Mutating commands** (InsertChar, Delete*, Paste, Cut) call `ctx.mark_dirty(dirty)` at the end of the function
- **Non-mutating commands** (Move*, Select*, Copy, PageUp/Down) use early `return` statements

The cleanest fix is to have `execute_command` return a boolean indicating whether a mutation occurred. This return value propagates up through `handle_key()` → `handle_key_buffer()`, where it gates the `tab.dirty = true` assignment.

This approach:
1. Is explicit about mutation vs navigation semantics
2. Doesn't require modifying the `FocusTarget` trait signature (which would be a breaking change)
3. Keeps the logic close to where the decision is made

Per TESTING_PHILOSOPHY.md, we'll write tests that verify semantic behavior:
- Navigation commands don't set `tab.dirty`
- Content-mutating commands do set `tab.dirty`

## Subsystem Considerations

No subsystems are directly relevant. The renderer subsystem (`docs/subsystems/renderer`) handles the humble view; this fix is in the state/update layer.

## Sequence

### Step 1: Add `content_mutated` flag to EditorContext

Add a `content_mutated: bool` field to `EditorContext` that focus targets can set when they perform a content mutation. Initialize to `false` at context creation. This is cleaner than modifying return types through the entire call chain.

Location: `crates/editor/src/context.rs`

Changes:
- Add `pub content_mutated: bool` field to `EditorContext` struct
- Initialize to `false` in constructor
- Add `pub fn set_content_mutated(&mut self)` helper method

### Step 2: Call `set_content_mutated()` for mutating commands in BufferFocusTarget

In `buffer_target.rs`, modify `execute_command()` to call `ctx.set_content_mutated()` whenever a content-mutating command is executed. The mutation commands are:
- `InsertChar`, `InsertNewline`, `InsertTab`
- `DeleteBackward`, `DeleteForward`, `DeleteBackwardWord`, `DeleteForwardWord`
- `DeleteToLineEnd`, `DeleteToLineStart`
- `Paste` (when there's content to paste)
- `Cut` (when there's a selection to cut)

Non-mutating commands that should **not** call this:
- All `Move*` commands (arrow keys, word jump, line/buffer start/end)
- All `Select*` commands (shift+arrow, select all)
- `Copy` (reads clipboard, doesn't modify buffer)
- `PageUp`, `PageDown` (viewport scrolling)

Location: `crates/editor/src/buffer_target.rs`

The call to `ctx.set_content_mutated()` should happen at the same point where `ctx.mark_dirty(dirty)` is already called (line ~507), and also in the early-return paths for `Paste` and `Cut` that mutate the buffer.

### Step 3: Replace dirty_region heuristic with content_mutated check

In `editor_state.rs`, replace the flawed heuristic at lines 1717-1728:

**Before:**
```rust
// Chunk: docs/chunks/unsaved_tab_tint - Mark file tab dirty if content changed
// If we processed a file tab and the dirty_region indicates changes, mark the tab dirty.
// This is a conservative heuristic: dirty_region can be set for cursor visibility or
// viewport scrolling, not just content mutations. We accept some over-marking because
// the success criteria only require that edits set dirty=true, which this achieves.
if is_file_tab && self.dirty_region.is_dirty() {
    if let Some(ws) = self.editor.active_workspace_mut() {
        if let Some(tab) = ws.active_tab_mut() {
            tab.dirty = true;
        }
    }
}
```

**After:**
```rust
// Chunk: docs/chunks/dirty_bit_navigation - Mark file tab dirty only for content mutations
// The EditorContext tracks whether a content-mutating command was executed.
// This correctly distinguishes mutations (insert, delete, paste, cut) from
// non-mutating operations (cursor movement, selection, scrolling) that also
// set dirty_region for rendering purposes.
if is_file_tab && ctx.content_mutated {
    if let Some(ws) = self.editor.active_workspace_mut() {
        if let Some(tab) = ws.active_tab_mut() {
            tab.dirty = true;
        }
    }
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 4: Write unit tests for non-mutating operations

Add tests to verify that navigation and selection commands don't set `tab.dirty`:

1. **test_arrow_key_navigation_does_not_set_dirty**: Arrow keys (up, down, left, right) don't set `tab.dirty`
2. **test_select_all_does_not_set_dirty**: Cmd+A doesn't set `tab.dirty`
3. **test_shift_arrow_selection_does_not_set_dirty**: Shift+arrow selection doesn't set `tab.dirty`
4. **test_word_jump_does_not_set_dirty**: Option+arrow word navigation doesn't set `tab.dirty`
5. **test_page_up_down_does_not_set_dirty**: Page up/down doesn't set `tab.dirty`

These tests should:
1. Create an EditorState with a file tab containing some text
2. Perform the navigation/selection operation
3. Assert `tab.dirty == false`

Location: `crates/editor/src/editor_state.rs` (in the existing `#[cfg(test)]` module)

### Step 5: Verify existing mutation tests still pass

Existing tests (`test_file_tab_dirty_after_edit`, `test_dirty_flag_cleared_on_save`, etc.) verify that content mutations correctly set `tab.dirty = true`. Run these to ensure the refactor doesn't break mutation tracking.

Location: `crates/editor/src/editor_state.rs`

### Step 6: Update code_paths in GOAL.md

Add the touched files to the chunk's code_paths frontmatter:
- `crates/editor/src/context.rs`
- `crates/editor/src/buffer_target.rs`
- `crates/editor/src/editor_state.rs`

Location: `docs/chunks/dirty_bit_navigation/GOAL.md`

## Dependencies

- Depends on `unsaved_tab_tint` chunk being ACTIVE (it introduced the `tab.dirty` flag wiring that this chunk fixes)
- The `unsaved_tab_tint` chunk's code_references document the flawed heuristic we're replacing

## Risks and Open Questions

1. **Other focus targets**: The fix is in `BufferFocusTarget.execute_command()`. If other focus targets (selector, find-in-file) also process key events that could mutate file tabs, they would need similar updates. However, examining the code, these focus targets operate on their own state (selector text, find query) rather than file buffer content.

2. **Paste edge case**: The `Paste` command only mutates if there's content in the clipboard. The current code already handles this correctly with a conditional check. We should ensure `set_content_mutated()` is only called when paste actually inserts content.

3. **Cut edge case**: Similarly, `Cut` only mutates if there's a selection. The conditional is already in place.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?
-->
