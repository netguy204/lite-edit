<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is a viewport sizing initialization issue. When a `Tab` is created, its
`Viewport` is initialized with `visible_lines = 0`. The viewport only receives
its correct dimensions when `Viewport::update_size` is called, which happens on
window resize and initial setup—but only for the *active* tab at that moment.

Tabs created with Cmd+T, tabs switched to via `switch_tab`, and tabs opened via
the file picker never have their viewports sized if no resize event occurred.
When `visible_lines = 0`, `dirty_lines_to_region` computes an empty visible
range `[first_visible_line, first_visible_line + 0)`, and every `DirtyLines`
variant maps to `DirtyRegion::None`. This causes `render_if_dirty` to skip
repaints after mouse clicks.

**Fix strategy**: Propagate the current viewport dimensions to the newly active
tab's viewport whenever tab activation occurs. The `view_height` field in
`EditorState` holds the current window height (updated on every resize), so we
call `update_viewport_size` on the new tab's viewport at these points:

1. `EditorState::new_tab` — after `add_tab` switches to the new tab
2. `EditorState::switch_tab` — after `workspace.switch_tab(index)`
3. `EditorState::associate_file` — which opens files from the picker into the
   current tab (the current tab's viewport should already be sized, but we
   ensure consistency)

Each of these points calls `self.viewport_mut().update_size(...)` with the
stored `view_height` and the buffer's line count to properly initialize the
newly active viewport.

## Subsystem Considerations

No subsystems are directly relevant. The fix is a localized change to
`EditorState` methods.

## Sequence

### Step 1: Add helper method to sync viewport size

Create a helper method `EditorState::sync_active_tab_viewport` that:
- Gets `self.view_height`
- Gets the active tab's buffer line count
- Calls `self.viewport_mut().update_size(self.view_height, line_count)`

This helper reduces code duplication and ensures consistent viewport sizing.

Location: `crates/editor/src/editor_state.rs`

### Step 2: Call the helper in `new_tab`

After `workspace.add_tab(new_tab)` and before `ensure_active_tab_visible`, call
the helper to size the new tab's viewport.

Location: `crates/editor/src/editor_state.rs`, inside `new_tab`

### Step 3: Call the helper in `switch_tab`

After `workspace.switch_tab(index)` (and before marking dirty), call the helper
to ensure the switched-to tab has the correct viewport dimensions.

Location: `crates/editor/src/editor_state.rs`, inside `switch_tab`

### Step 4: Ensure `associate_file` calls the helper

In `associate_file`, after loading the file content into the buffer and calling
`scroll_to(0, line_count)`, also call the helper. This handles the file picker
flow where a new file is opened into the current tab.

**Note**: The current tab's viewport is already sized if the user was editing
in it, but the file picker might open into a newly created tab (Cmd+T then
Cmd+P), so this ensures consistency.

Location: `crates/editor/src/editor_state.rs`, inside `associate_file`

### Step 5: Write regression test for Cmd+T flow

Add a test `test_new_tab_viewport_is_sized`:
1. Create `EditorState`, call `update_viewport_size(160.0)` (10 visible lines at
   line height 16)
2. Call `new_tab()` to create a second tab
3. Assert that the new tab's `viewport().visible_lines()` equals 10 (not 0)
4. Insert some text into the buffer
5. Simulate a mouse click that would place the cursor on line 5
6. Assert that the dirty region is NOT `None` (the viewport can compute dirty
   lines correctly because `visible_lines` is correct)

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module

### Step 6: Write regression test for switch_tab flow

Add a test `test_switch_tab_viewport_is_sized`:
1. Create `EditorState`, call `update_viewport_size(160.0)`
2. Create a second tab with some text
3. Switch back to tab 0, then switch to tab 1
4. Assert `viewport().visible_lines()` is correct
5. Call `ctx.mark_cursor_dirty()` and assert the dirty region is NOT `None`

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module

### Step 7: Write regression test for file picker confirmation

Add a test `test_associate_file_viewport_is_sized`:
1. Create a temporary file with known content
2. Create `EditorState`, call `update_viewport_size`
3. Call `new_tab()`, then immediately call `associate_file` with the temp file
4. Assert `viewport().visible_lines()` is correct (not 0)
5. Assert a cursor dirty mark produces a non-None region

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module

### Step 8: Run all tests to verify no regressions

Run `cargo test -p editor` to ensure all existing viewport and click-positioning
tests continue to pass.

## Dependencies

None. This chunk modifies existing code paths in `EditorState`.

## Risks and Open Questions

- **Terminal tabs**: Terminal tabs (`TabBuffer::Terminal`) do not have a
  `TextBuffer`, so `buffer().line_count()` will panic. The fix must handle this
  by checking `as_text_buffer()` and skipping the viewport sync for non-file
  tabs, or by using a fallback line count.

  **Resolution**: The helper should gracefully handle non-file tabs by either:
  (a) Skipping the sync if no text buffer exists, or (b) Using a reasonable
  default line count for terminal buffers. Since terminal tabs use a different
  rendering path and don't have the same dirty region tracking, option (a) is
  safer.

- **Existing tests**: Existing tests call `update_viewport_size` explicitly
  after creating `EditorState`, so they should not be affected. However, we
  should verify that no test relies on `visible_lines` being 0 for some
  intermediate state.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.

When reality diverges from the plan, document it here.
-->
