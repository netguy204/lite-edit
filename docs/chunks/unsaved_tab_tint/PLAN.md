<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk requires two parallel pieces of work:

1. **Wiring up the dirty flag**: The `Tab` struct already has a `dirty: bool` field, but no code sets it to `true`. We need to set `tab.dirty = true` after any buffer mutation in `handle_key_buffer` (character insert, delete, paste, etc.) and clear it back to `false` on successful save in `save_file()`.

2. **Adding dim red background tint**: When `is_dirty` is true, the tab bar should render with a very dim red tint instead of the normal `TAB_INACTIVE_COLOR` or `TAB_ACTIVE_COLOR`. This requires adding new color constants and modifying the tab bar rendering logic.

The key insight from codebase exploration:
- **Buffer mutations** flow through `BufferFocusTarget::execute_command()` in `buffer_target.rs`, which calls methods like `ctx.buffer.insert_char()`. These mutations return `DirtyLines` which is passed to `ctx.mark_dirty()`.
- **The EditorContext** only has access to the raw `TextBuffer`, not the `Tab` that owns it. So we cannot set `tab.dirty` from within `execute_command()`.
- **Instead**, we need to set the dirty flag at the `EditorState` level after `handle_key()` processes a key event that caused buffer mutations. The `EditorState` has access to both the `Editor` model (which contains tabs) and the `dirty_region`. If `dirty_region` indicates content changed (not just cursor movement), we mark the active tab dirty.

For the save path, `EditorState::save_file()` already writes the buffer content to disk. We simply add `tab.dirty = false` after a successful save.

Following TESTING_PHILOSOPHY.md's Humble View Architecture: the dirty flag logic and color selection are testable state transformations. The actual Metal rendering is the humble view and not unit-tested.

## Sequence

### Step 1: Add dirty tab background color constants

Add new color constants to `tab_bar.rs` for dirty tab backgrounds:
- `TAB_DIRTY_INACTIVE_COLOR`: Very dim red tint for inactive dirty tabs
- `TAB_DIRTY_ACTIVE_COLOR`: Very dim red tint for active dirty tabs

The tint should be subtle and consistent with the Catppuccin Mocha dark theme. Use the Catppuccin "red" color (#f38ba8) at very low opacity blended with the existing tab colors.

Location: `crates/editor/src/tab_bar.rs`

### Step 2: Modify tab bar rendering to use dirty colors

Update `TabBarGlyphBuffer::update()` to select the appropriate background color based on both active state and dirty state:
- Active + clean → `TAB_ACTIVE_COLOR`
- Active + dirty → `TAB_DIRTY_ACTIVE_COLOR`
- Inactive + clean → `TAB_INACTIVE_COLOR`
- Inactive + dirty → `TAB_DIRTY_INACTIVE_COLOR`

The existing yellow indicator dot logic (`DIRTY_INDICATOR_COLOR`) remains unchanged—it will now correctly appear since the flag will be wired up.

Location: `crates/editor/src/tab_bar.rs`

### Step 3: Set dirty flag after buffer mutations in EditorState

Modify `EditorState::handle_key()` to mark the active tab dirty after forwarding to the focus target, if the dirty_region indicates content changed (i.e., not `DirtyRegion::None` and not just a cursor line change from movement).

The logic:
1. Before calling `focus_target.handle_key()`, save a snapshot of `self.dirty_region`
2. After the call, check if `dirty_region` advanced (merged with new dirty lines)
3. If content was dirtied and the active tab is a file tab, set `tab.dirty = true`

To keep this simple and robust: check if `dirty_region != DirtyRegion::None` after the key is processed. This is a conservative heuristic that may over-mark (e.g., marking dirty on selection-only changes like Cmd+A). However, the success criteria only require that edits set dirty=true, which this achieves.

A more precise approach would be to track whether any mutating command was executed (InsertChar, DeleteBackward, Paste, etc.), but the dirty_region check is simpler and sufficient for this chunk.

Location: `crates/editor/src/editor_state.rs`

### Step 4: Clear dirty flag on save

Modify `EditorState::save_file()` to clear `tab.dirty = false` after successfully writing to disk.

Currently:
```rust
fn save_file(&mut self) {
    // ... validation ...
    let content = self.buffer().content();
    let _ = std::fs::write(&path, content.as_bytes());
    // Silently ignore write errors
}
```

Update to:
```rust
fn save_file(&mut self) {
    // ... validation ...
    let content = self.buffer().content();
    if std::fs::write(&path, content.as_bytes()).is_ok() {
        // Clear dirty flag on successful save
        if let Some(ws) = self.editor.active_workspace_mut() {
            if let Some(pane) = ws.active_pane_mut() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.dirty = false;
                }
            }
        }
    }
}
```

Location: `crates/editor/src/editor_state.rs`

### Step 5: Write unit tests for dirty flag behavior

Add tests verifying:
1. Editing a file buffer sets `tab.dirty = true` on the active tab
2. Saving a file clears `tab.dirty` back to `false`
3. Dirty tabs use the correct tinted background colors
4. Clean tabs continue to use their existing background colors

Per TESTING_PHILOSOPHY.md, focus on semantic assertions about the state:
- Test that inserting a character sets dirty=true
- Test that save clears dirty=false
- Test color selection logic in isolation

Location: `crates/editor/src/editor_state.rs` (dirty flag tests), `crates/editor/src/tab_bar.rs` (color tests)

### Step 6: Update GOAL.md code_paths

Add the touched files to the chunk's code_paths frontmatter:
- `crates/editor/src/tab_bar.rs`
- `crates/editor/src/editor_state.rs`

Location: `docs/chunks/unsaved_tab_tint/GOAL.md`

## Risks and Open Questions

1. **Over-marking dirty**: Using `dirty_region != None` as the heuristic may mark tabs dirty for non-content changes (like Cmd+A select-all which doesn't modify content). This is a conservative approach that errs on the side of showing the dirty indicator. The alternative—tracking specific mutating commands—adds complexity. Given the goal is "visual cue for unsaved changes," over-marking is acceptable.

2. **Red tint subtlety**: The exact RGB values for the dim red tint need visual tuning. Initial values will blend Catppuccin red (#f38ba8) at ~5-10% with the existing tab colors. May need adjustment after visual testing.

3. **Terminal tabs**: Terminal tabs don't have a "dirty" concept (they don't save to files). The existing code already skips setting dirty for non-file tabs (the mutation path is different). No additional work needed.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here:
- What changed?
- Why?
- What was the impact?
-->
