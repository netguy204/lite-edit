# Implementation Plan

## Approach

Wire double-click word selection through the existing mouse handling pipeline, building
on the `word_boundary_left` and `word_boundary_right` helpers from `word_boundary_primitives`
and following the patterns established in `word_jump_navigation`.

The implementation has two layers:

1. **Input layer**: Extend `MouseEvent` in `crates/editor/src/input.rs` to carry a
   `click_count: u32` field, and populate it from `NSEvent.clickCount` in `metal_view.rs`.

2. **Buffer target layer**: In `buffer_target.rs`'s `handle_mouse`, detect `click_count == 2`
   on a `Down` event. When detected, compute word boundaries using the helpers from
   `word_boundary_primitives`, then set selection anchor at word start and cursor at word
   end via `set_selection_anchor` + `move_cursor_preserving_selection`.

Per the Humble View Architecture (TESTING_PHILOSOPHY.md), the logic is fully testable
without platform dependencies:
- `MouseEvent` construction with `click_count` is pure data
- `handle_mouse` takes an `EditorContext` reference — testable in isolation
- The word boundary helpers are already unit-tested

Following TDD, write failing tests for double-click behavior before implementing the
`handle_mouse` logic. The NSEvent/Metal integration layer is a thin shell and verified
visually per the testing philosophy.

## Sequence

### Step 1: Extend `MouseEvent` to include `click_count`

Add a `click_count: u32` field to the `MouseEvent` struct in `crates/editor/src/input.rs`:

```rust
/// A mouse event.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseEvent {
    /// The type of mouse event
    pub kind: MouseEventKind,
    /// Position in view coordinates (pixels from top-left)
    pub position: (f64, f64),
    /// Modifier keys held during the event
    pub modifiers: Modifiers,
    // Chunk: docs/chunks/word_double_click_select - Double-click word selection
    /// Number of consecutive clicks (1 for single, 2 for double, etc.)
    pub click_count: u32,
}
```

Update all existing `MouseEvent` construction sites in tests to include `click_count: 1`
to maintain backward compatibility.

Location: `crates/editor/src/input.rs`

### Step 2: Populate `click_count` from `NSEvent.clickCount` in `metal_view.rs`

Modify `convert_mouse_event` to extract the click count from the NSEvent:

```rust
fn convert_mouse_event(&self, event: &NSEvent, kind: MouseEventKind) -> Option<MouseEvent> {
    // ... existing position calculation ...

    // Chunk: docs/chunks/word_double_click_select - Double-click word selection
    // Extract click count for double-click detection
    let click_count = event.clickCount() as u32;

    Some(MouseEvent {
        kind,
        position,
        modifiers,
        click_count,
    })
}
```

Note: `NSEvent.clickCount` returns an `NSInteger` (i64 on 64-bit). Casting to `u32` is
safe since click counts are always small positive integers.

Location: `crates/editor/src/metal_view.rs`

### Step 3: Write failing tests for double-click word selection

Create tests in `crates/editor/src/buffer_target.rs` covering the success criteria:

- Double-click mid-word selects entire word (anchor at word start, cursor at word end)
- Double-click at word start selects word
- Double-click on whitespace between words selects whitespace run
- Double-click on empty line is a no-op (no panic, no selection)
- Double-click at/past end of line selects last run on that line (or no-op if empty)
- Single clicks (`click_count == 1`) continue to behave exactly as before

The test pattern follows existing mouse tests:

```rust
#[test]
fn test_double_click_mid_word_selects_word() {
    // "hello world" with double-click on 'o' in "hello" (col 4)
    let mut buffer = TextBuffer::from_str("hello world");
    // ... create context and target ...
    let event = MouseEvent {
        kind: MouseEventKind::Down,
        position: (32.0, 155.0), // x for col 4
        modifiers: Modifiers::default(),
        click_count: 2,
    };
    target.handle_mouse(event, &mut ctx);

    // Should select "hello" (cols 0-5)
    assert!(buffer.has_selection());
    let (start, end) = buffer.selection_range().unwrap();
    assert_eq!(start, Position::new(0, 0));
    assert_eq!(end, Position::new(0, 5));
}
```

