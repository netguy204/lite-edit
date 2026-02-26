<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a small, targeted bug fix in `Editor::should_show_welcome_screen()`. The current implementation checks only two conditions:

1. Is the active tab a `TabKind::File`?
2. Is the tab's `TextBuffer` empty?

The fix adds a third condition: is the tab **not** backed by a file on disk (`associated_file.is_none()`)?

**Change summary:**
- The welcome screen is intended for fresh, unassociated tabs — not for viewing empty files
- When `Tab::associated_file` is `Some(_)`, the tab represents an actual file (even if zero-byte), so we show the empty buffer, not the welcome screen
- When `Tab::associated_file` is `None`, the tab is a new, unsaved scratch buffer, so we show the welcome screen as an orientation aid

**TDD approach per TESTING_PHILOSOPHY.md:**
1. Write failing tests first that assert the new file-backed behavior
2. Modify `should_show_welcome_screen()` to make them pass
3. Verify existing tests still pass

## Sequence

### Step 1: Write failing tests for file-backed empty tabs

Add two test cases to `crates/editor/src/workspace.rs` (in the existing `#[cfg(test)] mod tests` block):

1. `test_welcome_screen_not_shown_for_empty_file_backed_tab`: Create a tab with an empty `TextBuffer` but `associated_file: Some(path)`. Assert `should_show_welcome_screen()` returns `false`.

2. `test_welcome_screen_shown_for_empty_unassociated_tab`: Create a tab with an empty `TextBuffer` and `associated_file: None`. Assert `should_show_welcome_screen()` returns `true`. (This tests the existing behavior to prevent regression.)

These tests will fail initially because the current implementation returns `true` for any empty File tab regardless of `associated_file`.

Location: `crates/editor/src/workspace.rs#tests`

### Step 2: Update `should_show_welcome_screen()` to check `associated_file`

Modify `Editor::should_show_welcome_screen()` at line ~1184 to add the `associated_file` check:

```rust
pub fn should_show_welcome_screen(&self) -> bool {
    let workspace = match self.active_workspace() {
        Some(ws) => ws,
        None => return false,
    };

    let tab = match workspace.active_tab() {
        Some(t) => t,
        None => return false,
    };

    // Only show welcome screen for File tabs
    if tab.kind != TabKind::File {
        return false;
    }

    // Don't show welcome screen for file-backed tabs (even if empty)
    // The welcome screen is for fresh, unassociated scratch buffers only
    if tab.associated_file.is_some() {
        return false;
    }

    // Check if the buffer is empty
    match tab.as_text_buffer() {
        Some(buffer) => buffer.is_empty(),
        None => false,
    }
}
```

Also update the doc comment to reflect the new semantics:

```rust
/// Returns true if the welcome screen should be shown for the active tab.
///
/// The welcome screen is displayed when:
/// - There is an active workspace with an active tab
/// - The active tab is a File tab (not Terminal or AgentOutput)
/// - The tab is NOT backed by a file on disk (associated_file is None)
/// - The tab's TextBuffer is empty
///
/// This provides a Vim-style welcome/intro screen on initial launch and
/// when creating new empty tabs. File-backed tabs (even if empty) show
/// their actual (empty) contents instead.
```

Location: `crates/editor/src/workspace.rs#Editor::should_show_welcome_screen`

### Step 3: Verify tests pass and behavior is correct

Run the test suite to confirm:
- The two new tests pass
- Existing welcome screen tests (`test_welcome_screen_scroll_updates_offset`, `test_welcome_screen_scroll_clamps_at_zero`, `test_non_welcome_scroll_uses_viewport`) still pass
- No regressions in other tests

```bash
cargo test -p lite-edit-editor should_show_welcome
cargo test -p lite-edit-editor welcome_screen
```

### Step 4: Update GOAL.md code_paths and code_references

Add the file path and code reference to the chunk's GOAL.md frontmatter:

```yaml
code_paths:
- crates/editor/src/workspace.rs

code_references:
  - ref: crates/editor/src/workspace.rs#Editor::should_show_welcome_screen
    implements: "Welcome screen visibility excludes file-backed empty tabs"
```

Location: `docs/chunks/welcome_file_backed/GOAL.md`

---

**BACKREFERENCE COMMENTS**

The method-level backreference comment should be updated to reference this chunk:

```rust
// Chunk: docs/chunks/welcome_file_backed - Exclude file-backed tabs from welcome screen
pub fn should_show_welcome_screen(&self) -> bool {
```

Note: The existing method already has a backreference from `docs/chunks/welcome_screen`. The new chunk reference can be added as a second comment line, or we can rely on the code_references in GOAL.md rather than cluttering the code with multiple chunk backreferences for the same method.

## Risks and Open Questions

1. **Test isolation**: The tests need to create an `Editor` with a specific tab configuration. Need to verify that the existing test helpers (`Editor::new()` or similar) can be used to set up an empty file tab with a specific `associated_file` value. If not, may need to use `Tab::new_file()` directly.

2. **Existing test assumptions**: The existing `test_welcome_screen_scroll_*` tests in `editor_state.rs` use `EditorState::empty()` which creates an unassociated tab. These should continue to pass unchanged. However, if any tests inadvertently rely on the old behavior (empty file-backed tabs showing welcome screen), they would need to be updated.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
