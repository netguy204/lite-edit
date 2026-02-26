<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk extends the `FileChanged` event infrastructure (from `file_change_events`) to handle two additional filesystem event types: file deletion and file rename. The implementation follows the existing patterns established by that chunk.

**For file deletion (`Remove` events):**
- Add a new `EditorEvent::FileDeleted(PathBuf)` variant
- Forward `Remove` events from the `FileIndex` watcher for files with open buffers
- Show a confirm dialog with "Save" (recreate file) and "Abandon" (close tab) options
- Reuse the existing `ConfirmDialog` infrastructure with custom button labels

**For file rename (`Modify(Name(_))` events):**
- Add a new `EditorEvent::FileRenamed { from: PathBuf, to: PathBuf }` variant
- Forward rename events from the `FileIndex` watcher with both old and new paths
- Update `tab.associated_file` to the new path
- Update the tab label to reflect the new filename
- Re-evaluate syntax highlighting if the file extension changed

**Key design decisions:**

1. **Event routing**: Like `FileChanged`, these events are routed through the existing `EditorEvent` channel and drain loop. They are priority events (processed before PTY wakeup).

2. **Buffer matching**: The drain loop will iterate over all workspaces and tabs to find any tab whose `associated_file` matches the affected path. Multiple tabs can show the same file, so all must be updated.

3. **Confirm dialog**: For file deletion, we reuse the `ConfirmDialogContext` enum by adding a new `FileDeletedFromDisk` variant. The dialog uses `ConfirmDialog::with_labels()` for "Save"/"Abandon" button text.

4. **Syntax re-evaluation**: When a file is renamed and the extension changes (e.g., `.txt` → `.rs`), we call `Tab::setup_highlighting()` to re-detect the language and create a new highlighter.

**Testing approach (per TESTING_PHILOSOPHY.md):**

- Unit tests for event variant construction and properties
- Unit tests for the path-matching logic (finding tabs for a given path)
- Unit tests for tab label update logic
- Unit tests for extension change detection
- Integration tests (marked `#[ignore]`) for end-to-end event flow

## Subsystem Considerations

No existing subsystems are directly relevant to this work. This chunk extends the file watcher infrastructure established by `file_change_events`.

## Sequence

### Step 1: Add `FileDeleted` and `FileRenamed` event variants

Add two new variants to `EditorEvent` in `crates/editor/src/editor_event.rs`:

```rust
// Chunk: docs/chunks/deletion_rename_handling - External file deletion detection
/// A file was deleted externally (from disk)
///
/// This event is sent when the filesystem watcher detects that a file
/// with an open buffer was removed by an external process. The path
/// is absolute.
FileDeleted(PathBuf),

// Chunk: docs/chunks/deletion_rename_handling - External file rename detection
/// A file was renamed externally
///
/// This event is sent when the filesystem watcher detects that a file
/// with an open buffer was renamed by an external process. Both paths
/// are absolute.
FileRenamed { from: PathBuf, to: PathBuf },
```

Update `is_priority_event()` to return `true` for both (external file operations should be processed promptly).

