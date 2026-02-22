# Implementation Plan

## Approach

`MiniBuffer` will be a thin composition wrapper around existing primitives:
- **`TextBuffer`** (from `lite_edit_buffer`): Provides all text editing operations
- **`Viewport`**: Tracks viewport state (needed by `BufferFocusTarget`)
- **`BufferFocusTarget`**: Handles key event → command resolution and execution

The design follows the project's Humble View Architecture (per TESTING_PHILOSOPHY.md):
MiniBuffer is pure state + update logic with no platform dependencies, making it
fully testable without windows or GPU.

**Key insight**: Rather than reimplementing any editing logic, MiniBuffer:
1. Owns a `TextBuffer` and `Viewport`
2. Delegates all key handling to a `BufferFocusTarget` via `EditorContext`
3. Filters only the events that would violate the single-line invariant:
   - `Key::Return` → no-op (would insert newline)
   - `Key::Up` / `Key::Down` → no-op (no multi-line cursor movement)
   - All other keys pass through unchanged

This ensures MiniBuffer gets all affordances (word-jump, kill-line, selection,
clipboard) for free, with minimal code to maintain.

**TDD approach**: Per TESTING_PHILOSOPHY.md, we write failing tests first for
behavioral code. Step 1 creates the struct scaffolding (no behavior to test),
then Step 2 writes failing tests for each success criterion before implementing.

## Sequence

### Step 1: Create the module scaffolding

Create `crates/editor/src/mini_buffer.rs` with:
- Module-level chunk backreference comment
- Import statements for required types
- Empty `MiniBuffer` struct definition with private fields:
  - `buffer: TextBuffer`
  - `viewport: Viewport`
  - `dirty_region: DirtyRegion`
  - `font_metrics: FontMetrics`

Add `mod mini_buffer;` to `crates/editor/src/main.rs` module list.

Location: `crates/editor/src/mini_buffer.rs`, `crates/editor/src/main.rs`

### Step 2: Write failing tests for all success criteria

Following TDD, write the test suite first. Tests covering:
- `new()` creates empty buffer with no selection
- Typing characters builds `content()`
- Backspace removes last character; on empty is no-op
- Alt+Backspace (option: true) deletes word backward
- Ctrl+K kills to end of line
- Option+Left / Option+Right move cursor by word
- Shift+Right extends selection; `selection_range()` returns correct span
- Return is no-op (no newline inserted)
- Up and Down are no-ops
- Cmd+A selects all; `selection_range()` covers full content
- `clear()` empties content and removes selection
- `cursor_col()` returns correct position
- `has_selection()` reflects selection state

Tests should fail initially since methods are not yet implemented.

Location: `crates/editor/src/mini_buffer.rs` (`#[cfg(test)]` module)

### Step 3: Implement `MiniBuffer::new(font_metrics: FontMetrics)`

Implement the constructor:
- Create empty `TextBuffer::new()`
- Create `Viewport::new(font_metrics.line_height as f32)`
- Initialize `DirtyRegion::None`
- Store font metrics

Location: `crates/editor/src/mini_buffer.rs`

### Step 4: Implement accessor methods

Implement the read-only accessors:
- `content(&self) -> String` — returns `self.buffer.content()`
- `cursor_col(&self) -> usize` — returns `self.buffer.cursor_position().col`
- `selection_range(&self) -> Option<(usize, usize)>` — extracts column range from buffer's selection
- `has_selection(&self) -> bool` — delegates to `self.buffer.has_selection()`

Location: `crates/editor/src/mini_buffer.rs`

### Step 5: Implement `MiniBuffer::handle_key(&mut self, event: KeyEvent)`

The core method that enforces the single-line invariant:

```rust
pub fn handle_key(&mut self, event: KeyEvent) {
    // Filter events that would break single-line invariant
    match &event.key {
        Key::Return => return,  // No newlines
        Key::Up | Key::Down => return,  // No vertical movement
        _ => {}
    }

    // Create EditorContext and delegate to BufferFocusTarget
    let mut target = BufferFocusTarget::new();
    let mut ctx = EditorContext::new(
        &mut self.buffer,
        &mut self.viewport,
        &mut self.dirty_region,
        self.font_metrics,
        self.font_metrics.line_height as f32,  // view_height (single line)
        f32::MAX,  // view_width (no wrapping needed)
    );
    target.handle_key(event, &mut ctx);
}
```

Location: `crates/editor/src/mini_buffer.rs`

### Step 6: Implement `MiniBuffer::clear(&mut self)`

Reset the buffer to empty state:
- Replace `self.buffer` with `TextBuffer::new()`
- Clear any dirty region

Location: `crates/editor/src/mini_buffer.rs`

### Step 7: Run tests and verify all pass

Execute the test suite:
```bash
cargo test -p lite-edit --lib mini_buffer
```

All tests from Step 2 should now pass. If any fail, debug and fix.

Location: Terminal

### Step 8: Add doc comments and finalize

Add rustdoc comments to:
- Module-level documentation explaining MiniBuffer's purpose
- Struct-level documentation
- All public methods

Ensure the code compiles with `cargo build` and passes `cargo clippy`.

Location: `crates/editor/src/mini_buffer.rs`

## Dependencies

- **text_buffer chunk**: Provides `TextBuffer` with full editing API — **ACTIVE**
- **buffer_view_trait chunk**: Provides `BufferView` trait (not directly used but informs API) — **ACTIVE**
- **Existing crate dependencies**: `lite_edit_buffer` crate is already a dependency

No new external libraries needed.

## Risks and Open Questions

1. **Cmd+Up / Cmd+Down filtering**: The narrative mentions these should also be
   no-ops. Need to verify if `BufferFocusTarget` handles these differently from
   plain Up/Down (it does: Cmd+Up → MoveToBufferStart, Cmd+Down → MoveToBufferEnd).
   These are valid single-line operations (move to start/end of the one line),
   so they should NOT be filtered. Only vertical movement commands (Up/Down without
   Cmd) should be filtered.

2. **content() return type**: GOAL.md says `-> &str` or `-> String`. TextBuffer's
   `content()` returns `String`. Returning `&str` would require storing the content,
   which adds complexity. Will use `-> String` for simplicity.

3. **EditorContext view dimensions**: MiniBuffer is single-line, so `view_height`
   should be `line_height` (one line visible). `view_width` is set to `f32::MAX`
   to disable line wrapping in the minibuffer context.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->