Location: `crates/editor/src/buffer_target.rs` in `#[cfg(test)] mod tests`

### Step 4: Implement double-click handling in `handle_mouse`

Modify the `MouseEventKind::Down` arm in `handle_mouse` to check `click_count`:

```rust
// Chunk: docs/chunks/word_double_click_select - Double-click word selection
MouseEventKind::Down => {
    // Convert pixel position to buffer position
    let position = pixel_to_buffer_position(
        event.position,
        ctx.view_height,
        &ctx.font_metrics,
        ctx.viewport.first_visible_line(),
        ctx.buffer.line_count(),
        |line| ctx.buffer.line_len(line),
    );

    if event.click_count == 2 {
        // Spec: docs/trunk/SPEC.md#word-model
        // Double-click: select word or whitespace run at click position
        let line_content = ctx.buffer.line_content(position.line);
        let line_chars: Vec<char> = line_content.chars().collect();

        // Handle empty line or click past line end
        if line_chars.is_empty() || position.col >= line_chars.len() {
            // No-op for empty line; for past-end, select last run if any
            if !line_chars.is_empty() {
                let last_col = line_chars.len().saturating_sub(1);
                let word_start = word_boundary_left(&line_chars, line_chars.len());
                let word_end = line_chars.len();
                ctx.buffer.set_selection_anchor(Position::new(position.line, word_start));
                ctx.buffer.move_cursor_preserving_selection(Position::new(position.line, word_end));
                ctx.mark_cursor_dirty();
            }
            return;
        }

        // Find word boundaries: start is left boundary, end is right boundary
        // word_boundary_left(chars, col+1) finds start of run containing chars[col]
        // word_boundary_right(chars, col) finds end of run containing chars[col]
        let word_start = word_boundary_left(&line_chars, position.col + 1);
        let word_end = word_boundary_right(&line_chars, position.col);

        ctx.buffer.set_selection_anchor(Position::new(position.line, word_start));
        ctx.buffer.move_cursor_preserving_selection(Position::new(position.line, word_end));
        ctx.mark_cursor_dirty();
    } else {
        // Single click: position cursor and set anchor for potential drag
        ctx.buffer.set_cursor(position);
        ctx.buffer.set_selection_anchor_at_cursor();
        ctx.mark_cursor_dirty();
    }
}
```

This requires importing `word_boundary_left` and `word_boundary_right` from the buffer
crate. Since these are currently private functions in `text_buffer.rs`, they need to be
made accessible. The approach follows what `word_jump_navigation` established: either:
- Make them `pub(crate)` and call via `TextBuffer` re-exports, OR
- Expose thin wrapper methods on `TextBuffer` itself

Per the GOAL.md dependency note: "the visibility decision made [in word_jump_navigation]
should be reused here." Examining `word_jump_navigation`'s implementation, the helpers
remain private and are called within `TextBuffer` methods (`move_word_left`/`move_word_right`).

For `buffer_target.rs` to use them, we have two options:
1. **Add `select_word_at` method to `TextBuffer`** — encapsulates the word-selection
   logic in the buffer layer, keeping helpers private
2. **Make helpers `pub(crate)` and re-export** — exposes primitives for external use

Option 1 is cleaner (maintains encapsulation) and more consistent with the existing
pattern where buffer methods handle the word model complexity.

**Revised approach**: Add `select_word_at(col: usize)` to `TextBuffer`:

```rust
// Chunk: docs/chunks/word_double_click_select - Double-click word selection
// Spec: docs/trunk/SPEC.md#word-model
/// Selects the word or whitespace run at the given column on the current line.
///
/// Sets the selection anchor at the word start and the cursor at the word end.
/// Returns `true` if a selection was made, `false` if the line is empty or col
/// is out of bounds.
pub fn select_word_at(&mut self, col: usize) -> bool {
    let line_content = self.line_content(self.cursor.line);
    let line_chars: Vec<char> = line_content.chars().collect();

    if line_chars.is_empty() {
        return false;
    }

    // Clamp col to valid range
    let col = col.min(line_chars.len().saturating_sub(1));

    let word_start = word_boundary_left(&line_chars, col + 1);
    let word_end = word_boundary_right(&line_chars, col);

    self.selection_anchor = Some(Position::new(self.cursor.line, word_start));
    self.cursor.col = word_end;
    true
}
```

