<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The current `goto_cross_file_definition()` function calls `associate_file()` which
replaces the content of the current tab with the target file. This is incorrect —
it should either switch to an existing tab containing the file or create a new tab.

The fix follows the standard editor pattern for cross-file navigation:

1. **Check for existing tab**: Use `Workspace::find_tab_by_path()` to check if the
   target file is already open
2. **Switch or open**: If found, switch to that tab; if not, create a new tab with
   the target file's content
3. **Position cursor**: Set cursor to the definition position
4. **Scroll to reveal**: Use `ensure_visible_wrapped()` to scroll the viewport

We also need to enhance `go_back()` to support cross-tab navigation, since the
jump stack records tab IDs but the current implementation only handles same-tab
jumps.

The implementation will build on existing patterns:
- `Workspace::find_tab_by_path()` from chunk `base_snapshot_reload`
- `Workspace::add_tab()` and `Pane::switch_tab()` for tab management
- `Viewport::ensure_visible_wrapped()` from the `viewport_scroll` subsystem

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk USES the viewport
  scroll subsystem's `ensure_visible_wrapped()` to scroll the target file's viewport
  to reveal the definition. The relationship is marked as `uses` in the GOAL.md
  frontmatter.

## Sequence

### Step 1: Add `switch_to_tab_by_id` method to Workspace

Add a method that finds a tab by ID across all panes and switches to it:
- Search all panes for a tab with the given ID
- If found, set that pane as active and switch to that tab within the pane
- Return `true` if the tab was found and switched to, `false` otherwise

Location: `crates/editor/src/workspace.rs`

This method is needed by both `goto_cross_file_definition()` (to switch to an
existing tab) and `go_back()` (to navigate to a different tab from the jump stack).

### Step 2: Add `open_file_in_new_tab` helper to EditorState

Create a helper method that:
- Creates a new file tab with the given path
- Loads the file content into the tab's buffer
- Sets up syntax highlighting
- Adds the tab to the workspace
- Syncs viewport dimensions

This consolidates logic currently split across `associate_file()` and `new_tab()`,
and provides a clean API for opening a file in a new tab rather than replacing
the current tab's content.

Location: `crates/editor/src/editor_state.rs`

### Step 3: Rewrite `goto_cross_file_definition` to properly navigate

Replace the current implementation that calls `associate_file()`:

1. Push current position to jump stack (existing code)
2. Check if target file is already open: `workspace.find_tab_by_path(&target_file)`
3. If found:
   - Call `workspace.switch_to_tab_by_id(tab_id)` to switch to it
4. If not found:
   - Call `open_file_in_new_tab(target_file)` to open in a new tab
5. Move cursor to definition position (existing code, but now on correct tab)
6. Call `ensure_cursor_visible()` or equivalent to scroll viewport

Location: `crates/editor/src/editor_state.rs`

### Step 4: Enhance `go_back` to support cross-tab navigation

The current `go_back()` only handles same-tab jumps (it checks `tab.id == pos.tab_id`
and does nothing if they differ). Extend it to:

1. Pop from jump stack (existing)
2. Check if target tab ID matches current tab
3. If different, call `workspace.switch_to_tab_by_id(pos.tab_id)`
4. Set cursor to saved position (existing)
5. Scroll to reveal cursor position

This completes the navigation round-trip: goto-definition can cross files, and
go-back can return to the original file.

Location: `crates/editor/src/editor_state.rs`

### Step 5: Add ensure_cursor_visible helper

Create a helper method that triggers viewport scrolling to reveal the current cursor
position after navigation. This should:

1. Get the active tab's buffer and viewport
2. Build the necessary context for `ensure_visible_wrapped()` (cursor position,
   line count, wrap layout, line length function)
3. Call `ensure_visible_wrapped()`
4. Mark viewport as dirty if scrolling occurred

This will be called after setting the cursor in both `goto_cross_file_definition()`
and `go_back()`.

Location: `crates/editor/src/editor_state.rs`

### Step 6: Write unit tests

Add tests per docs/trunk/TESTING_PHILOSOPHY.md that verify the semantic behavior:

**Test: Cross-file goto opens new tab (target not already open)**
- Set up: workspace with one tab containing file A
- Action: call `goto_cross_file_definition` with target file B
- Assert: workspace now has two tabs, active tab is file B, cursor at definition

**Test: Cross-file goto switches to existing tab**
- Set up: workspace with tab A (active) and tab B
- Action: call `goto_cross_file_definition` targeting file B
- Assert: still two tabs, active tab is now B, cursor at definition

**Test: Cross-file goto preserves original file**
- Set up: workspace with tab A containing edits
- Action: call `goto_cross_file_definition` targeting file B
- Assert: tab A still exists with original content unchanged

**Test: Go-back navigates to different tab**
- Set up: workspace with tabs A and B, jump stack has entry for A, active is B
- Action: call `go_back()`
- Assert: active tab is now A, cursor at jump stack position

**Test: Go-back + goto round-trip**
- Set up: file A with symbol referencing definition in file B
- Action: goto_definition from A, then go_back
- Assert: back in file A at original position

Location: `crates/editor/src/editor_state.rs` (in `#[cfg(test)]` module)

## Dependencies

This chunk depends on:
- `treesitter_symbol_index` (ACTIVE): Provides the cross-file symbol lookup and
  calls `goto_cross_file_definition()` with the target file/position
- `treesitter_gotodef` (ACTIVE): Original goto-definition implementation with
  jump stack

Both are already ACTIVE (merged), so no blocking dependencies.

## Risks and Open Questions

1. **Multi-pane navigation**: The current design searches all panes for a tab by path.
   When the target file is open in a different pane, should we switch to that pane
   or open a new tab in the current pane? The simpler approach (switch to existing
   tab's pane) maintains the principle of not duplicating open files.

2. **File loading errors**: If the target file cannot be read when opening a new tab,
   how should we handle this? Options:
   - Show a status message and abort navigation
   - Open an empty tab with the path set (consistent with `associate_file` behavior)
   We'll follow the existing `associate_file` pattern of gracefully handling read
   errors.

3. **Jump stack tab ID validity**: The jump stack stores tab IDs, but tabs can be
   closed. If `go_back()` tries to navigate to a closed tab, it should silently
   skip that entry (current behavior is acceptable).

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
-->