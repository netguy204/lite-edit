<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk adds base content snapshot tracking to `Tab` and implements automatic reload for clean (unmodified) buffers when the file changes on disk. The implementation builds on the `file_change_events` chunk, which already:

1. Routes `Modify(Data(Content))` events from the filesystem watcher to `EditorEvent::FileChanged(PathBuf)`
2. Provides self-write suppression via `file_change_suppression.suppress()`
3. Has a placeholder `handle_file_changed()` handler in the drain loop

The approach follows the investigation findings (`docs/investigations/concurrent_edit_sync/OVERVIEW.md`):

- **H4**: Storing `base_content` as a plain `String` is memory-acceptable for typical source files (< 1MB each)
- The base snapshot is the "common ancestor" needed for three-way merge (next chunk)
- Clean buffers (`dirty == false`) reload silently; dirty buffers defer to the three-way merge chunk

**Key design decisions:**

1. **`base_content` field location**: Added to `Tab` struct directly, alongside `associated_file` and `dirty`
2. **When to set `base_content`**:
   - At file load (`associate_file()`) — set to file contents read from disk
   - At file save (`save_file()`) — set to buffer contents written to disk
3. **Reload behavior for clean buffers**:
   - Re-read file from disk
   - Replace buffer content via `TextBuffer::from_str()`
   - Update `base_content` to the new disk content
   - Preserve cursor position if still valid (clamp to buffer bounds if needed)
   - Re-apply syntax highlighting
   - Mark `DirtyRegion::FullViewport` dirty

**Testing approach (per TESTING_PHILOSOPHY.md):**

- Unit tests for cursor position preservation/clamping logic (pure data manipulation)
- Integration testing via manual verification (filesystem events are flaky in CI)
- The `base_content` field is internal state; test its effects (reload behavior) not its existence

## Subsystem Considerations

No existing subsystems are relevant to this chunk. The chunk establishes the foundation for concurrent-edit-sync infrastructure but doesn't touch any documented cross-cutting patterns in `docs/subsystems/`.

## Sequence

### Step 1: Add `base_content` field to `Tab` struct

Add a new field to the `Tab` struct in `crates/editor/src/workspace.rs`:

```rust
/// The file content as last known on disk.
///
/// Populated when a file is loaded (`associate_file()`) and when a file
/// is saved (`save_file()`). Used as the base version for three-way merge
/// when the file changes on disk while the buffer has unsaved edits.
///
/// `None` for tabs that have never been associated with a file, or for
/// terminal tabs.
// Chunk: docs/chunks/base_snapshot_reload - Base version tracking for merge
pub base_content: Option<String>,
```

Initialize to `None` in `Tab::new()`, `Tab::new_terminal()`, and `Tab::new_welcome()`.

Location: `crates/editor/src/workspace.rs`

### Step 2: Set `base_content` in `associate_file()`

Modify `EditorState::associate_file()` to store the loaded file content in `base_content`.

After successfully reading the file and creating the `TextBuffer`, set:

```rust
// Store base content snapshot for three-way merge
// Chunk: docs/chunks/base_snapshot_reload - Populate base on load
if let Some(ws) = self.editor.active_workspace_mut() {
    if let Some(tab) = ws.active_tab_mut() {
        tab.base_content = Some(contents.to_string());
    }
}
```

This should happen inside the `Ok(bytes)` branch after the buffer is created.

Location: `crates/editor/src/editor_state.rs` (in `associate_file()`)

### Step 3: Set `base_content` in `save_file()`

Modify `EditorState::save_file()` to update `base_content` after a successful save.

After `std::fs::write()` succeeds and before clearing the dirty flag, set:

```rust
// Update base content snapshot to match saved content
// Chunk: docs/chunks/base_snapshot_reload - Populate base on save
tab.base_content = Some(content.clone());
```

Note: `content` is already bound to `self.buffer().content()` earlier in the function.

Location: `crates/editor/src/editor_state.rs` (in `save_file()`)

