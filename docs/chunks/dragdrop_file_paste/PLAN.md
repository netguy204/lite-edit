<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

We implement macOS drag-and-drop by extending the existing `MetalView` NSView subclass
to act as an `NSDraggingDestination`. The view will:

1. Register for file URL drag types at creation time
2. Implement the `NSDraggingDestination` protocol methods to accept dropped files
3. Convert dropped `NSURL` objects to string paths
4. Send the paths through the event channel to the drain loop
5. Insert the paths as text at the cursor position (same flow as paste)

**Key architectural decisions:**

- **Event channel integration**: Dropped file paths flow through `EditorEvent` like all
  other inputs, maintaining the unified event architecture from `pty_wakeup_reentrant`.
  This avoids introducing new borrow patterns.

- **Paste-like text insertion**: Once paths arrive at the drain loop, they're inserted
  using the same `InputEncoder::encode_paste` mechanism used for Cmd+V. For terminal
  tabs, this respects bracketed paste mode. For buffer editors, paths are inserted
  directly.

- **Shell escaping**: Paths containing spaces or special characters are shell-escaped
  or quoted. We use single quotes with escaped internal single quotes to match how
  macOS Terminal.app handles paths.

- **Multiple files**: Space-separated, each individually escaped.

**Testing strategy (per TESTING_PHILOSOPHY.md):**

- **Unit-testable**: Shell escaping logic is pure Rust, fully testable.
- **Humble view**: The Cocoa drag-drop setup is platform shell code (not unit-tested).
  We verify visually that the drag cursor and drop behavior work.
- **Integration**: The path-to-text-insertion flow is tested by simulating a
  `FileDrop` event and asserting the terminal/buffer receives the escaped paths.

## Subsystem Considerations

No subsystems are directly relevant. This chunk extends the event channel pattern
established in `pty_wakeup_reentrant` but does not modify any documented subsystems.

## Sequence

### Step 1: Add shell_escape utility function

Create a new utility module `crates/editor/src/shell_escape.rs` with a function
that escapes file paths for shell use.

**Behavior:**
- Wrap path in single quotes
- Escape any internal single quotes by ending the quote, adding `\'`, and resuming
- Example: `/path/to/foo's file.txt` → `'/path/to/foo'\''s file.txt'`
- For paths without special characters, single quotes are still used for consistency

**Tests (TDD - write first):**
- Simple path: `/Users/test/file.txt` → `'/Users/test/file.txt'`
- Path with space: `/Users/test/my file.txt` → `'/Users/test/my file.txt'`
- Path with single quote: `/Users/test/foo's.txt` → `'/Users/test/foo'\''s.txt'`
- Path with both: `/Users/test/foo's file.txt` → `'/Users/test/foo'\''s file.txt'`
- Multiple paths joined: vec of paths → space-separated escaped paths

Location: `crates/editor/src/shell_escape.rs`

---

### Step 2: Add FileDrop event variant to EditorEvent

Extend `EditorEvent` enum with a new variant for file drops:

```rust
/// Files were dropped onto the view
///
/// Contains the list of file paths (as UTF-8 strings) that were dropped.
/// The paths are absolute and need shell escaping before insertion.
FileDrop(Vec<String>),
```

