<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

Route `TextInputEvent` through the focus system by adding a `handle_text_input()` method
to `FocusTarget` and `FocusStack`, paralleling the existing `handle_key()` pattern.
The fix removes the early return in `EditorState::handle_insert_text()` and instead
dispatches to the focus stack when the active layer needs text input.

**Strategy**: Extend the existing `FocusTarget` trait with text input handling. The
`MiniBuffer` already provides character insertion via `handle_key()` with `KeyEvent::char()`,
but on macOS, regular typing flows through `insertText:` → `TextInputEvent`, not `KeyEvent`.
We need to bridge `TextInputEvent` to the minibuffer's `handle_key()` path.

**Design choice**: Rather than adding a separate `handle_text_input()` method to
`FocusTarget`, we'll convert `TextInputEvent` to synthetic `KeyEvent::char()` calls
inside `EditorState::handle_insert_text()`. This is simpler and reuses existing
minibuffer affordances. The decision about which target receives the text is made
based on `FocusLayer`, not by full stack dispatch.

**Testing approach**: Per TESTING_PHILOSOPHY.md's "humble view" architecture, the
text input routing logic is testable without platform dependencies. We test by
constructing state, calling `handle_insert_text()`, and asserting on results.

## Sequence

### Step 1: Add text input method to MiniBuffer

Add a `handle_text_input(&mut self, text: &str)` method to `MiniBuffer` that converts
the text string to synthetic `KeyEvent::char()` calls. This enables character-by-character
insertion while reusing all existing affordances (cursor management, selection replacement).

**Location**: `crates/editor/src/mini_buffer.rs`

**Implementation**:
```rust
// Chunk: docs/chunks/minibuffer_input - Text input support for MiniBuffer
/// Handles text input (from IME, keyboard, paste).
///
/// Converts the text string to character key events and inserts them.
/// This reuses the existing key handling logic for cursor management,
/// selection replacement, and dirty tracking.
pub fn handle_text_input(&mut self, text: &str) {
    for ch in text.chars() {
        self.handle_key(KeyEvent::char(ch));
    }
}
```

**Tests** (TDD - write first):
- `test_handle_text_input_single_char`: `handle_text_input("a")` results in content "a"
- `test_handle_text_input_string`: `handle_text_input("hello")` results in content "hello"
- `test_handle_text_input_unicode`: `handle_text_input("日本語")` inserts correctly
- `test_handle_text_input_replaces_selection`: select all, then `handle_text_input("x")` replaces

### Step 2: Add text input method to SelectorWidget

Add `handle_text_input(&mut self, text: &str)` to `SelectorWidget` that delegates to its
`MiniBuffer`. This is the entry point for text input when the selector is focused.

**Location**: `crates/editor/src/selector.rs`

**Implementation**:
```rust
// Chunk: docs/chunks/minibuffer_input - Text input support for selector
/// Handles text input events (from IME, keyboard, paste).
///
/// Inserts text into the query field and resets selection to index 0
/// if the query changed. Use this for macOS `insertText:` events.
pub fn handle_text_input(&mut self, text: &str) {
    let prev_query = self.mini_buffer.content();
    self.mini_buffer.handle_text_input(text);
    if self.mini_buffer.content() != prev_query {
        self.selected_index = 0;
    }
}
```

**Tests** (TDD - write first):
- `test_handle_text_input_updates_query`: `handle_text_input("foo")` updates `query()` to "foo"
- `test_handle_text_input_resets_index`: with items, `handle_text_input("a")` resets selected_index to 0
- `test_handle_text_input_filters_results`: integration test showing text input → query change → re-filter

### Step 3: Add text input method to FindFocusTarget

Add `handle_text_input(&mut self, text: &str)` to `FindFocusTarget` that delegates to its
`MiniBuffer` and sets `query_changed` flag.

**Location**: `crates/editor/src/find_target.rs`

**Implementation**:
```rust
// Chunk: docs/chunks/minibuffer_input - Text input support for find strip
/// Handles text input events (from IME, keyboard, paste).
///
/// Inserts text into the query field. Sets `query_changed` to true
/// if the content changed, allowing live search to trigger.
pub fn handle_text_input(&mut self, text: &str) {
    let prev_content = self.mini_buffer.content();
    self.mini_buffer.handle_text_input(text);
    let new_content = self.mini_buffer.content();
    self.query_changed = prev_content != new_content;
}
```

**Tests** (TDD - write first):
- `test_handle_text_input_updates_query`: `handle_text_input("search")` updates query
- `test_handle_text_input_sets_changed_flag`: `handle_text_input("a")` sets `query_changed` to true
- `test_handle_text_input_empty_string_no_change`: `handle_text_input("")` doesn't set changed flag

### Step 4: Add text input method to SelectorFocusTarget

Add `handle_text_input(&mut self, text: &str)` to `SelectorFocusTarget` that delegates to
its underlying `SelectorWidget`.

**Location**: `crates/editor/src/selector_target.rs`

**Implementation**:
```rust
// Chunk: docs/chunks/minibuffer_input - Text input delegation
/// Handles text input events (from IME, keyboard, paste).
///
/// Delegates to the underlying SelectorWidget for query editing.
pub fn handle_text_input(&mut self, text: &str) {
    self.widget.handle_text_input(text);
}
```

