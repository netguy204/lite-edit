

<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk replaces six non-wrap-aware `ensure_visible()` call sites in
`editor_state.rs` with the wrap-aware `ensure_visible_wrapped()` (or
`ensure_visible_wrapped_with_margin()` with `margin=0`). Both methods already
exist in `viewport.rs` from the `find_scroll_wrap_awareness` chunk. **No
changes to `viewport.rs` are needed.**

The six call sites group into two borrow patterns:

| Pattern | Lines | Context |
|---------|-------|---------|
| **A — buffer + viewport both in scope** | 2664, 3659, 3794, 3841 | `if let Some((buffer, viewport)) = tab.buffer_and_viewport_mut()` |
| **B — buffer borrow ended** | 1504, 1617 | `tab.viewport.ensure_visible(…)` called after separate buffer borrow |

For **Pattern A**: pass `|i| buffer.line_len(i)` directly as the closure.
`buffer` and `viewport` are disjoint reborrow targets of different `tab` fields,
so Rust allows immutable use of `buffer` alongside `&mut viewport`. Build
`WrapLayout` inline: `use crate::wrap_layout::WrapLayout;
WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics)` — both
`self.view_width` and `self.font_metrics` are accessible because they are
distinct fields of `EditorState` from `self.editor`.

For **Pattern B**: pre-collect `line_lens: Vec<usize>` inside a short buffer
borrow scope, then call `tab.viewport.ensure_visible_wrapped(…)` with a
`|i| line_lens.get(i).copied().unwrap_or(0)` closure. This mirrors the
pattern already established in `ensure_cursor_visible_in_active_tab()` (line
~4387).

`RAIL_WIDTH` is already imported at the top of `editor_state.rs`
(`use crate::left_rail::{…, RAIL_WIDTH};` line 56). `WrapLayout` is used inline
at other call sites in the same file (lines 2029, 3095, 3223) — follow that
convention (`use crate::wrap_layout::WrapLayout;` at local scope).

Following the project's TDD discipline, failing tests are written before each
fix.

## Subsystem Considerations

**`docs/subsystems/viewport_scroll`** (DOCUMENTED): This chunk IMPLEMENTS
fixes that bring call sites into compliance with the subsystem's Soft
Convention 1: "Prefer `set_scroll_offset_px_wrapped` over `set_scroll_offset_px`
when wrapping is enabled." The same principle applies to `ensure_visible`:
callers should use `ensure_visible_wrapped` when wrapping is enabled.

**Existing deviation noted**: `ensure_cursor_visible_in_active_tab()` (line
~4388) builds its `WrapLayout` with `self.view_width` rather than
`self.view_width - RAIL_WIDTH`. This underestimates `cols_per_row` by the rail
width (56 px). This pre-existing deviation is outside this chunk's scope; it
will be added to the subsystem's Known Deviations. The new call sites in this
chunk will use the correct `self.view_width - RAIL_WIDTH`.

## Sequence

---

### Step 1: Write a failing test for the cursor snap-back (line 2664)

The cursor snap-back fires in `handle_key_event` (file-tab path, line 2661) when
`viewport.buffer_line_to_screen_line(cursor_line).is_none()`. This check is
**line-based** (it uses `RowScroller::row_to_visible_offset`, which treats
`first_visible_row` as a buffer-line index). When wrap-aware scrolling has
moved `scroll_offset_px` to `N * line_height` where `N` is an abs screen row,
`first_visible_row()` returns `N`. If the cursor's buffer line index is less
than `N`, the check incorrectly concludes the cursor is off-screen and the
snap-back fires — then `ensure_visible(cursor_line, line_count)` scrolls to the
wrong (line-based) position.

**Failing test scenario** (choose a key with no cursor movement — `Cmd+C` —
so there is no post-key `ensure_visible_wrapped` to mask the wrong snap-back):

```
// Setup:
// - 10 lines (0-9): "aaaaaaaaaaaaaa" (14 chars → 2 screen rows each at 10 cols/row)
// - Line 10: "x" (1 screen row)
// - Viewport: 5 visible rows (80px), narrow width (RAIL_WIDTH + 80px → 10 cols/row)
// - Cursor at line 10, col 0
//
// After wrap-aware scrolling, viewport is at scroll_offset = 256px
//   (new_top_row = 20 - 4 = 16; 16 * 16 = 256px)
// first_visible_row() = 256/16 = 16 (screen row 16, not buffer line 16)
//
// buffer_line_to_screen_line(10): row_to_visible_offset(10) → 10 < 16 → None
//   → snap-back fires
//
// ensure_visible(10, 11) [WRONG]: first_visible=16 > target=10 → scrolls up
//   → scroll_offset = 10 * 16 = 160px  ← BUG
//
// ensure_visible_wrapped(10, 0, …) [CORRECT]: abs_row=20, current_top=16,
//   effective_visible=5. 20 > 16+5=21? No → no scroll needed (cursor visible)
//   → scroll_offset stays at 256px ← CORRECT
//
// ASSERT: after Cmd+C, scroll_offset_px == 256px (not 160px)
```

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module, at the
end of the wrap-awareness test section (after the `test_find_scroll_wrap_awareness`
test, ~line 8919). Name: `test_arrow_scroll_snap_back_wrap_awareness`.