Update `is_user_input()` to return `true` for `FileDrop` (it's user input).

Location: `crates/editor/src/editor_event.rs`

---

### Step 3: Add send_file_drop to EventSender

Add a method to `EventSender` for sending file drop events:

```rust
pub fn send_file_drop(&self, paths: Vec<String>) -> Result<(), SendError<EditorEvent>> {
    let result = self.inner.sender.send(EditorEvent::FileDrop(paths));
    (self.inner.run_loop_waker)();
    result
}
```

Location: `crates/editor/src/event_channel.rs`

---

### Step 4: Register MetalView for file URL drag types

In `MetalView::new()`, after creating the view, register it to accept file URL
drags. This requires:

1. Get `NSPasteboardTypeFileURL` (the modern UTI for file URLs)
2. Call `self.registerForDraggedTypes(&[file_url_type])`

The `objc2-app-kit` crate provides these via the `NSPasteboard` feature. We may
need to enable this feature in `Cargo.toml` if not already enabled.

**Implementation note:** `registerForDraggedTypes` requires an `NSArray` of
`NSPasteboardType` values. The modern type is `NSPasteboardTypeFileURL`.

Location: `crates/editor/src/metal_view.rs` (in `MetalView::new()`)

---

### Step 5: Implement NSDraggingDestination protocol methods

Add `NSDraggingDestination` protocol implementation to `MetalView`. The minimum
required methods are:

**`draggingEntered:`** - Called when drag enters the view
- Return `NSDragOperationCopy` to indicate we accept the drop
- This makes the drag cursor show a copy badge

**`performDragOperation:`** - Called when user releases the drag
- Extract file URLs from the `NSDraggingInfo` pasteboard
- Convert each `NSURL` to a `String` path via `url.path().to_string()`
- Send via `EventSender::send_file_drop()`
- Return `true` on success

**Protocol implementation pattern (following existing method overrides):**
```rust
impl MetalView {
    // In the define_class! macro block:

    #[unsafe(method(draggingEntered:))]
    fn __dragging_entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
        NSDragOperation::Copy
    }

    #[unsafe(method(performDragOperation:))]
    fn __perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
        // Extract paths from pasteboard and send via event channel
        ...
    }
}
```

**Path extraction:**
```rust
let pasteboard = sender.draggingPasteboard();
// Read file URLs from the pasteboard
let urls: Option<Retained<NSArray<NSURL>>> = pasteboard.readObjectsForClasses_options(
    &NSArray::from_slice(&[NSURL::class()]),
    None
);
// Convert each NSURL to String path via url.path().to_string()
```

Location: `crates/editor/src/metal_view.rs`

---

### Step 6: Handle FileDrop in drain loop

Add a match arm in `EventDrainLoop::process_pending_events()` to handle
`EditorEvent::FileDrop(paths)`:

```rust
EditorEvent::FileDrop(paths) => {
    self.handle_file_drop(paths);
}
```

Implement `handle_file_drop`:
- Shell-escape each path using the utility from Step 1
- Join with spaces
- Insert as text using the same mechanism as paste

**For terminal focus:** Use `InputEncoder::encode_paste()` + `terminal.write_input()`
(same as Cmd+V paste in `handle_key_terminal`).

**For buffer focus:** Use `ctx.buffer.insert_str()` (same as Cmd+V paste in
`BufferFocusTarget::execute_command(Command::Paste)`).

The approach mirrors how `handle_key` routes to different targets based on focus.
We call into `EditorState` to handle the insertion.

Location: `crates/editor/src/drain_loop.rs`, `crates/editor/src/editor_state.rs`

---

### Step 7: Add handle_file_drop to EditorState

Add a method `EditorState::handle_file_drop(paths: Vec<String>)` that:

1. Shell-escapes each path
2. Joins them with spaces into a single string
3. Based on current focus:
   - **Terminal**: Uses `InputEncoder::encode_paste()` + `terminal.write_input()`
   - **Buffer/FindInFile/Selector**: Uses `buffer.insert_str()` or ignores

This mirrors how `handle_key` dispatches to focus-specific handlers.

Location: `crates/editor/src/editor_state.rs`

---

### Step 8: Integration test for FileDrop event flow

Write an integration test that:
1. Creates an `EditorState` with a terminal tab
2. Simulates a `FileDrop(vec!["/path/to/file.txt"])` event
3. Asserts that the shell-escaped path was written to the terminal

This tests the path from event → shell_escape → terminal without involving
the actual macOS drag-drop APIs.

Location: `crates/editor/src/editor_state.rs` (test module)

---

### Step 9: Visual verification

Manually test:
1. Drag a single file from Finder onto the terminal pane → path appears
2. Drag multiple files → space-separated paths appear
3. Drag file with spaces in name → properly escaped (single-quoted)
4. Drag file with single quote in name → properly escaped
5. Drag onto buffer editor pane → path(s) inserted at cursor
6. Drag non-file content (e.g., text) → no crash, no action
7. Verify drag cursor shows copy badge during hover
8. Test at 1x and 2x (Retina) scale factors

---

**BACKREFERENCE COMMENTS**

When implementing code, add backreference comments:
- `// Chunk: docs/chunks/dragdrop_file_paste - ...`

Place at method-level for the drag-drop handlers and shell_escape functions.

## Dependencies

- **objc2-app-kit features**: May need to enable `NSDragging` and/or `NSPasteboard`
  features in `Cargo.toml` to access `NSDraggingDestination`, `NSDraggingInfo`,
  `registerForDraggedTypes`, and `NSPasteboardTypeFileURL`.

- **Existing chunks**: Builds on `pty_wakeup_reentrant` event channel architecture.

## Risks and Open Questions

1. **objc2-app-kit API surface**: The exact API for `NSDraggingDestination` protocol
   implementation in objc2 0.6 may differ from documentation examples. May need to
   consult objc2 source or examples for correct trait implementation syntax.

2. **NSPasteboardTypeFileURL availability**: This is the modern UTI-based type.
   If targeting older macOS versions, may need fallback to `NSFilenamesPboardType`.
   Verify minimum macOS version requirements.

3. **Non-file drags**: When dragging text or other non-file content, we should
   gracefully ignore it. Need to verify the pasteboard doesn't contain file URLs
   before attempting to read them.

4. **Path encoding**: File paths from NSURL should be UTF-8, but edge cases with
   filesystem encoding may exist. Convert via `path().to_string()` and handle
   potential non-UTF-8 paths gracefully.

5. **Thread safety**: `performDragOperation` is called on the main thread, same
   as other NSView callbacks. The EventSender is designed for this, so should be fine.

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