### Step 4: Add helper method to find tab by path

Add a helper method to `Workspace` to find a tab by its associated file path:

```rust
/// Find a tab by its associated file path.
///
/// Returns `None` if no tab has the given path, or if the path doesn't match
/// any open tab's `associated_file`.
// Chunk: docs/chunks/base_snapshot_reload - Tab lookup for file change handling
pub fn find_tab_by_path(&self, path: &Path) -> Option<TabId> {
    for tab in &self.tabs {
        if let Some(ref associated) = tab.associated_file {
            if associated == path {
                return Some(tab.id);
            }
        }
    }
    None
}
```

Also add a mutable variant:

```rust
/// Find a mutable tab by its associated file path.
// Chunk: docs/chunks/base_snapshot_reload - Tab lookup for file change handling
pub fn find_tab_mut_by_path(&mut self, path: &Path) -> Option<&mut Tab> {
    self.tabs.iter_mut().find(|tab| {
        tab.associated_file.as_ref().map(|p| p == path).unwrap_or(false)
    })
}
```

Location: `crates/editor/src/workspace.rs`

### Step 5: Add reload helper method to `EditorState`

Add a helper method that performs the reload logic for a clean buffer:

```rust
/// Reload a file tab's buffer from disk.
///
/// This is called when `FileChanged` arrives for a tab with `dirty == false`.
/// It re-reads the file, replaces the buffer content, updates `base_content`,
/// preserves cursor position (clamped to buffer bounds), and re-applies
/// syntax highlighting.
///
/// Returns `true` if the reload succeeded, `false` if the file couldn't be
/// read or no matching tab was found.
// Chunk: docs/chunks/base_snapshot_reload - Clean buffer reload
fn reload_file_tab(&mut self, path: &Path) -> bool {
    // Find the workspace and tab for this path
    // Read the file content
    // Check dirty flag - only reload if clean
    // Replace buffer content
    // Update base_content
    // Clamp cursor position
    // Re-apply syntax highlighting
    // Mark DirtyRegion::FullViewport
}
```

Implementation details:
1. Iterate through workspaces to find the tab with matching `associated_file`
2. Return early if `tab.dirty == true` (defer to three_way_merge chunk)
3. Read file via `std::fs::read()` with `String::from_utf8_lossy()`
4. Store old cursor position before replacing buffer
5. Create new `TextBuffer::from_str()` and assign to tab
6. Clamp cursor: `row = row.min(buffer.line_count().saturating_sub(1))`, `col = col.min(line.len())`
7. Set `tab.base_content = Some(new_content)`
8. Re-setup syntax highlighting (via existing `setup_tab_highlighting()` or similar)
9. Mark `self.dirty_region.merge(DirtyRegion::FullViewport)`

Location: `crates/editor/src/editor_state.rs`

### Step 6: Implement `handle_file_changed()` in drain loop

Replace the placeholder in `EventDrainLoop::handle_file_changed()`:

```rust
// Chunk: docs/chunks/base_snapshot_reload - File change event handler
fn handle_file_changed(&mut self, path: std::path::PathBuf) {
    // Check if this is a self-triggered event (our own save)
    if self.state.is_file_change_suppressed(&path) {
        // Ignore - this was our own write
        return;
    }

    // Find the tab for this path across all workspaces
    // If tab.dirty == false:
    //     Call reload_file_tab() to reload from disk
    // If tab.dirty == true:
    //     Placeholder for three_way_merge chunk - do nothing for now
    // If no tab matches the path:
    //     Ignore - the file isn't open

    self.state.reload_file_tab(&path);
}
```

Note: `reload_file_tab()` already checks the dirty flag internally and does nothing for dirty buffers.

Location: `crates/editor/src/drain_loop.rs`

### Step 7: Handle "no matching tab" case

