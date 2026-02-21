<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk completes the file I/O story by implementing file-buffer association and save functionality. The implementation builds directly on the `file_picker` chunk's `resolved_path` field and follows the project's Humble View Architecture:

1. **Extend `EditorState`** with an `associated_file: Option<PathBuf>` field to track the current file association
2. **Add `associate_file(path: PathBuf)` method** that loads file contents into the buffer (or leaves it empty for new files), resets cursor/scroll state, and stores the path
3. **Consume `resolved_path`** from the file picker confirmation to trigger file association
4. **Update window title** via `NSWindow::setTitle_` when the association changes
5. **Handle Cmd+S** to write buffer contents to the associated file

Tests follow the project's TDD discipline for testable behavior (buffer replacement, cursor reset, file writing) but skip tests for platform integration (window title setting via NSWindow).

The implementation uses `std::fs::read_to_string` with `String::from_utf8_lossy` for UTF-8 file reading (replacing invalid bytes with U+FFFD) and `std::fs::write` for saving.

## Sequence

### Step 1: Add `associated_file` field to `EditorState`

Add the field specified in the success criteria:

```rust
// In editor_state.rs, in EditorState struct
/// The file currently associated with the buffer (if any).
/// When `Some`, this is the path that Cmd+S writes to.
pub associated_file: Option<PathBuf>,
```

Initialize to `None` in `EditorState::new()` and `EditorState::empty()`.

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Initial `associated_file` is `None`

### Step 2: Implement `associate_file(path: PathBuf)` method

Add method to `EditorState` that:

1. If the file at `path` exists:
   - Read its contents using `std::fs::read()` (returns `Vec<u8>`)
   - Convert to UTF-8 using `String::from_utf8_lossy()` (replaces invalid bytes with `\u{FFFD}`)
   - Replace the buffer with `TextBuffer::from_str(&contents)`
   - Reset cursor to `(0, 0)` via `buffer.set_cursor(Position::new(0, 0))`
   - Reset viewport scroll offset to 0 via `viewport.scroll_to(0, line_count)`
2. If the file does not exist:
   - Leave the buffer as-is (empty for new files created by file picker)
3. Store `path` in `associated_file`
4. Mark `DirtyRegion::FullViewport`

```rust
// In editor_state.rs
// Chunk: docs/chunks/file_save - File-buffer association and Cmd+S save
pub fn associate_file(&mut self, path: PathBuf) {
    if path.exists() {
        // Read file contents with UTF-8 lossy conversion
        match std::fs::read(&path) {
            Ok(bytes) => {
                let contents = String::from_utf8_lossy(&bytes);
                self.buffer = TextBuffer::from_str(&contents);
                self.buffer.set_cursor(lite_edit_buffer::Position::new(0, 0));
                let line_count = self.buffer.line_count();
                self.viewport.scroll_to(0, line_count);
            }
            Err(_) => {
                // Silently ignore read errors (out of scope for this chunk)
            }
        }
    }
    // For non-existent files, leave buffer as-is (file picker already created empty file)

    self.associated_file = Some(path);
    self.dirty_region.merge(DirtyRegion::FullViewport);
}
```

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- `associate_file` with an existing file: buffer content matches file, cursor at `(0, 0)`
- `associate_file` with an existing file: `associated_file` is `Some(path)`
- `associate_file` with a non-existent path: buffer unchanged
- `associate_file` with a non-existent path: `associated_file` is `Some(path)`
- `associate_file` resets scroll offset to 0

### Step 3: Consume `resolved_path` after file picker confirmation

Modify the file picker confirmation handling to call `associate_file()`:

In `EditorState::handle_selector_confirm()`, after storing `resolved_path` and before calling `close_selector()`:
1. Take the resolved path: `let path = self.resolved_path.take().unwrap();`
2. Call `self.associate_file(path);`
3. Re-store in `resolved_path` if needed for debugging (or just leave it as `None`)

Actually, looking more closely at the flow: the file picker stores `resolved_path`, then closes. The `file_save` chunk should consume this immediately after the picker closes. Let's modify to call `associate_file` right after setting `resolved_path`:

```rust
// In handle_selector_confirm, after setting resolved_path:
self.resolved_path = Some(resolved.clone());

// Immediately associate the file with the buffer
self.associate_file(resolved);
```

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- After file picker confirmation, buffer contains file contents (for existing file)
- After file picker confirmation with new file, buffer remains empty
- After file picker confirmation, `associated_file` is set

### Step 4: Add Cmd+S handler for file save

Modify `EditorState::handle_key()` to intercept Cmd+S before delegating:

```rust
// In handle_key, within the Cmd+!Ctrl block:
if let Key::Char('s') = event.key {
    self.save_file();
    return;
}
```

Implement `save_file()` method:

```rust
// Chunk: docs/chunks/file_save - File-buffer association and Cmd+S save
fn save_file(&mut self) {
    let path = match &self.associated_file {
        Some(p) => p.clone(),
        None => return, // No file associated - no-op
    };

    let content = self.buffer.content();
    let _ = std::fs::write(&path, content.as_bytes());
    // Silently ignore write errors (out of scope for this chunk)

    // Cmd+S does NOT mark dirty (buffer unchanged visually)
}
```

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Cmd+S with `associated_file == None`: no-op, buffer unchanged
- Cmd+S with `associated_file == Some(path)`: file on disk contains buffer content
- Cmd+S does not modify the buffer
- Cmd+S does not move the cursor
- Cmd+S does not mark dirty region

