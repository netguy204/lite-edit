<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current architecture has the renderer owning a `TextBuffer` copy (`self.buffer: Option<TextBuffer>`) that is synced from the editor's active tab every frame via `sync_renderer_buffer`. This copy-and-sync pattern is:

1. **Wasteful**: Every frame reconstructs the entire buffer content
2. **Type-unsafe**: Calls `buffer()` which panics on terminal tabs
3. **Unnecessary**: `GlyphBuffer::update_from_buffer_with_wrap` already accepts `&dyn BufferView`

The fix is to make the renderer **stateless with respect to buffer content**. Instead of owning a buffer copy, it receives a `&dyn BufferView` reference at render time from the active tab. This works because:

- `TextBuffer` implements `BufferView` (file tabs)
- `TerminalBuffer` implements `BufferView` (terminal tabs)
- The renderer's `GlyphBuffer` methods already accept `&dyn BufferView`

**Strategy:**

1. Remove `self.buffer: Option<TextBuffer>` from `Renderer`
2. Delete all buffer-copy methods (`set_buffer`, `buffer`, `buffer_mut`, `sync_renderer_buffer`)
3. Modify `update_glyph_buffer` to accept `&dyn BufferView` as a parameter
4. Modify `render_with_editor` to fetch the active tab's `BufferView` from `Editor` and pass it through
5. Handle the `AgentTerminal` placeholder variant by routing to `Workspace::agent_terminal()`

The testing philosophy emphasizes testing behavior at boundaries. The key boundary here is the polymorphic dispatch: we need to verify that both `TextBuffer` and `TerminalBuffer` can be rendered through the same code path.

## Sequence

### Step 1: Add helper method to Editor for active BufferView

Add a method to `Editor` that returns `Option<&dyn BufferView>` for the currently active tab. This handles the `AgentTerminal` placeholder by delegating to `Workspace::agent_terminal()`.

Location: `crates/editor/src/workspace.rs`

```rust
impl Editor {
    /// Returns a reference to the active tab's BufferView.
    ///
    /// Returns `None` if there is no active workspace or tab.
    /// Handles AgentTerminal placeholder by delegating to workspace agent.
    pub fn active_buffer_view(&self) -> Option<&dyn BufferView> {
        let workspace = self.active_workspace()?;
        let tab = workspace.active_tab()?;

        if tab.buffer.is_agent_terminal() {
            // AgentTerminal is a placeholder - get the actual buffer from workspace
            workspace.agent_terminal().map(|t| t as &dyn BufferView)
        } else {
            Some(tab.buffer())
        }
    }
}
```

### Step 2: Remove buffer field from Renderer

Remove the `buffer: Option<TextBuffer>` field from the `Renderer` struct and all methods that operate on it:

- Remove `buffer` field
- Remove `set_buffer(&mut self, buffer: TextBuffer)`
- Remove `buffer_mut(&mut self) -> Option<&mut TextBuffer>`
- Remove `buffer(&self) -> Option<&TextBuffer>`

Location: `crates/editor/src/renderer.rs`

### Step 3: Refactor update_glyph_buffer to accept BufferView

Change `update_glyph_buffer` from a method that reads `self.buffer` to one that accepts `&dyn BufferView` as a parameter. The method signature becomes:

```rust
fn update_glyph_buffer(&mut self, view: &dyn BufferView)
```

The body replaces `self.buffer` references with the `view` parameter:
- `buffer.line_count()` → `view.line_count()`
- The call to `update_from_buffer_with_wrap` already takes `&dyn BufferView`

Location: `crates/editor/src/renderer.rs`

### Step 4: Update render_with_editor to thread BufferView

Modify `render_with_editor` to:

1. Call `editor.active_buffer_view()` to get `Option<&dyn BufferView>`
2. Only call `update_glyph_buffer(view)` if a view exists
3. Proceed with rendering even if no buffer view (empty viewport is valid)

The pattern becomes:
```rust
if let Some(view) = editor.active_buffer_view() {
    self.update_glyph_buffer(view);
}
```

Location: `crates/editor/src/renderer.rs`

### Step 5: Update other render methods

Update these methods to follow the same pattern:
- `render_dirty`: Currently calls `update_glyph_buffer()` unconditionally; needs to accept or retrieve a BufferView
- `render_with_selector`: Currently calls `update_glyph_buffer()` unconditionally
- `render_with_find_strip`: Currently calls `update_glyph_buffer()` unconditionally
- `render`: Currently calls `update_glyph_buffer()` unconditionally

