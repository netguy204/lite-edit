<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This is a pure structural refactor using the "Extract and Delegate" pattern:

1. **Create the new `RowScroller` struct** in a new file `row_scroller.rs` containing
   the three core fields (`scroll_offset_px`, `visible_rows`, `row_height`) and all
   thirteen uniform-row scroll methods.

2. **Refactor `Viewport`** to contain a `RowScroller` field and delegate the shared
   methods to it. The two buffer-specific methods (`dirty_lines_to_region`,
   `ensure_visible_wrapped`) remain as `Viewport`-only additions.

3. **Expose `RowScroller`** from the editor crate's public surface via `pub use` in
   `main.rs`, and add a `row_scroller() -> &RowScroller` accessor to `Viewport`.

4. **Test-driven development** per TESTING_PHILOSOPHY.md: write unit tests for
   `RowScroller` first (the red phase), then implement the methods (green phase).
   Existing `Viewport` tests must pass unchanged after the refactor.

The approach follows the "Humble View Architecture" principle — `RowScroller` is
pure state manipulation with no platform dependencies, making it fully testable
without mocking.

**Key insight**: The thirteen methods in `Viewport` that move to `RowScroller` use
the terms "line"/"lines" (e.g., `first_visible_line`, `visible_lines`). Since
`RowScroller` is domain-agnostic (it works for any uniform-height row list, not
just text lines), we rename these to "row"/"rows" (e.g., `first_visible_row`,
`visible_rows`). `Viewport` keeps its public API unchanged by delegating through
thin wrappers that preserve the "line" terminology.

## Subsystem Considerations

No subsystems directory exists in this project. This chunk does not touch any
existing cross-cutting patterns.

## Sequence

### Step 1: Create `RowScroller` struct definition with failing tests

Create `crates/editor/src/row_scroller.rs` with:

1. The `RowScroller` struct definition:
   ```rust
   pub struct RowScroller {
       scroll_offset_px: f32,
       visible_rows: usize,
       row_height: f32,
   }
   ```

2. Stub implementations for all thirteen methods (returning placeholder values or
   panicking with `todo!()`).

3. A comprehensive test module with tests for all thirteen methods, covering:
   - Basic construction (`new` with zero scroll, zero visible rows)
   - Getters (`row_height`, `visible_rows`, `scroll_offset_px`)
   - `first_visible_row` derivation from pixel offset
   - `scroll_fraction_px` calculation
   - `set_scroll_offset_px` with clamping
   - `update_size` computing `visible_rows` from height
   - `visible_range` with edge cases (empty, small buffer, fractional scroll)
   - `scroll_to` with clamping
   - `ensure_visible` for rows above, below, and already visible
   - `row_to_visible_offset` mapping
   - `visible_offset_to_row` mapping

Tests should initially fail (red phase).

Location: `crates/editor/src/row_scroller.rs`

### Step 2: Implement `RowScroller` methods

Implement all thirteen methods to make the tests pass (green phase). The formulas
are identical to the existing `Viewport` methods, with "line" renamed to "row":

| Method | Implementation |
|--------|----------------|
| `new(row_height)` | `RowScroller { scroll_offset_px: 0.0, visible_rows: 0, row_height }` |
| `row_height()` | Getter |
| `visible_rows()` | Getter |
| `first_visible_row()` | `(scroll_offset_px / row_height).floor() as usize` |
| `scroll_fraction_px()` | `scroll_offset_px % row_height` |
| `scroll_offset_px()` | Getter |
| `set_scroll_offset_px(px, row_count)` | Clamp to `[0, (row_count - visible_rows) * row_height]` |
| `update_size(height_px)` | `visible_rows = (height_px / row_height).floor() as usize` |
| `visible_range(row_count)` | `first..min(first + visible_rows + 1, row_count)` |
| `scroll_to(row, row_count)` | `set_scroll_offset_px(row * row_height, row_count)` |
| `ensure_visible(row, row_count)` | Scroll up/down if row outside window; return whether scrolled |
| `row_to_visible_offset(row)` | `Some(row - first_visible_row())` if visible, else `None` |
| `visible_offset_to_row(offset)` | `first_visible_row() + offset` |