**Tests**:
- `test_handle_text_input_updates_widget_query`: verify delegation works correctly

### Step 5: Modify EditorState::handle_insert_text() to route by focus

Replace the early return with focus-aware routing. When focus is Selector or FindInFile,
route text to the appropriate minibuffer. When focus is Buffer, route to buffer/terminal.

**Location**: `crates/editor/src/editor_state.rs`

**Current code** (lines ~2955-2959):
```rust
pub fn handle_insert_text(&mut self, event: lite_edit_input::TextInputEvent) {
    // Only handle text input in Buffer focus mode
    if self.focus != EditorFocus::Buffer {
        return;
    }
    // ... buffer/terminal handling
}
```

**New implementation**:
```rust
// Chunk: docs/chunks/minibuffer_input - Focus-aware text input routing
pub fn handle_insert_text(&mut self, event: lite_edit_input::TextInputEvent) {
    let text = &event.text;
    if text.is_empty() {
        return;
    }

    match self.focus {
        EditorFocus::Selector => {
            // Route to selector's minibuffer via focus stack
            if let Some(target) = self.focus_stack.top_mut() {
                if target.layer() == FocusLayer::Selector {
                    // Downcast to SelectorFocusTarget and call handle_text_input
                    // Alternative: use Any + downcast, or add method to FocusTarget trait
                    // For now, directly route to active_selector
                }
            }
            // Fallback: route to active_selector if it exists
            if let Some(ref mut selector) = self.active_selector {
                selector.widget_mut().handle_text_input(text);
                // Trigger query re-evaluation (dirty marking)
                self.invalidation.merge(InvalidationKind::Layout);
            }
        }
        EditorFocus::FindInFile => {
            // Route to find strip's minibuffer
            if let Some(ref mut find_target) = self.find_target {
                find_target.handle_text_input(text);
                // Trigger live search if query changed
                if find_target.query_changed() {
                    self.run_live_find_search();
                    find_target.clear_query_changed();
                }
                self.invalidation.merge(InvalidationKind::Layout);
            }
        }
        EditorFocus::ConfirmDialog => {
            // ConfirmDialog doesn't accept text input - ignore
        }
        EditorFocus::Buffer => {
            // Existing buffer/terminal handling (unchanged)
            // ... (keep existing code for ws/tab lookup, terminal check, buffer insert)
        }
    }
}
```

**Note**: The exact implementation depends on whether `active_selector` is a `SelectorFocusTarget`
or `SelectorWidget`. Need to check the current state field types.

**Tests** (TDD - write first):
- `test_text_input_selector_focus_updates_query`: with focus=Selector, text input goes to selector
- `test_text_input_find_focus_updates_query`: with focus=FindInFile, text input goes to find strip
- `test_text_input_find_focus_triggers_live_search`: verify `run_live_find_search()` is called
- `test_text_input_buffer_focus_unchanged`: verify existing buffer behavior still works
- `test_text_input_terminal_still_works`: verify terminal text input still works

### Step 6: Update drain_loop to handle query change effects

After `handle_insert_text()`, ensure the selector's item list is re-filtered if the query
changed. This may already happen via existing patterns, but verify and add if needed.

**Location**: `crates/editor/src/drain_loop.rs` or `crates/editor/src/editor_state.rs`

**Check existing code**: The selector query filtering might already be triggered by
invalidation marks. If not, add `sync_selector_items()` call after text input to selector.

### Step 7: Write integration tests

Add integration tests that verify the full flow works:

**Location**: `crates/editor/tests/typing_test.rs` (or new file)

**Tests**:
- `test_typing_in_file_picker_filters_results`: Open file picker, type characters, verify
  query updates and list filters
- `test_typing_in_find_strip_searches`: Open find strip, type characters, verify search
  matches update
- `test_escape_still_dismisses_overlays`: Ensure modifier keys (Escape, Return, arrows)
  still work through KeyEvent path
- `test_ime_composition_in_selector`: Test IME marked text flow in selector (if applicable)

### Step 8: Update GOAL.md code_paths

Update the chunk's `code_paths` frontmatter with the files touched:

```yaml
code_paths:
  - crates/editor/src/mini_buffer.rs
  - crates/editor/src/selector.rs
  - crates/editor/src/selector_target.rs
  - crates/editor/src/find_target.rs
  - crates/editor/src/editor_state.rs
```

## Risks and Open Questions

1. **FocusStack vs direct field access**: The current implementation has both `focus_stack`
   and direct fields like `active_selector`, `find_target`. Need to verify which is the
   source of truth for routing. Based on code exploration, it appears the direct fields
   are still used for state, while `focus_stack` is for event dispatch.

2. **IME marked text handling**: The goal mentions IME composition should work. Currently
   `handle_set_marked_text()` also has an early return for non-Buffer focus. This chunk
   might need to extend to handle IME in minibuffers, or that could be a follow-up chunk.
   For MVP, focus on regular text input (`insertText:`).

3. **Paste handling**: Paste also goes through `TextInputEvent`. Verify that paste into
   selector/find works after this fix. If paste has a separate code path, may need similar
   routing.

4. **Terminal tab text input**: The existing terminal handling in `handle_insert_text()`
   must continue to work. The new routing should only affect cases where focus is
   Selector/FindInFile, not terminal tabs with focus=Buffer.