Use `state.viewport_mut().set_scroll_offset_px_unclamped(256.0)` to simulate
the state after wrap-aware navigation. Build the key event as:

```rust
let cmd_c = KeyEvent::new(
    Key::Char('c'),
    Modifiers { command: true, ..Default::default() },
);
state.handle_key(cmd_c);
assert_eq!(
    state.viewport().scroll_offset_px(),
    256.0,
    "snap-back must not clobber wrap-correct scroll offset"
);
```

The test will fail before Step 2 because `ensure_visible` scrolls to 160px.

---

### Step 2: Fix the cursor snap-back at line 2664

Replace the `ensure_visible` call inside the snap-back guard:

```rust
// BEFORE (line ~2664):
if viewport.ensure_visible(cursor_line, line_count) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

```rust
// AFTER:
// Chunk: docs/chunks/arrow_scroll_wrap_awareness - Wrap-aware snap-back
use crate::wrap_layout::WrapLayout;
let cursor_col = buffer.cursor_position().col;
let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);
if viewport.ensure_visible_wrapped(
    cursor_line,
    cursor_col,
    line_count,
    &wrap_layout,
    |i| buffer.line_len(i),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Note: `cursor_line` is already bound from the line above
(`let cursor_line = buffer.cursor_position().line;`). `cursor_col` can be added
as a second field access. The `|i| buffer.line_len(i)` closure borrows `buffer`
immutably, which is disjoint from the `&mut viewport` borrow — both are split
borrows of different `tab` fields. `self.view_width` and `self.font_metrics` are
accessible here (same pattern as line 2675 where `font_metrics = self.font_metrics`
is used inside the same outer block).

Run tests. The Step 1 test should now pass.

---

### Step 3: Write failing tests for file-drop and text-insertion call sites (lines 3659, 3794)

Both sites have the same pattern: inside a `buffer_and_viewport_mut()` block,
after inserting text, `ensure_visible(cursor_line, buffer.line_count())` is
called for the new cursor position. With wrapped content, the line-based scroll
under-counts and scrolls to the wrong position.

Use the same 10-long-lines scenario as Step 1:

```
test_file_drop_insertion_wrap_awareness
  // Setup: 10 long lines → cursor ends up at line 10 after insertion
  // Scroll to 256px via set_scroll_offset_px_unclamped
  // Call handle_file_dropped with a small text payload that inserts at line 10
  // ASSERT: scroll_offset_px == 256.0 (cursor was visible, must not over-scroll)
  // Before fix: ensure_visible(10, 11) → 160px (FAIL)

test_text_insertion_wrap_awareness
  // Same setup but trigger via handle_insert_text (type a character)
  // ASSERT: scroll_offset_px == 256.0
  // Before fix: ensure_visible(10, 11) → 160px (FAIL)
```

Both tests call `state.viewport_mut().set_scroll_offset_px_unclamped(256.0)`
after positioning the cursor at line 10 (append content to reach line 10).

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module, after
the Step 1 test.

---

### Step 4: Fix file-drop insertion at line 3659

In `handle_file_dropped`, inside the `if let Some((buffer, viewport))` block:

```rust
// BEFORE (~line 3659):
let cursor_line = buffer.cursor_position().line;
if viewport.ensure_visible(cursor_line, buffer.line_count()) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

```rust
// AFTER:
// Chunk: docs/chunks/arrow_scroll_wrap_awareness - Wrap-aware scroll after file drop
use crate::wrap_layout::WrapLayout;
let cursor_pos = buffer.cursor_position();
let line_count = buffer.line_count();
let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);
if viewport.ensure_visible_wrapped(
    cursor_pos.line,
    cursor_pos.col,
    line_count,
    &wrap_layout,
    |i| buffer.line_len(i),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

---

### Step 5: Fix text-insertion call site at line 3794

In `handle_insert_text`, inside the file-tab `buffer_and_viewport_mut()` block:

```rust
// BEFORE (~line 3793):
let cursor_line = buffer.cursor_position().line;
if viewport.ensure_visible(cursor_line, buffer.line_count()) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

```rust
// AFTER:
// Chunk: docs/chunks/arrow_scroll_wrap_awareness - Wrap-aware scroll after text insertion
use crate::wrap_layout::WrapLayout;
let cursor_pos = buffer.cursor_position();
let line_count = buffer.line_count();
let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);
if viewport.ensure_visible_wrapped(
    cursor_pos.line,
    cursor_pos.col,
    line_count,
    &wrap_layout,
    |i| buffer.line_len(i),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Run tests. The Step 3 tests should now pass.

---

### Step 6: Write a failing test for IME marked text (line 3841)

```
test_ime_marked_text_wrap_awareness
  // Setup: 10 long lines → cursor at line 10
  // Scroll to 256px
  // Call state.handle_set_marked_text(MarkedTextEvent { text: "x".to_string(), selected_range: 0..1 })
  // ASSERT: scroll_offset_px == 256.0
  // Before fix: ensure_visible(10, 11) → 160px (FAIL)
```

Location: `crates/editor/src/editor_state.rs`, `#[cfg(test)]` module.

---

### Step 7: Fix IME marked text at line 3841

In `handle_set_marked_text`, inside the `buffer_and_viewport_mut()` block:

```rust
// BEFORE (~line 3840):
let cursor_line = buffer.cursor_position().line;
if viewport.ensure_visible(cursor_line, buffer.line_count()) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

```rust
// AFTER:
// Chunk: docs/chunks/arrow_scroll_wrap_awareness - Wrap-aware scroll after IME marked text
use crate::wrap_layout::WrapLayout;
let cursor_pos = buffer.cursor_position();
let line_count = buffer.line_count();
let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);
if viewport.ensure_visible_wrapped(
    cursor_pos.line,
    cursor_pos.col,
    line_count,
    &wrap_layout,
    |i| buffer.line_len(i),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Run tests. The Step 6 test should now pass.

---

### Step 8: Write failing tests for gotodef call sites (lines 1504, 1617)

For **line 1617** (cross-file gotodef): use the existing `goto_cross_file_definition`
test harness. Build a target file with 10 long lines (14 chars each) plus the
definition target at line 10, and a narrow viewport. Call
`state.goto_cross_file_definition(…)` and assert the resulting
`scroll_offset_px` is correct for wrap-aware scrolling (not the line-based
under-scroll). Since line 1617 fires before `ensure_cursor_visible_in_active_tab`
at line 1624, before the fix the final scroll position is determined by
`ensure_cursor_visible_in_active_tab` (which already uses `ensure_visible_wrapped`
but with `self.view_width` instead of `self.view_width - RAIL_WIDTH`). To isolate
line 1617's effect, assert the scroll position using a narrow enough viewport
that the wrong line 1617 scroll would be detectably different.

In practice: with the same 10-long-lines setup and a narrow viewport
(`view_width = RAIL_WIDTH + 80.0`), line 1617's `ensure_visible(10, 11)` fires
first with the wrong position, and then `ensure_cursor_visible_in_active_tab`
corrects it. Verifying that line 1617 alone produces the right position requires
either:
- Calling `goto_cross_file_definition` and checking that `tab.viewport`
  scrolls correctly (the correction by `ensure_cursor_visible_in_active_tab`
  makes the final result the same for most cases, so the line 1617 test is
  more of a correctness / no-regression check), **OR**
- Testing via a scenario where the line 1617 scroll is incorrect but
  `ensure_cursor_visible_in_active_tab` doesn't fully correct it (due to the
  `view_width` vs `view_width - RAIL_WIDTH` difference).