Add module-level backreference:
```rust
// Chunk: docs/chunks/row_scroller_extract - RowScroller extraction from Viewport
```

Location: `crates/editor/src/row_scroller.rs`

### Step 3: Add `RowScroller` module to the crate

Add `mod row_scroller;` and `pub use row_scroller::RowScroller;` to `main.rs`
so that `RowScroller` is part of the editor crate's public surface.

Location: `crates/editor/src/main.rs`

### Step 4: Refactor `Viewport` to contain and delegate to `RowScroller`

Modify `Viewport` to:

1. Replace the three fields (`scroll_offset_px`, `visible_lines`, `line_height`)
   with a single `scroller: RowScroller` field.

2. Update `new()` to create an inner `RowScroller`.

3. Delegate the thirteen shared methods to `self.scroller`, preserving the
   existing public API (method names stay as `first_visible_line`, etc.):
   - `line_height()` → `self.scroller.row_height()`
   - `visible_lines()` → `self.scroller.visible_rows()`
   - `first_visible_line()` → `self.scroller.first_visible_row()`
   - `scroll_fraction_px()` → `self.scroller.scroll_fraction_px()`
   - `scroll_offset_px()` → `self.scroller.scroll_offset_px()`
   - `set_scroll_offset_px(px, count)` → `self.scroller.set_scroll_offset_px(px, count)`
   - `update_size(height)` → `self.scroller.update_size(height)`
   - `visible_range(count)` → `self.scroller.visible_range(count)`
   - `scroll_to(line, count)` → `self.scroller.scroll_to(line, count)`
   - `ensure_visible(line, count)` → `self.scroller.ensure_visible(line, count)`
   - `buffer_line_to_screen_line(line)` → `self.scroller.row_to_visible_offset(line)`
   - `screen_line_to_buffer_line(screen)` → `self.scroller.visible_offset_to_row(screen)`

4. Keep `dirty_lines_to_region` and `ensure_visible_wrapped` as `Viewport`-only
   methods. These call the delegated getters (e.g., `self.first_visible_line()`)
   to access state.

5. Add a public accessor:
   ```rust
   pub fn row_scroller(&self) -> &RowScroller {
       &self.scroller
   }
   ```

Location: `crates/editor/src/viewport.rs`

### Step 5: Verify all existing tests pass

Run `cargo test -p lite-edit` to confirm:
- All new `RowScroller` tests pass
- All existing `Viewport` tests pass unchanged

No test modifications should be needed — this confirms the refactor is behavioral
no-op.

### Step 6: Add documentation

Add doc comments to `RowScroller` explaining:
- Its purpose (fractional pixel scroll arithmetic for uniform-height rows)
- How it differs from `Viewport` (no buffer/wrap dependencies)
- How downstream code (e.g., `SelectorWidget`) can use it directly

Ensure all public methods have `///` doc comments explaining their behavior.

## Dependencies

None. This chunk is independent of the other chunks in the narrative
(`selector_coord_flip` can be implemented in parallel). The existing `Viewport`
implementation provides all the logic to extract.

## Risks and Open Questions

1. **Terminology consistency**: The GOAL.md lists "row_to_visible_offset" and
   "visible_offset_to_row" as `RowScroller` methods, but `Viewport` has these as
   `buffer_line_to_screen_line` and `screen_line_to_buffer_line`. The plan
   delegates through thin wrappers to preserve `Viewport`'s existing API. Confirm
   this is the intended behavior — callers of `Viewport` should not need changes.

2. **Test coverage**: The existing `Viewport` tests are comprehensive. Running
   them unchanged after the refactor is the primary validation that behavior is
   preserved. If any test fails, the refactor has introduced a bug.

3. **Crate visibility**: The editor is currently a binary crate (no `lib.rs`).
   Exposing `RowScroller` publicly via `pub use` in `main.rs` works, but the
   downstream `selector_row_scroller` chunk will need to import it correctly.
   Verify the import path works: `use crate::row_scroller::RowScroller;` (or
   `use crate::RowScroller;` if re-exported).

## Deviations

<!-- Populated during implementation -->