Then `handle_mouse` simply calls:
```rust
if event.click_count == 2 {
    ctx.buffer.set_cursor(position); // Sets cursor.line
    if ctx.buffer.select_word_at(position.col) {
        ctx.mark_cursor_dirty();
    }
}
```

Location: `crates/buffer/src/text_buffer.rs` (new method) and
`crates/editor/src/buffer_target.rs` (call site)

### Step 5: Write failing tests for `select_word_at` in `text_buffer.rs`

Before implementing the buffer method, add tests:

- Select word mid-word → anchor at word start, cursor at word end
- Select word at word start → same behavior
- Select whitespace run between words → selects whitespace
- Select on empty line → returns false, no selection
- Select at col 0 → selects first run
- Select at end of line → selects last run

Location: `crates/buffer/src/text_buffer.rs` in `#[cfg(test)] mod tests`

### Step 6: Implement `select_word_at` in `TextBuffer`

Add the method as designed in Step 4.

Location: `crates/buffer/src/text_buffer.rs`

### Step 7: Update `handle_mouse` to use `select_word_at`

Implement the double-click logic in `handle_mouse` as described in Step 4,
calling the new `select_word_at` method.

Location: `crates/editor/src/buffer_target.rs`

### Step 8: Update existing test MouseEvent constructors

All existing tests that create `MouseEvent` must be updated to include `click_count: 1`.
Grep for `MouseEvent {` in `buffer_target.rs` and update each occurrence.

Location: `crates/editor/src/buffer_target.rs`

### Step 9: Run full test suite

Run `cargo test` in both `crates/buffer` and `crates/editor` directories.
All new tests should pass, and no existing tests should regress.

### Step 10: Update GOAL.md code_paths

Add the touched files to the chunk's GOAL.md frontmatter:

```yaml
code_paths:
  - crates/editor/src/input.rs
  - crates/editor/src/metal_view.rs
  - crates/editor/src/buffer_target.rs
  - crates/buffer/src/text_buffer.rs
```

Location: `docs/chunks/word_double_click_select/GOAL.md`

## Dependencies

- **word_boundary_primitives**: This chunk depends on the `word_boundary_left` and
  `word_boundary_right` helper functions. These are private functions in `text_buffer.rs`
  but will be accessed via a new `select_word_at` method on `TextBuffer`.

- **word_jump_navigation**: This chunk depends on the visibility pattern established
  for the word boundary helpers. Since `word_jump_navigation` kept them private and
  exposed behavior through `TextBuffer` methods, this chunk follows the same pattern.

## Risks and Open Questions

- **SPEC.md word-model section**: The GOAL.md references `docs/trunk/SPEC.md#word-model`
  but that section doesn't exist yet (SPEC.md is still a template). The implementation
  will carry the comment pointing to where the spec *should* define the word model,
  consistent with the approach in `word_boundary_primitives` and `word_jump_navigation`.

- **NSEvent.clickCount casting**: `NSEvent.clickCount` returns `NSInteger` (i64 on 64-bit).
  Casting to `u32` is safe for realistic click counts but theoretically could truncate
  if macOS ever returns a value > 4 billion (not a concern in practice).

- **Past-line-end behavior**: The GOAL.md specifies "double-clicking at or past the end
  of a line selects the last run on that line." The implementation handles this by
  clamping `col` to `line_chars.len() - 1` before calling the boundary helpers. This
  matches intuitive behavior: clicking in the gutter area after text selects the last
  word.

- **Triple-click and beyond**: This chunk only implements double-click. macOS editors
  typically use triple-click for line selection. Future work could extend the
  `click_count` handling, but that's out of scope for this chunk.

## Deviations

<!-- POPULATE DURING IMPLEMENTATION -->