### Step 5: Update window title on file association

The window title update requires access to the `NSWindow`, which is owned by `AppDelegate` in `main.rs`. We need a mechanism to notify `main.rs` when the associated file changes.

**Approach**: Add a `window_title_needs_update: bool` flag to `EditorState` that `associate_file()` sets to `true`. Then in `EditorController::render_if_dirty()`, check this flag and update the window title.

However, a cleaner approach is to have `EditorController` check the current `associated_file` path and derive the title directly. Add a method `EditorState::window_title() -> &str` that returns:
- The filename (last path component) if `associated_file.is_some()`
- `"Untitled"` if `associated_file.is_none()`

```rust
// In editor_state.rs
/// Returns the window title based on the current file association.
/// Returns the filename if a file is associated, or "Untitled" otherwise.
pub fn window_title(&self) -> String {
    match &self.associated_file {
        Some(path) => path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string(),
        None => "Untitled".to_string(),
    }
}
```

Then in `main.rs`, track the previous title and update when changed:

```rust
// In EditorController
fn update_window_title_if_needed(&mut self, window: &NSWindow) {
    let new_title = self.state.window_title();
    // Use objc2 to set the title
    window.setTitle(&NSString::from_str(&new_title));
}
```

This requires passing the window reference to `EditorController`. Currently `EditorController` holds `metal_view` but not `window`. We'll need to either:
1. Store a reference to the window in `EditorController`
2. Access the window via `metal_view.window()`

Option 2 is cleaner - `NSView` has a `window` method that returns the window it's attached to.

Location: `crates/editor/src/editor_state.rs` and `crates/editor/src/main.rs`

Write tests for:
- `window_title()` returns "Untitled" when no file associated
- `window_title()` returns filename when file is associated

### Step 6: Wire window title update in main.rs

Modify `EditorController` to track window title state and update when needed:

1. Add `last_window_title: String` field to `EditorController`
2. In `render_if_dirty()`, check if `state.window_title()` differs from `last_window_title`
3. If different, get the window via `metal_view.window()` and call `setTitle`
4. Store the new title in `last_window_title`

```rust
// In EditorController::render_if_dirty or a new method:
fn update_window_title_if_needed(&mut self) {
    let current_title = self.state.window_title();
    if current_title != self.last_window_title {
        // Get window from metal_view
        if let Some(window) = self.metal_view.window() {
            window.setTitle(&NSString::from_str(&current_title));
        }
        self.last_window_title = current_title;
    }
}
```

Note: Need to check the objc2 API for `NSView::window()` and `NSWindow::setTitle_`.

Location: `crates/editor/src/main.rs`

No tests (platform integration).

### Step 7: Add module-level backreference comment

Add the chunk backreference comment at the top of `editor_state.rs`:

```rust
// Chunk: docs/chunks/file_save - File-buffer association and Cmd+S save
```

Location: `crates/editor/src/editor_state.rs` (add to existing chunk comments)

### Step 8: Update code_paths in GOAL.md

Update the `code_paths` field in the chunk's GOAL.md frontmatter with the files modified:

```yaml
code_paths:
  - crates/editor/src/editor_state.rs
  - crates/editor/src/main.rs
```

Location: `docs/chunks/file_save/GOAL.md`

### Step 9: Manual smoke test

Verify the following by running the application:

1. Open app - window title should be "lite-edit" (or "Untitled" after our change)
2. Press Cmd+P, select an existing file, press Enter
3. Verify buffer shows file contents
4. Verify window title shows filename
5. Verify cursor is at (0, 0)
6. Edit the buffer (add some text)
7. Press Cmd+S
8. Re-open the same file (Cmd+P, select it) or verify via external tool that changes were saved
9. Press Cmd+P, type a new filename, press Enter
10. Verify buffer is empty (new file)
11. Verify window title shows the new filename
12. Type some content, press Cmd+S
13. Verify the new file exists on disk with the content

## Dependencies

This chunk depends on:
- `file_picker` - Provides the `resolved_path` field and file picker UI

## Risks and Open Questions

1. **UTF-8 handling**: Using `String::from_utf8_lossy` replaces invalid UTF-8 bytes with U+FFFD. This is intentional per the success criteria but may cause confusion for users editing binary files. Future work could detect binary files and refuse to open them.

2. **File read/write errors**: Currently silently ignored. This is acceptable for the MVP but should be improved in a future chunk with error reporting UI.

3. **Large file performance**: Loading a very large file will freeze the UI while reading. Consider async file loading in a future chunk.

4. **File encoding**: We assume UTF-8 encoding. Files with other encodings (UTF-16, Latin-1, etc.) will be mangled. Future work could detect encoding.

5. **Window reference**: We need to access `NSWindow` from `EditorController` to set the title. Using `metal_view.window()` should work, but need to verify the objc2 API.

6. **Initial window title**: The current app shows "lite-edit" as the initial title. We'll change this to "Untitled" when no file is associated, which is a slight behavior change.

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