<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Add a new `EditorState` method `setup_all_tab_highlighting()` that iterates
every workspace → every pane → every tab and calls `Tab::setup_highlighting()`
with the `LanguageRegistry` and `SyntaxTheme`. Call this method from `main.rs`
immediately after session restore, alongside the existing
`initialize_symbol_indexing_for_all_workspaces()` call.

This mirrors the established pattern: `gotodef_session_restore` added
`initialize_symbol_indexing_for_all_workspaces()` at the same call site to fix
a similar "works on normal open, missing after restore" gap. The fix follows
the same structure — a post-restore initialization method on `EditorState`
that walks all workspaces.

The implementation keeps `session.rs` focused on data restoration (no
highlighting concern leaks in) and puts the highlighting setup in
`editor_state.rs` where `language_registry` and the theme are naturally
accessible.

Testing follows TDD per TESTING_PHILOSOPHY.md: a unit test in `session.rs`
verifies that restored tabs have `highlighter: None` (confirming the bug
exists at the data layer), then an integration test verifies that after
calling the new method, all restored tabs have highlighters for recognized
extensions.

<!-- No subsystems are relevant to this chunk. The renderer, spatial_layout,
     and viewport_scroll subsystems are not touched by this fix. -->

## Sequence

### Step 1: Write a failing test confirming the bug

Add a unit test in `crates/editor/src/session.rs` (in the existing `#[cfg(test)]`
module) that:

1. Creates a `SessionData` with a workspace containing a `.rs` file tab
2. Calls `restore_into_editor()` to get an `Editor`
3. Asserts that the restored tab's `highlighter` is `None`

This test documents the current broken behavior and will remain as a
characterization test (it asserts what session.rs produces — no highlighter —
since session.rs doesn't have access to `LanguageRegistry`).

Location: `crates/editor/src/session.rs` test module

### Step 2: Add `setup_all_tab_highlighting()` to EditorState

Add a new public method to `EditorState` in `crates/editor/src/editor_state.rs`:

```rust
// Chunk: docs/chunks/highlight_restore - Apply highlighting to all restored tabs
/// Sets up syntax highlighting for all file tabs across all workspaces.
///
/// This should be called after session restore to ensure all restored tabs
/// have syntax highlighting. During normal file open, `setup_highlighting()`
/// is called per-tab, but session restore creates tabs without highlighters.
pub fn setup_all_tab_highlighting(&mut self) {
    let theme = SyntaxTheme::catppuccin_mocha();
    for ws in &mut self.editor.workspaces {
        for pane in ws.all_panes_mut() {
            for tab in &mut pane.tabs {
                tab.setup_highlighting(&self.language_registry, theme);
            }
        }
    }
}
```

This follows the exact same traversal pattern as
`initialize_symbol_indexing_for_all_workspaces()` but goes one level deeper
(into tabs within panes). The `setup_highlighting()` call on each tab is a
no-op for tabs without a file path or unrecognized extensions (returns `false`),
so no filtering is needed.

**Borrow checker note:** `self.language_registry` is an `Arc<LanguageRegistry>`,
so we can take a shared reference to it while mutably iterating `self.editor.workspaces`.
The `theme` is a value type (`SyntaxTheme`) created before the loop. No borrow
conflicts.

Location: `crates/editor/src/editor_state.rs`, near `initialize_symbol_indexing_for_all_workspaces()`

### Step 3: Call `setup_all_tab_highlighting()` after session restore in main.rs

In `crates/editor/src/main.rs`, at the session restore success path (around
line 413, after `state.initialize_symbol_indexing_for_all_workspaces()`), add:

```rust
// Chunk: docs/chunks/highlight_restore - Apply highlighting to restored tabs
state.setup_all_tab_highlighting();
```

This mirrors the pattern of `initialize_symbol_indexing_for_all_workspaces()`
being called at the same location.

Location: `crates/editor/src/main.rs`, inside the `Ok(editor)` arm of session restore

### Step 4: Write an integration test for post-restore highlighting

Add a test in `crates/editor/tests/session_persistence.rs` that verifies
end-to-end highlighting after restore:

1. Create a temp directory with `.rs` and `.py` files (recognized extensions)
   and a `.xyz` file (unrecognized)
2. Build a `SessionData` referencing those files
3. Call `restore_into_editor()` to get an `Editor`
4. Create an `EditorState` with `new_deferred()`, assign the restored editor
5. Call `setup_all_tab_highlighting()`
6. Assert that the `.rs` tab has `highlighter.is_some()`
7. Assert that the `.py` tab has `highlighter.is_some()`
8. Assert that the `.xyz` tab has `highlighter.is_none()` (unrecognized extension)

This test verifies the success criteria: restored buffers have syntax
highlighting immediately after session restore for recognized languages.

Location: `crates/editor/tests/session_persistence.rs`

### Step 5: Build and run all tests

Run `cargo build` and `cargo test` to verify:
- The new method compiles
- The integration test passes
- No regressions in existing session or highlighting tests

## Dependencies

None. All required infrastructure exists:
- `Tab::setup_highlighting()` is already implemented
- `LanguageRegistry` is already constructed in `EditorState`
- `SyntaxTheme::catppuccin_mocha()` is already the standard theme
- `Workspace::all_panes_mut()` provides the traversal API

## Risks and Open Questions

- **Startup latency**: `setup_highlighting()` creates a tree-sitter parser and
  does an initial full parse per tab. For sessions with many tabs (e.g., 20+
  files), this could add noticeable startup delay. Mitigation: tree-sitter
  parsing is fast (sub-millisecond for typical files), and this matches what
  already happens when opening files individually. If it becomes a problem,
  lazy initialization (highlight on first render) would be a separate chunk.

- **Theme hardcoding**: All call sites use `SyntaxTheme::catppuccin_mocha()`.
  This is an existing pattern, not a new one. If theme selection is added later,
  this method would need updating alongside all other call sites.

## Deviations

- Step 2: `SyntaxTheme` is not `Copy` or `Clone`, so `catppuccin_mocha()` must
  be called per-tab rather than once before the loop. No functional impact.

- Step 4: Integration test placed in `editor_state.rs` test module instead of
  `session_persistence.rs`. `EditorState` is a private module in `main.rs` (not
  exported via `lib.rs`), so integration tests in `crates/editor/tests/` cannot
  access it. Moving the test to `editor_state.rs` keeps the same coverage.

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