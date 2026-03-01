<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The bug is a missing call to `ensure_visible` after cursor movement in the goto-definition
flow. The existing `goto_definition()` method (and its helper `goto_cross_file_definition()`)
correctly moves the cursor with `buffer.set_cursor()` and merges `InvalidationKind::Layout`,
but never scrolls the viewport to reveal the new cursor position.

The fix follows the established pattern used elsewhere in `editor_state.rs`, such as in
`run_live_search()` (line ~1894) where `self.viewport_mut().ensure_visible_with_margin()`
is called after cursor movement. We'll add `ensure_visible()` calls at the three jump sites:

1. **Same-file jump** (locals resolution) - after setting cursor at definition position
2. **Cross-file jump** - in `goto_cross_file_definition()` after opening file and setting cursor
3. **Go-back navigation** - in `go_back()` after restoring cursor from jump stack

For wrap-mode compatibility, we use `Viewport::ensure_visible()` which handles the common
unwrapped case. The `ensure_visible_wrapped()` variant requires `WrapLayout` context that
isn't readily available at these call sites, and the simpler variant is sufficient for
discrete navigation operations (as opposed to continuous typing where wrap-row tracking
matters more).

## Subsystem Considerations

- **docs/subsystems/viewport_scroll** (DOCUMENTED): This chunk IMPLEMENTS the viewport
  scroll subsystem's `ensure_visible()` pattern at three additional call sites.

The viewport_scroll subsystem's invariant #6 states: "`ensure_visible` snaps to whole-row
boundaries." This chunk uses `Viewport::ensure_visible()` to scroll after goto-definition
jumps, following the established pattern used in find-in-file and other cursor-movement
operations.

## Sequence

### Step 1: Add test for same-file goto-definition scroll reveal

Write a failing test that verifies the viewport scrolls after a same-file
goto-definition jump. The test should:

1. Create an `EditorState` with a buffer containing many lines (e.g., 50 lines)
2. Set up a definition at line 40 (off-screen when viewport shows lines 0-9)
3. Position cursor at a reference to that definition near line 0
4. Call `goto_definition()` (simulating F12)
5. Assert that `first_visible_line()` has scrolled to reveal line 40

Location: `crates/editor/src/editor_state.rs` in the `#[cfg(test)]` module

Note: Since `goto_definition` requires tree-sitter locals queries, this test
may need to use mock/simplified resolution or test at a different abstraction
level. Consider testing the scroll behavior by directly calling the internal
cursor-setting and scroll logic.

### Step 2: Fix same-file goto-definition scroll

In `EditorState::goto_definition()`, after setting the cursor to the definition
position (~line 1442), add a call to `ensure_visible`:

```rust
// Move cursor to definition
let tab = workspace.active_tab_mut().unwrap();
if let Some(buffer) = tab.as_text_buffer_mut() {
    buffer.set_cursor(Position::new(def_line, def_col));
}

// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor
let workspace = self.editor.active_workspace_mut().unwrap();
let tab = workspace.active_tab().unwrap();
let line_count = tab.as_text_buffer().map(|b| b.line_count()).unwrap_or(0);
let viewport = workspace.active_pane_mut().unwrap().active_tab_mut().unwrap();
if viewport.viewport.ensure_visible(def_line, line_count) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Location: `crates/editor/src/editor_state.rs`, `goto_definition()` method

### Step 3: Add test for cross-file goto-definition scroll reveal

Write a test that verifies viewport scrolls after `goto_cross_file_definition()`.
This is harder to test in isolation since it involves file opening, but the
scroll pattern is identical. Consider testing that after the method completes,
the viewport's `first_visible_line()` has been adjusted appropriately.

### Step 4: Fix cross-file goto-definition scroll

In `EditorState::goto_cross_file_definition()`, after setting the cursor to
the target position (~line 1529), add the same scroll-to-reveal pattern:

```rust
// Move cursor to the definition position
if let Some(ws) = self.editor.active_workspace_mut() {
    if let Some(tab) = ws.active_tab_mut() {
        if let Some(buffer) = tab.as_text_buffer_mut() {
            buffer.set_cursor(Position::new(target_line, target_col));
        }
    }
}

// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor
let line_count = self.buffer().line_count();
if self.viewport_mut().ensure_visible(target_line, line_count) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Location: `crates/editor/src/editor_state.rs`, `goto_cross_file_definition()` method

### Step 5: Add test for go-back navigation scroll reveal

Write a test that verifies viewport scrolls after `go_back()` returns to a
previous cursor position that's now off-screen.

### Step 6: Fix go-back navigation scroll

In `EditorState::go_back()`, after restoring the cursor position from the
jump stack (~line 1611), add scroll-to-reveal:

```rust
// Restore cursor position
if let Some(buffer) = tab.as_text_buffer_mut() {
    buffer.set_cursor(Position::new(pos.line, pos.col));
}

// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor after go-back
let line_count = tab.as_text_buffer().map(|b| b.line_count()).unwrap_or(0);
if tab.viewport.ensure_visible(pos.line, line_count) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Location: `crates/editor/src/editor_state.rs`, `go_back()` method

### Step 7: Manual verification

Test the fix manually:

1. Open a large Rust file (e.g., `crates/editor/src/editor_state.rs`)
2. Scroll to a function near the top of the file
3. Cmd+click on a symbol whose definition is hundreds of lines away
4. Verify the viewport scrolls to show the definition

Repeat for:
- Same-file definitions (locals resolution)
- Cross-file definitions (symbol index)
- Go-back (Ctrl+-) returning to original position

## Dependencies

None. The `Viewport::ensure_visible()` method already exists and is used elsewhere
in the codebase. This chunk only adds calls to it at new locations.

## Risks and Open Questions

1. **Wrap mode handling**: The `ensure_visible()` method uses unwrapped line-to-screen
   mapping. For most goto-definition use cases (jumping to function/type definitions),
   this is fine because definitions typically start at column 0 or low columns.
   However, if the definition is deep within a very long wrapped line, the cursor
   might land on a screen row that's still off-screen. The `ensure_visible_wrapped()`
   variant would handle this correctly but requires `WrapLayout` context that isn't
   readily available at the call sites.

   **Decision**: Use `ensure_visible()` for now. This is consistent with how
   `run_live_search` handles scroll-to-match. If users report issues with wrapped
   definitions, consider creating a helper method that constructs the necessary
   `WrapLayout` context.

2. **Borrow checker complexity**: The `goto_definition()` method has complex borrow
   patterns due to multiple mutable borrows of `self.editor`. The `ensure_visible`
   call needs access to both the viewport (via active tab) and the line count
   (via buffer). May need to restructure code to avoid borrow conflicts.

3. **Double invalidation**: The current code already calls `self.invalidation.merge(InvalidationKind::Layout)`. Adding another conditional merge when scrolling
   occurs is harmless (Layout merges with itself) but may look redundant. Consider
   whether to remove the unconditional merge and rely only on the scroll-triggered one.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->