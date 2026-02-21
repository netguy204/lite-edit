<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk wires together the three dependent chunks (`fuzzy_file_matcher`, `selector_widget`, `selector_rendering`) into an end-to-end Cmd+P file picker experience. The approach follows the project's Humble View Architecture:

1. **Extend `EditorState`** with an `EditorFocus` enum and focus-management fields to route input events correctly
2. **Add `FileIndex` lifecycle management** to `EditorState` (lazy init on first Cmd+P, reused thereafter)
3. **Implement `tick_picker()`** for streaming file index updates into the selector widget
4. **Modify `handle_key` and `handle_mouse`** to route events based on `EditorFocus`
5. **Handle `SelectorOutcome`** outcomes for confirmation and cancellation
6. **Create files on confirmation** when the selected path doesn't exist

The implementation maintains single-threaded ownership: `EditorState` owns all mutable state, and the focus target pattern already provides the framework for input routing. We extend it with selector-specific routing.

Tests follow the project's TDD discipline where meaningful behavior exists (focus state transitions, outcome handling) but skip tests for visual/platform integration (rendering with selector overlay).

## Sequence

### Step 1: Add `EditorFocus` enum and focus-related fields to `EditorState`

Add the focus enum and fields specified in the success criteria:

```rust
// In editor_state.rs

/// Which UI element currently owns keyboard/mouse focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorFocus {
    /// Normal buffer editing mode
    #[default]
    Buffer,
    /// Selector overlay is active (file picker, command palette, etc.)
    Selector,
}
```

Add to `EditorState`:
- `focus: EditorFocus` (default `Buffer`)
- `active_selector: Option<SelectorWidget>`
- `file_index: Option<FileIndex>`
- `last_cache_version: u64` (default `0`)
- `resolved_path: Option<PathBuf>` - stores the path resolved on selector confirmation for `file_save` chunk to consume

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Initial focus is `Buffer`
- `active_selector` is `None` initially

### Step 2: Implement Cmd+P handler to open selector

Modify `EditorState::handle_key` to intercept `Cmd+P`:

When `focus == Buffer` and event is `Key::Char('p')` with `command: true` (without Ctrl):
1. Get the current working directory via `std::env::current_dir()`
2. Initialize `file_index` if `None`: call `FileIndex::start(cwd)`
3. Query initial items: `file_index.query("")` with empty string
4. Create a new `SelectorWidget`, call `set_items()` with the query results (map `MatchResult.path` to display strings)
5. Store in `active_selector`, set `focus = Selector`
6. Record `last_cache_version = file_index.cache_version()`
7. Mark `DirtyRegion::FullViewport`

When `focus == Selector` and event is `Cmd+P`:
- Treat as Escape: close the selector (set `focus = Buffer`, clear `active_selector`, mark dirty)

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Cmd+P when focus is Buffer transitions to Selector focus
- Cmd+P when focus is Selector closes the selector (transitions to Buffer)
- After Cmd+P, `active_selector` is Some
- Cmd+P does not insert 'p' into the buffer

### Step 3: Forward key events to selector when `focus == Selector`

When `focus == Selector` in `handle_key`:
1. Forward the event to `active_selector.as_mut().unwrap().handle_key(&event)`
2. Store the previous query string before handling
3. Match on the returned `SelectorOutcome`:

**`SelectorOutcome::Pending`:**
- If query changed (compare previous query to current `widget.query()`):
  - Call `file_index.query(widget.query())` to get filtered results
  - Map results to strings and call `widget.set_items()`
  - Update `last_cache_version`
- Mark `DirtyRegion::FullViewport` (selection or query may have changed)

**`SelectorOutcome::Confirmed(idx)`:**
- Resolve the path (Step 5)
- Call `file_index.record_selection(&resolved_path)`
- Set `focus = Buffer`, clear `active_selector`
- Mark `DirtyRegion::FullViewport`
- Store `resolved_path` for `file_save` chunk

**`SelectorOutcome::Cancelled`:**
- Set `focus = Buffer`, clear `active_selector`
- Mark `DirtyRegion::FullViewport`

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Typing characters in selector appends to query and triggers re-filter
- Down arrow moves selection
- Enter with items returns Confirmed, closes selector
- Enter with empty items returns Confirmed(usize::MAX), creates new file
- Escape closes selector without changing anything

### Step 4: Forward mouse events to selector when `focus == Selector`

When `focus == Selector` in `handle_mouse`:
1. Calculate overlay geometry using `calculate_overlay_geometry()` from `selector_overlay`
2. Forward mouse event to `widget.handle_mouse(position, kind, geometry.item_height, geometry.list_origin_y)`
3. Handle `SelectorOutcome` same as keyboard (Pending/Confirmed/Cancelled)

When `focus == Buffer`:
- Existing mouse handling behavior (forward to focus target)

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Mouse click on item in selector changes selection
- Mouse click-release on selected item confirms
- Mouse events outside selector panel are ignored (return Pending)

### Step 5: Implement path resolution on confirmation