For methods that don't receive `editor: &Editor`, we have two options:
1. Pass the `BufferView` as a parameter
2. Make those methods no-op when no buffer view (they're used for simpler rendering paths)

Since `render_with_editor` is the primary entry point used by `main.rs`, and the other methods appear to be legacy or specialized, we'll:
- Keep `render_dirty` working for the selector overlay path
- Remove calls that assume `self.buffer` exists

Location: `crates/editor/src/renderer.rs`

### Step 6: Update apply_mutation method

The `apply_mutation` method currently uses `self.buffer` to get line count:

```rust
pub fn apply_mutation(&self, dirty_lines: &DirtyLines) -> DirtyRegion {
    if let Some(buffer) = &self.buffer {
        self.viewport.dirty_lines_to_region(dirty_lines, buffer.line_count())
    } else {
        DirtyRegion::None
    }
}
```

This needs to accept a line count parameter instead:

```rust
pub fn apply_mutation(&self, dirty_lines: &DirtyLines, line_count: usize) -> DirtyRegion {
    self.viewport.dirty_lines_to_region(dirty_lines, line_count)
}
```

Location: `crates/editor/src/renderer.rs`

### Step 7: Delete sync_renderer_buffer from main.rs

Remove the `sync_renderer_buffer` method from `EditorController` in `main.rs`. This method is no longer needed since the renderer doesn't own a buffer copy.

Update `render_if_dirty` to no longer call `sync_renderer_buffer`. The buffer synchronization is now implicit via `editor.active_buffer_view()` at render time.

Location: `crates/editor/src/main.rs`

### Step 8: Remove buffer initialization in main.rs

In `setup_window`, remove the lines that:
1. Create a `TextBuffer::from_str(&demo_content)` for the renderer
2. Call `renderer.set_buffer(initial_buffer)`

The renderer no longer needs an initial buffer.

Location: `crates/editor/src/main.rs`

### Step 9: Update imports

Remove unused imports:
- `lite_edit_buffer::TextBuffer` from renderer.rs (if no longer needed)
- Any imports related to buffer copying in main.rs

Location: `crates/editor/src/renderer.rs`, `crates/editor/src/main.rs`

### Step 10: Verify compilation and fix any remaining issues

Run `cargo build` and `cargo test` to identify any remaining compilation errors from the refactoring. Common issues to watch for:

- Methods that previously assumed `self.buffer` exists
- Test code that used `renderer.buffer()` or `renderer.set_buffer()`
- Lifetime issues with `&dyn BufferView` references

### Step 11: Manual testing

Test the following scenarios manually:

1. **Start editor normally**: Opens with demo content, renders correctly
2. **Spawn terminal tab** (`Cmd+Shift+T`): No crash, terminal renders (may be blank initially)
3. **Type in terminal tab**: No crash in render path
4. **Switch to file tab** (`Cmd+1`): Renders file content correctly
5. **Switch back to terminal tab**: Renders terminal content correctly

## Dependencies

This chunk depends on:
- `terminal_active_tab_safety` (ACTIVE): Provides the safe accessors and guards that prevent panics when the active tab is a terminal tab. This chunk builds on that foundation by making the render path itself polymorphic.

The dependency is already declared in the GOAL.md frontmatter.

## Risks and Open Questions

1. **AgentTerminal placeholder handling**: The `TabBuffer::AgentTerminal` variant is a placeholder that panics if you call `as_buffer_view()`. The implementation must route through `Workspace::agent_terminal()` instead. The Step 1 helper method handles this.

2. **Render method signatures**: Several render methods (`render`, `render_dirty`, `render_with_selector`, `render_with_find_strip`) don't receive `&Editor`. We need to decide whether to:
   - Pass `&dyn BufferView` to each
   - Pass `&Editor` to each
   - Make them work without a buffer view (empty render)

   The plan opts for option 1 for `render_with_editor` (the main path) and considers others as legacy/specialized.

3. **Performance**: Fetching `active_buffer_view()` every frame is O(1) (just array indexing). This is negligible compared to the previous approach of copying entire buffer content.

4. **Lifetime issues**: The `&dyn BufferView` reference must not outlive the render call. Since rendering is synchronous and the view reference is used only within the method, this should be fine.

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