Given this complexity, **the primary test coverage for 1617 is that the scroll
matches the wrap-aware result**. Write the test similarly to
`test_goto_cross_file_definition_opens_new_tab` but with wrapped content and
a scroll assertion.

For **line 1504** (same-file gotodef): this fires through `handle_goto_definition`
which requires a tree-sitter syntax tree and is harder to unit-test directly.
The fix is straightforward (same pattern as 1617), and the success criterion
"Go-to-definition scrolls to the correct position with wrapped lines" is
partially verified by the 1617 test and by visual inspection. Add a
`// TODO: integration test` comment noting that a same-file gotodef wrap-scroll
test can be added once a simpler test harness for tree-sitter is available.

---

### Step 9: Fix gotodef same-file at line 1504

In `handle_goto_definition` (same-file path), after the buffer cursor is set:

```rust
// BEFORE (~line 1504):
// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor
if tab.viewport.ensure_visible(def_line, line_count) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

```rust
// AFTER:
// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor
// Chunk: docs/chunks/arrow_scroll_wrap_awareness - Wrap-aware gotodef scroll (same file)
use crate::wrap_layout::WrapLayout;
let line_lens: Vec<usize> = tab
    .as_text_buffer()
    .map(|b| (0..line_count).map(|i| b.line_len(i)).collect())
    .unwrap_or_default();