When `FileChanged` arrives for a path that has no open tab (e.g., a file in the workspace that isn't currently open), the event should be silently ignored. This is already the behavior if `reload_file_tab()` returns early when no tab is found.

Add a log statement or metric if desired for debugging:

```rust
if !self.state.reload_file_tab(&path) {
    // File not open or couldn't be reloaded - this is expected for
    // files in the workspace that aren't currently in a tab
}
```

Location: `crates/editor/src/drain_loop.rs`

### Step 8: Add cursor clamping utility function

Add a utility function for cursor position clamping (can be used by other chunks too):

```rust
/// Clamp a cursor position to be valid within the given buffer.
///
/// The row is clamped to `[0, line_count - 1]` (or 0 for empty buffers).
/// The column is clamped to `[0, line_length]` for the clamped row.
// Chunk: docs/chunks/base_snapshot_reload - Cursor clamping after reload
pub fn clamp_position_to_buffer(pos: Position, buffer: &dyn BufferView) -> Position {
    let line_count = buffer.line_count();
    if line_count == 0 {
        return Position::new(0, 0);
    }

    let row = pos.row.min(line_count - 1);
    let line_len = buffer.line(row).map(|l| l.len()).unwrap_or(0);
    let col = pos.col.min(line_len);

    Position::new(row, col)
}
```

Location: `crates/editor/src/editor_state.rs` or `crates/buffer/src/position.rs` if a better location exists

### Step 9: Add unit tests for cursor clamping

Write tests for the cursor clamping function:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_position_empty_buffer() {
        let buffer = TextBuffer::new();
        let pos = clamp_position_to_buffer(Position::new(5, 10), &buffer);
        assert_eq!(pos, Position::new(0, 0));
    }

    #[test]
    fn test_clamp_position_row_beyond_buffer() {
        let buffer = TextBuffer::from_str("line1\nline2");
        let pos = clamp_position_to_buffer(Position::new(10, 0), &buffer);
        assert_eq!(pos.row, 1); // clamped to last line
    }

    #[test]
    fn test_clamp_position_col_beyond_line() {
        let buffer = TextBuffer::from_str("abc");
        let pos = clamp_position_to_buffer(Position::new(0, 10), &buffer);
        assert_eq!(pos.col, 3); // clamped to end of line
    }

    #[test]
    fn test_clamp_position_valid() {
        let buffer = TextBuffer::from_str("hello\nworld");
        let pos = clamp_position_to_buffer(Position::new(1, 3), &buffer);
        assert_eq!(pos, Position::new(1, 3)); // unchanged
    }
}
```

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

### Step 10: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched:

```yaml
code_paths:
  - crates/editor/src/workspace.rs
  - crates/editor/src/editor_state.rs
  - crates/editor/src/drain_loop.rs
```

## Dependencies

- **file_change_events chunk**: Must be complete. This chunk provides `EditorEvent::FileChanged`, the drain loop handler placeholder, and self-write suppression. The `base_snapshot_reload` chunk builds directly on this infrastructure.

- No new external crate dependencies needed.

## Risks and Open Questions

1. **Path normalization**: The `notify` crate delivers absolute paths. `Tab::associated_file` also stores absolute paths (set in `associate_file()`). They should match without normalization, but symlinks could cause mismatches. Defer symlink handling unless issues arise in practice.

2. **Race condition on rapid external edits**: If an external program writes rapidly (multiple writes within the debounce window), we may reload to an intermediate state. The debouncer coalesces these into a single event, but the file content when we read it might not be the "final" state. This is acceptable — we'll get another FileChanged event if the file changes again.

3. **Very large files**: The investigation noted H4 is UNTESTED. For files > 1MB, storing a second copy of the content doubles memory usage. For the target use case (source files), this is acceptable. A future enhancement could use mtime + on-demand re-read for large files.

4. **Syntax highlighting state**: After reload, we need to re-apply syntax highlighting. The existing `setup_tab_highlighting()` mechanism should work, but verify it handles re-initialization correctly.

5. **Dirty flag during reload**: The reload is only performed when `dirty == false`. After reload, the buffer content matches disk content, so `dirty` should remain `false`. No dirty flag manipulation needed.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
