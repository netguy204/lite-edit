<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a small, focused bug fix that changes the timing of two operations:

1. **Eager FileIndex initialization** — Move `FileIndex::start(cwd)` from `open_file_picker()` (first Cmd+P) to `EditorState::new()` (app startup). This ensures the background walk has time to populate the cache before the user ever opens the picker.

2. **Aggressive tick_picker polling** — Call `tick_picker()` after every user event (key, mouse, scroll) in addition to the existing blink-timer call. This ensures that cache updates surface immediately when the user interacts, rather than waiting up to 500ms for the next blink tick.

The approach builds on the existing code structure without changing any public APIs. All modifications are internal to `EditorState` and `EditorController`.

## Sequence

### Step 1: Change `file_index` initialization to construction time

In `crates/editor/src/editor_state.rs`, modify `EditorState::new()` to initialize `file_index` eagerly:

**Current code** (lines 224-225):
```rust
file_index: None,
last_cache_version: 0,
```

**New code**:
```rust
file_index: Some(FileIndex::start(
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
)),
last_cache_version: 0,
```

This starts the background walk immediately at app startup. By the time the user presses Cmd+P (typically several seconds later), the walk will have discovered most or all files.

### Step 2: Simplify `open_file_picker()` to remove conditional initialization

In `crates/editor/src/editor_state.rs`, remove the conditional `FileIndex` creation from `open_file_picker()`:

**Current code** (lines 442-449):
```rust
fn open_file_picker(&mut self) {
    // Get the current working directory
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Initialize file_index if needed
    if self.file_index.is_none() {
        self.file_index = Some(FileIndex::start(cwd.clone()));
    }
```

**New code**:
```rust
fn open_file_picker(&mut self) {
    // file_index is initialized eagerly at EditorState construction time;
    // Chunk: docs/chunks/picker_eager_index
```

Note: Remove the `cwd` variable entirely since it's no longer used. The rest of the function remains unchanged — it still queries the existing `file_index` and creates the selector widget.

### Step 3: Add `tick_picker` call to `handle_key` in EditorController

In `crates/editor/src/main.rs`, call `tick_picker()` at the end of `handle_key()`:

**Current code** (lines 215-225):
```rust
fn handle_key(&mut self, event: KeyEvent) {
    self.state.handle_key(event);

    // Check for quit request
    if self.state.should_quit {
        self.terminate_app();
        return;
    }

    self.render_if_dirty();
}
```

**New code**:
```rust
fn handle_key(&mut self, event: KeyEvent) {
    self.state.handle_key(event);

    // Check for quit request
    if self.state.should_quit {
        self.terminate_app();
        return;
    }

    // Poll for file index updates so picker results stream in on every keystroke
    // Chunk: docs/chunks/picker_eager_index
    let picker_dirty = self.state.tick_picker();
    if picker_dirty.is_dirty() {
        self.state.dirty_region.merge(picker_dirty);
    }

    self.render_if_dirty();
}
```

### Step 4: Add `tick_picker` call to `handle_mouse` in EditorController

In `crates/editor/src/main.rs`, call `tick_picker()` at the end of `handle_mouse()`:

**Current code** (lines 228-231):
```rust
fn handle_mouse(&mut self, event: MouseEvent) {
    self.state.handle_mouse(event);
    self.render_if_dirty();
}
```

**New code**:
```rust
fn handle_mouse(&mut self, event: MouseEvent) {
    self.state.handle_mouse(event);

    // Poll for file index updates so picker results stream in on mouse interaction
    // Chunk: docs/chunks/picker_eager_index
    let picker_dirty = self.state.tick_picker();
    if picker_dirty.is_dirty() {
        self.state.dirty_region.merge(picker_dirty);
    }

    self.render_if_dirty();
}
```

### Step 5: Add `tick_picker` call to `handle_scroll` in EditorController

In `crates/editor/src/main.rs`, call `tick_picker()` at the end of `handle_scroll()`:

**Current code** (lines 236-239):
```rust
fn handle_scroll(&mut self, delta: ScrollDelta) {
    self.state.handle_scroll(delta);
    self.render_if_dirty();
}
```

**New code**:
```rust
fn handle_scroll(&mut self, delta: ScrollDelta) {
    self.state.handle_scroll(delta);

    // Poll for file index updates so picker results stream in on scroll
    // Chunk: docs/chunks/picker_eager_index
    let picker_dirty = self.state.tick_picker();
    if picker_dirty.is_dirty() {
        self.state.dirty_region.merge(picker_dirty);
    }

    self.render_if_dirty();
}
```

### Step 6: Run existing tests to verify no regressions

```bash
cargo test --package editor
```

All existing tests should pass. The behavioral changes are:
- `FileIndex` is now always initialized (was lazy) — tests that create `EditorState` may see different timing, but `tick_picker()` already handles both `Some` and `None` cases.
- `tick_picker()` is called more frequently — this is idempotent (returns `DirtyRegion::None` when no updates are needed).

### Step 7: Manual smoke test

1. Build and run the editor: `cargo run`
2. Press Cmd+P immediately after launch (within ~100ms)
3. Verify the picker shows the full file list (or a growing list if the walk is still running), not just recency entries
4. Type a partial filename, verify results filter correctly
5. Press Escape, wait a few seconds, press Cmd+P again, verify behavior is consistent
6. If the project has many files, verify results stream in during initial walk (may require a larger project to observe)

## Dependencies

None. This chunk builds on the existing `FileIndex`, `EditorState`, and `EditorController` infrastructure from the `fuzzy_file_matcher` and `file_picker` chunks.

## Risks and Open Questions

- **Test timing sensitivity**: Some tests may have implicit assumptions about when `file_index` is initialized. The change from `None` to `Some(...)` at construction time could affect tests that check `file_index.is_none()`. Review test code to confirm no such assertions exist.

- **CWD changes after startup**: If the working directory changes after `EditorState` construction (unlikely in practice), the `FileIndex` will continue indexing the original directory. This is pre-existing behavior (the lazy initialization also used `cwd` at first Cmd+P time, not at each Cmd+P time), so no change in behavior. If this becomes a concern, it could be addressed in a future chunk.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