let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);
if tab.viewport.ensure_visible_wrapped(
    def_line,
    def_col,
    line_count,
    &wrap_layout,
    |i| line_lens.get(i).copied().unwrap_or(0),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Note: `def_col` is already bound (it comes from
`byte_offset_to_position(&source, def_range.start)`). `line_count` is bound at
line 1498. The `line_lens` collection is a read-only borrow of `tab.as_text_buffer()`
which ends before the `tab.viewport` mutable borrow begins.

---

### Step 10: Fix gotodef cross-file at line 1617

In the cross-file navigation path, after the buffer cursor is set:

```rust
// BEFORE (~line 1617):
// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor
if tab.viewport.ensure_visible(target_line, line_count) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

```rust
// AFTER:
// Chunk: docs/chunks/gotodef_scroll_reveal - Scroll viewport to reveal cursor
// Chunk: docs/chunks/arrow_scroll_wrap_awareness - Wrap-aware gotodef scroll (cross-file)
use crate::wrap_layout::WrapLayout;
let line_lens: Vec<usize> = tab
    .as_text_buffer()
    .map(|b| (0..line_count).map(|i| b.line_len(i)).collect())
    .unwrap_or_default();
let wrap_layout = WrapLayout::new(self.view_width - RAIL_WIDTH, &self.font_metrics);
if tab.viewport.ensure_visible_wrapped(
    target_line,
    target_col,
    line_count,
    &wrap_layout,
    |i| line_lens.get(i).copied().unwrap_or(0),
) {
    self.invalidation.merge(InvalidationKind::Layout);
}
```

Note: `target_col` is already bound before this block (it is the column
coordinate of the definition target used to set the cursor position at line
1614). `line_count` is bound at line 1612.

Run tests. The Step 8 tests should now pass.

---

### Step 11: Update viewport_scroll subsystem Known Deviations

Add the pre-existing `ensure_cursor_visible_in_active_tab` deviation to
`docs/subsystems/viewport_scroll/OVERVIEW.md` Known Deviations:

```
- `editor_state.rs#EditorState::ensure_cursor_visible_in_active_tab` builds
  `WrapLayout` with `self.view_width` instead of `self.view_width - RAIL_WIDTH`,
  slightly overestimating `cols_per_row` for navigation scenarios. Discovered
  during `arrow_scroll_wrap_awareness`; not addressed here (minor visual impact,
  separate fix).
```

---

### Step 12: Update `code_paths` in GOAL.md

Update the `code_paths` field in
`docs/chunks/arrow_scroll_wrap_awareness/GOAL.md`:

```yaml
code_paths:
  - crates/editor/src/editor_state.rs
```

(Only `editor_state.rs` is modified; `viewport.rs` is unchanged.)

---

## Dependencies

- `find_scroll_wrap_awareness` must be complete (ACTIVE) before this chunk is
  implemented. It provides `ensure_visible_wrapped` and
  `ensure_visible_wrapped_with_margin` in `viewport.rs`. The GOAL.md already
  declares this via `depends_on: [find_scroll_wrap_awareness]`.

## Risks and Open Questions

- **`buffer_line_to_screen_line` guard at line 2661 is also line-based**: The
  snap-back trigger check (`viewport.buffer_line_to_screen_line(cursor_line).is_none()`)
  uses line-based logic that confuses screen rows with buffer lines in wrapped
  mode. This can cause the snap-back to fire when the cursor IS actually visible
  (or fail to fire when it isn't). Fixing the *guard* is a separate concern
  beyond this chunk's scope; this chunk only fixes what happens *when* the
  snap-back fires. The guard produces false positives (fires unnecessarily),
  which after this fix results in a no-op scroll rather than a wrong scroll —
  an acceptable interim state.

- **`ensure_cursor_visible_in_active_tab` partially overlaps with line 1617**:
  The cross-file gotodef path calls both line 1617 and
  `ensure_cursor_visible_in_active_tab()`. After this fix, both use wrap-aware
  scrolling. Two sequential wrap-aware calls to the same viewport with the same
  cursor position are idempotent (the second is always a no-op). No correctness
  issue, but the redundancy could be cleaned up as a future simplification.

- **Tab-character column approximation**: `ensure_visible_wrapped` treats
  `cursor_col` as a visual column. For lines with tab characters, this may
  differ from the character index. Pre-existing limitation inherited from the
  existing `ensure_visible_wrapped` implementation; out of scope here.

- **Multi-pane width**: All new call sites use `self.view_width - RAIL_WIDTH`
  as the WrapLayout width. In multi-pane layouts, panes may be narrower. This
  is the same limitation documented in the `find_scroll_wrap_awareness` PLAN.md.

## Deviations

<!--
POPULATE DURING IMPLEMENTATION, not at planning time.
-->
