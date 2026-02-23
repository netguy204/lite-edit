<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The optimization targets two functions in `crates/editor/src/glyph_buffer.rs`:

1. **`update_from_buffer_with_cursor`** — The non-wrap rendering path
2. **`update_from_buffer_with_wrap`** — The wrap-enabled rendering path

Both functions currently call `view.styled_line(buffer_line)` multiple times per visible buffer line (3× in the non-wrap path, 3× in the wrap path), once in each rendering phase that needs styled content:

- Phase 1: Background quads (checks for non-default background colors)
- Phase 3: Glyph quads (renders actual text with per-span foreground colors)
- Phase 4: Underline quads (renders underlines for underlined spans)

Each call returns a `StyledLine` struct containing a `Vec<Span>` where each `Span` owns a `String`. The `styled_line()` method clones this data from the buffer's internal representation (or highlight cache for `HighlightedBufferView`).

**Strategy**: Pre-collect `StyledLine` results once per visible buffer line into a temporary `Vec<Option<StyledLine>>` at the start of each rendering function, then reference this collection in each phase instead of calling `view.styled_line()` again.

**Why `Vec<Option<StyledLine>>` instead of borrowing**: The `BufferView` trait returns `Option<StyledLine>` by value (owned), not by reference. We cannot change the trait signature without breaking all implementors. Pre-collecting into a `Vec` trades one upfront allocation (the `Vec` itself) for eliminating 2 redundant `StyledLine` clones per line per frame.

**Testing approach**: Since this is a refactoring that should produce identical output, the primary verification is:
1. All existing tests pass (no behavioral change)
2. Visual verification that rendered output is unchanged

This is an optimization refactoring, not new functionality, so per TESTING_PHILOSOPHY.md we verify via existing tests rather than adding new unit tests for the optimization itself. The goal is behavior preservation, which is confirmed by test suite continuity.

## Subsystem Considerations

No subsystems are directly relevant to this change. The `viewport_scroll` subsystem is not touched—this chunk only optimizes how `GlyphBuffer` consumes data from `BufferView.styled_line()`, which is an internal rendering implementation detail unrelated to scroll coordinate mapping.

## Sequence

### Step 1: Refactor `update_from_buffer_with_cursor` to pre-collect styled lines

Location: `crates/editor/src/glyph_buffer.rs`, function `update_from_buffer_with_cursor`

**Current pattern** (lines 559-563, 604-633, 689-741, 750-787):
```rust
// Estimation loop (OK - just for capacity)
for line in visible_range.clone() {
    if let Some(styled_line) = view.styled_line(line) { ... }
}
// Phase 1: Background quads
for buffer_line in visible_range.clone() {
    if let Some(styled_line) = view.styled_line(buffer_line) { ... }  // Call #1
}
// Phase 3: Glyph quads
for buffer_line in visible_range.clone() {
    if let Some(styled_line) = view.styled_line(buffer_line) { ... }  // Call #2
}
// Phase 4: Underline quads
for buffer_line in visible_range.clone() {
    if let Some(styled_line) = view.styled_line(buffer_line) { ... }  // Call #3
}
```

**New pattern**:
```rust
// Pre-collect styled lines once (combines estimation and collection)
let styled_lines: Vec<Option<StyledLine>> = visible_range.clone()
    .map(|line| view.styled_line(line))
    .collect();

// Use styled_lines[i] where i is the index into visible_range
// Phase 1, 3, 4 reference the pre-collected data
```

The key change: iterate once, store results, reference by index in each phase.

**Index calculation**: For `buffer_line` in `visible_range`, the index into `styled_lines` is:
```rust
let idx = buffer_line - viewport.first_visible_line();
```

### Step 2: Refactor `update_from_buffer_with_wrap` to pre-collect styled lines

Location: `crates/editor/src/glyph_buffer.rs`, function `update_from_buffer_with_wrap`

This function is more complex because it iterates buffer lines starting from `first_visible_buffer_line` (not `visible_range`), and tracks `cumulative_screen_row` to know when to stop.

**Current pattern**:
- Phase 1 (lines 1209-1290): Calls `view.styled_line(buffer_line)` in loop
- Phase 3 (lines 1439-1539): Calls `view.styled_line(buffer_line)` via `if let Some(sl)`
- Phase 4 (lines 1548-1635): Calls `view.styled_line(buffer_line)` in loop

**New pattern**:

Pre-collect styled lines for all buffer lines that will be rendered:
```rust
// Determine which buffer lines will be rendered (before phases begin)
let mut rendered_buffer_lines: Vec<usize> = Vec::new();
{
    let mut cumulative_screen_row: usize = 0;
    let mut is_first_buffer_line = true;
    for buffer_line in first_visible_buffer_line..line_count {
        if cumulative_screen_row >= max_screen_rows {
            break;
        }
        rendered_buffer_lines.push(buffer_line);
        let line_len = view.line_len(buffer_line);
        let rows_for_line = wrap_layout.screen_rows_for_line(line_len);
        let start_row_offset = if is_first_buffer_line { screen_row_offset_in_line } else { 0 };
        is_first_buffer_line = false;
        cumulative_screen_row += rows_for_line - start_row_offset;
    }
}

// Pre-collect styled lines for these buffer lines
let styled_lines: Vec<Option<StyledLine>> = rendered_buffer_lines.iter()
    .map(|&line| view.styled_line(line))
    .collect();

// Then in each phase, iterate rendered_buffer_lines.iter().zip(styled_lines.iter())
```

Alternatively, use a `HashMap<usize, StyledLine>` keyed by buffer line, but since buffer lines are contiguous starting from `first_visible_buffer_line`, a `Vec` indexed by `buffer_line - first_visible_buffer_line` is simpler.

### Step 3: Verify all tests pass

Run:
```bash
cargo test -p lite-edit
cargo test -p lite-edit-buffer
```

This confirms the refactoring preserves existing behavior. All existing `glyph_buffer` tests should pass unchanged.

### Step 4: Manual visual verification

Build and run the editor, verify:
- Syntax-highlighted file displays correctly
- Scrolling renders correctly
- Selection highlighting works
- Underlines (if any) render correctly

This is important because the rendering code has no functional tests beyond the unit tests—visual correctness is the ultimate arbiter.

---

**BACKREFERENCE COMMENTS**

Add a chunk backreference at the top of the modified section in `glyph_buffer.rs`:
```rust
// Chunk: docs/chunks/glyph_single_styled_line - Pre-collect styled lines to avoid redundant calls
```

## Dependencies

No dependencies. This chunk modifies only `glyph_buffer.rs` and relies only on existing types (`StyledLine`, `BufferView`).

## Risks and Open Questions

1. **Memory overhead of pre-collection**: Pre-collecting `styled_lines` adds a `Vec` allocation per frame. However, this is offset by avoiding 2 redundant `StyledLine` clones per line. Net memory should be lower since we allocate the same `StyledLine` data once instead of 3×.

2. **Phase iteration divergence**: The wrap-enabled path has complex iteration logic tracking `cumulative_screen_row`, `start_row_offset`, etc. We must ensure all phases still iterate in the same order and with the same bounds. Using `.iter().zip()` or indexed access should maintain consistency.

3. **Edge case: empty visible range**: If `visible_range` is empty or no buffer lines are rendered, the `styled_lines` Vec will be empty. Phases should handle this gracefully (empty loops produce no quads, which is correct).

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

Example:
- Step 4: Originally planned to use std::fs::rename for atomic swap.
  Testing revealed this isn't atomic across filesystems. Changed to
  write-fsync-rename-fsync sequence per platform best practices.
-->