Update `is_user_input()` to return `false` for both (they're not user input).

Add unit tests for the new variants' trait implementations.

Location: `crates/editor/src/editor_event.rs`

### Step 2: Add `send_file_deleted` and `send_file_renamed` methods to `EventSender`

Add methods to `EventSender` in `crates/editor/src/event_channel.rs`:

```rust
/// Sends a file-deleted event to the channel.
pub fn send_file_deleted(&self, path: PathBuf) -> Result<(), SendError<EditorEvent>> {
    let result = self.inner.sender.send(EditorEvent::FileDeleted(path));
    (self.inner.run_loop_waker)();
    result
}

/// Sends a file-renamed event to the channel.
pub fn send_file_renamed(&self, from: PathBuf, to: PathBuf) -> Result<(), SendError<EditorEvent>> {
    let result = self.inner.sender.send(EditorEvent::FileRenamed { from, to });
    (self.inner.run_loop_waker)();
    result
}
```

Location: `crates/editor/src/event_channel.rs`

### Step 3: Add `FileDeletedFromDisk` context variant to `ConfirmDialogContext`

Add a new variant to `ConfirmDialogContext` in `crates/editor/src/confirm_dialog.rs`:

```rust
// Chunk: docs/chunks/deletion_rename_handling - File deleted confirmation context
/// File was deleted from disk while buffer was open.
FileDeletedFromDisk {
    /// The pane containing the affected tab.
    pane_id: PaneId,
    /// The index of the tab within the pane.
    tab_idx: usize,
    /// The path that was deleted (for recreating the file).
    deleted_path: PathBuf,
},
```

Add unit tests for the new context variant (pattern matching, Clone).

Location: `crates/editor/src/confirm_dialog.rs`

### Step 4: Extend `FileIndex` to forward `Remove` and `Modify(Name)` events

Modify `handle_fs_event()` in `crates/editor/src/file_index.rs` to detect and forward these events:

1. For `EventKind::Remove(_)`: If a callback is registered and the path matches a tracked file, invoke a deletion callback.

2. For `EventKind::Modify(ModifyKind::Name(_))`: Track rename pairs (FSEvents delivers both old and new paths). When we have both, invoke a rename callback.

**Rename detection strategy**: FSEvents delivers `Modify(Name(_))` events for both the old path (no longer exists) and new path (now exists). We can distinguish them by checking `path.exists()`:
- If `!path.exists()` → this is the old path (source of rename)
- If `path.exists()` → this is the new path (target of rename)

Store the old path temporarily and emit the `FileRenamed` event when we see the new path arrive.

Add two new callback types alongside the existing `file_change_callback`:
- `on_file_deleted: Option<Arc<dyn Fn(PathBuf) + Send + Sync>>`
- `on_file_renamed: Option<Arc<dyn Fn(PathBuf, PathBuf) + Send + Sync>>`

Location: `crates/editor/src/file_index.rs`

### Step 5: Wire up callbacks in `Workspace`

Extend the callback wiring in `Workspace::new()` (and related constructors) to also set the file deleted and file renamed callbacks:

```rust
let sender_clone_del = sender.clone();
let sender_clone_ren = sender.clone();

file_index.set_file_deleted_callback(move |path| {
    let _ = sender_clone_del.send_file_deleted(path);
});

file_index.set_file_renamed_callback(move |from, to| {
    let _ = sender_clone_ren.send_file_renamed(from, to);
});
```

Location: `crates/editor/src/workspace.rs`, `crates/editor/src/editor_state.rs`

### Step 6: Implement drain loop handler for `FileDeleted`

Add a handler in `EventDrainLoop::process_single_event()`:

```rust
EditorEvent::FileDeleted(path) => {
    self.handle_file_deleted(path);
}
```

Implement `handle_file_deleted()`:

1. Check if the path is suppressed (self-write suppression for consistency)
2. Search all workspaces and panes for tabs where `associated_file == Some(path)`
3. For the first matching tab found:
   - If `dirty == false`: Show confirm dialog with "Save" and "Abandon"
   - If `dirty == true`: Show confirm dialog with "Save" and "Abandon" (same behavior - the user has content they might want to keep)
4. Store the pane_id, tab_idx, and path in the `FileDeletedFromDisk` context

Note: We only prompt for one tab at a time. If multiple tabs show the same deleted file, subsequent `FileDeleted` events will prompt for them (or the user will see them when they switch tabs).

Location: `crates/editor/src/drain_loop.rs`

### Step 7: Implement confirm outcome handler for `FileDeletedFromDisk`

Add handling for `FileDeletedFromDisk` in `EditorState::handle_confirm_outcome()`:

```rust
ConfirmDialogContext::FileDeletedFromDisk { pane_id, tab_idx, deleted_path } => {
    // Save was selected - recreate the file with buffer contents
    self.save_file_to_path(pane_id, tab_idx, &deleted_path);
}
```

For "Abandon" (cancelled in confirm dialog terms, but the user selected Abandon):
- If the confirm outcome is `Confirmed` (Abandon button), close the tab via `force_close_tab()`
- If the confirm outcome is `Cancelled` (Cancel button), also close the tab (the file is gone, there's no "keep editing")

Wait - re-reading the success criteria: "If the user chooses Save, the file is written from the buffer and the tab continues normally. If the user chooses Abandon, the tab is closed."

So the dialog should be:
- **Save button**: Recreate the file from buffer, tab continues (clear dirty flag after save)
- **Abandon button**: Close the tab without saving

This means "Save" is the confirm action (right button), "Abandon" is the cancel action (left button). But that's backwards from the normal confirm dialog semantics where "Abandon" is the destructive action.

**Design decision**: Use `ConfirmDialog::with_labels("File was deleted. Save to recreate?", "Abandon", "Save")`. The confirm button (right) will be "Save", and the cancel button (left) will be "Abandon". The handler:
- `ConfirmOutcome::Confirmed` → Save the file, continue
- `ConfirmOutcome::Cancelled` → Close the tab

Location: `crates/editor/src/editor_state.rs`

### Step 8: Implement drain loop handler for `FileRenamed`

Add a handler in `EventDrainLoop::process_single_event()`:

```rust
EditorEvent::FileRenamed { from, to } => {
    self.handle_file_renamed(from, to);
}
```

Implement `handle_file_renamed()`:

1. Search all workspaces and panes for tabs where `associated_file == Some(from)`
2. For each matching tab:
   - Update `tab.associated_file = Some(to.clone())`
   - Update `tab.label` to the new filename (extract from `to.file_name()`)
   - Check if the file extension changed:
     - Extract extensions from `from` and `to`
     - If different, call `tab.setup_highlighting()` to re-detect language
   - Mark `dirty_region` as needing a full viewport refresh (tab bar changed)

Location: `crates/editor/src/drain_loop.rs`

### Step 9: Add helper method for extension comparison

Add a helper method to `Tab` or a standalone function:

```rust
/// Returns true if the file extension changed between two paths.
fn extension_changed(from: &Path, to: &Path) -> bool {
    from.extension() != to.extension()
}
```

Location: `crates/editor/src/workspace.rs` (or inline in drain_loop)

### Step 10: Add unit tests for `FileDeleted` and `FileRenamed` event properties

Add tests to `crates/editor/src/editor_event.rs`:

```rust
#[test]
fn test_file_deleted_is_priority() {
    let event = EditorEvent::FileDeleted(PathBuf::from("/path/to/file.rs"));
    assert!(event.is_priority_event());
}

#[test]
fn test_file_deleted_is_not_user_input() {
    let event = EditorEvent::FileDeleted(PathBuf::from("/path/to/file.rs"));
    assert!(!event.is_user_input());
}

#[test]
fn test_file_renamed_is_priority() {
    let event = EditorEvent::FileRenamed {
        from: PathBuf::from("/path/to/old.rs"),
        to: PathBuf::from("/path/to/new.rs"),
    };
    assert!(event.is_priority_event());
}

#[test]
fn test_file_renamed_is_not_user_input() {
    let event = EditorEvent::FileRenamed {
        from: PathBuf::from("/path/to/old.rs"),
        to: PathBuf::from("/path/to/new.rs"),
    };
    assert!(!event.is_user_input());
}
```

Location: `crates/editor/src/editor_event.rs`

### Step 11: Add unit tests for confirm dialog context

Add tests to `crates/editor/src/confirm_dialog.rs`:

```rust
#[test]
fn test_context_file_deleted_stores_pane_tab_and_path() {
    let ctx = ConfirmDialogContext::FileDeletedFromDisk {
        pane_id: 42,
        tab_idx: 3,
        deleted_path: PathBuf::from("/path/to/deleted.txt"),
    };

    match ctx {
        ConfirmDialogContext::FileDeletedFromDisk { pane_id, tab_idx, deleted_path } => {
            assert_eq!(pane_id, 42);
            assert_eq!(tab_idx, 3);
            assert_eq!(deleted_path, PathBuf::from("/path/to/deleted.txt"));
        }
        _ => panic!("Expected FileDeletedFromDisk variant"),
    }
}

#[test]
fn test_context_file_deleted_is_clone() {
    let ctx = ConfirmDialogContext::FileDeletedFromDisk {
        pane_id: 1,
        tab_idx: 0,
        deleted_path: PathBuf::from("/path"),
    };
    let cloned = ctx.clone();
    // Verify both have same values...
}
```

Location: `crates/editor/src/confirm_dialog.rs`

### Step 12: Add unit tests for extension comparison logic

```rust
#[test]
fn test_extension_changed_same_extension() {
    assert!(!extension_changed(
        Path::new("/a/foo.rs"),
        Path::new("/b/bar.rs")
    ));
}

#[test]
fn test_extension_changed_different_extension() {
    assert!(extension_changed(
        Path::new("/a/foo.txt"),
        Path::new("/a/foo.rs")
    ));
}

#[test]
fn test_extension_changed_no_extension_to_extension() {
    assert!(extension_changed(
        Path::new("/a/Makefile"),
        Path::new("/a/Makefile.bak")
    ));
}

#[test]
fn test_extension_changed_extension_to_no_extension() {
    assert!(extension_changed(
        Path::new("/a/foo.rs"),
        Path::new("/a/foo")
    ));
}
```

Location: `crates/editor/src/workspace.rs` or `drain_loop.rs`

### Step 13: Update GOAL.md code_paths

Update the chunk's GOAL.md frontmatter with the files touched:

```yaml
code_paths:
  - crates/editor/src/editor_event.rs
  - crates/editor/src/event_channel.rs
  - crates/editor/src/confirm_dialog.rs
  - crates/editor/src/file_index.rs
  - crates/editor/src/workspace.rs
  - crates/editor/src/drain_loop.rs
  - crates/editor/src/editor_state.rs
```

## Dependencies

- **file_change_events** chunk must be complete. It provides:
  - The `EditorEvent::FileChanged` infrastructure we're extending
  - The `EventSender::send_file_changed()` pattern we'll follow
  - The `FileIndex` callback mechanism for forwarding events
  - The self-write suppression registry (which we reuse for consistency)

## Risks and Open Questions

1. **Rename event ordering**: FSEvents delivers `Modify(Name(_))` events for both old and new paths, but the order is not guaranteed. We may need to buffer events and match them. If we see the new path first and the old path later, we need to handle that.

2. **Rapid rename chains**: If a file is renamed multiple times quickly (e.g., `a.rs` → `b.rs` → `c.rs`), we may receive interleaved events. The debouncer from `file_change_events` helps, but we should consider whether rename events also need debouncing.

3. **Path canonicalization**: Paths from FSEvents should be absolute, and `Tab::associated_file` stores absolute paths. However, symlinks could cause mismatches. We may need to canonicalize paths before comparison. Defer this unless issues arise.

4. **Multiple tabs with same file**: If the same file is open in multiple tabs (perhaps in different panes), all tabs should be updated on rename. For deletion, we prompt for one tab at a time to avoid dialog overload.

5. **Tab label extraction**: `PathBuf::file_name()` returns `Option<&OsStr>`. We need to handle the case where the path has no filename (unlikely for regular files, but defensive).

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->