Create a helper method `resolve_picker_path(idx: usize, items: &[String], query: &str) -> PathBuf`:

1. Get current working directory
2. If `idx < items.len()`: return `cwd / items[idx]`
3. If `idx == usize::MAX` (empty items sentinel) or the query doesn't match any item:
   - Return `cwd / query` (new file)
4. If the resolved file does not exist on disk:
   - Create it as an empty file via `std::fs::File::create()`

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- Selecting an existing file returns the correct path
- Confirming with empty items and a query creates a new file
- Non-existent paths are created as empty files

### Step 6: Ignore scroll events when selector is open

Modify `handle_scroll`:
- If `focus == Selector`, return early without processing (ignore scroll)
- Otherwise, proceed with existing scroll handling

Location: `crates/editor/src/editor_state.rs`

Write test:
- Scroll events when focus is Selector do not change viewport scroll_offset

### Step 7: Implement `tick_picker()` for streaming refresh

Add method to `EditorState`:

```rust
pub fn tick_picker(&mut self) -> DirtyRegion {
    if self.focus != EditorFocus::Selector {
        return DirtyRegion::None;
    }

    let file_index = match &self.file_index {
        Some(idx) => idx,
        None => return DirtyRegion::None,
    };

    let current_version = file_index.cache_version();
    if current_version <= self.last_cache_version {
        return DirtyRegion::None;
    }

    // Re-query with current query
    let widget = self.active_selector.as_mut().unwrap();
    let results = file_index.query(widget.query());
    let items: Vec<String> = results.iter().map(|r| r.path.display().to_string()).collect();
    widget.set_items(items);
    self.last_cache_version = current_version;

    DirtyRegion::FullViewport
}
```

Location: `crates/editor/src/editor_state.rs`

Write tests for:
- `tick_picker` returns `None` when focus is Buffer
- `tick_picker` returns `None` when cache_version hasn't changed
- `tick_picker` updates items and returns `FullViewport` when cache_version increased

### Step 8: Integrate `tick_picker()` into main loop timer

Modify the cursor blink timer handler in `main.rs` to also call `tick_picker()`:

In `EditorController::toggle_cursor_blink()`:
1. Call `tick_picker()` on `self.state`
2. If it returns dirty, merge into dirty region

Alternatively, add this to a separate timer callback or combine with the existing blink timer.

Location: `crates/editor/src/main.rs`

No tests needed (platform integration).

### Step 9: Wire renderer to use `render_with_selector`

Modify `EditorController::render_if_dirty()` to check if selector is active and use `render_with_selector()`:

```rust
fn render_if_dirty(&mut self) {
    if self.state.is_dirty() {
        self.renderer.set_cursor_visible(self.state.cursor_visible);
        self.sync_renderer_buffer();

        let dirty = self.state.take_dirty_region();

        // Use render_with_selector when selector is active
        if self.state.focus == EditorFocus::Selector {
            let selector = self.state.active_selector.as_ref();
            self.renderer.render_with_selector(
                &self.metal_view,
                selector,
                self.state.cursor_visible, // cursor blink affects selector cursor too
            );
        } else {
            self.renderer.render_dirty(&self.metal_view, &dirty);
        }
    }
}
```

Location: `crates/editor/src/main.rs`

No tests needed (visual integration).

### Step 10: Manual smoke test

Verify the following by running the application:

1. Press Cmd+P, see the overlay appear with the query row and file list
2. Type partial filename, see list narrow in real-time
3. Press Down/Up, see selection move
4. Press Enter, see overlay close
5. Press Cmd+P again, type a name that doesn't exist, press Enter, verify new empty file created in working directory
6. Press Cmd+P, then Cmd+P again, verify overlay closes (toggle behavior)
7. Press Cmd+P, then Escape, verify overlay closes without side effects

## Dependencies

This chunk depends on the following completed chunks:
- `fuzzy_file_matcher` - Provides `FileIndex` for background directory walking and fuzzy queries
- `selector_widget` - Provides `SelectorWidget` for query/selection state and `SelectorOutcome`
- `selector_rendering` - Provides `calculate_overlay_geometry()` and `Renderer::render_with_selector()`

## Risks and Open Questions

1. **Current working directory stability**: We assume `std::env::current_dir()` returns a stable value throughout the session. If the working directory changes, the `FileIndex` becomes stale. For now this is acceptable; a future chunk could detect cwd changes and re-initialize the index.

2. **File creation permissions**: Creating a new file may fail due to permissions. The current design creates the file immediately on confirmation; if this fails, we should handle the error gracefully (perhaps leave `resolved_path` as `None` or log a warning).

3. **Selector cursor blink**: We reuse `cursor_visible` from `EditorState` for the selector's query cursor. This works because both use the same blink timer. If they need independent blink states, we'd need a separate field.

4. **Mouse coordinate system**: Mouse events arrive in view coordinates with y=0 at top. The overlay geometry also uses this coordinate system, so no transformation should be needed. Verify during smoke testing.

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
