<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

This chunk optimizes three secondary inefficiencies in the syntax highlighting hot path, targeting the ~490µs residual cost at line 0 (where the O(n) line scanning is already eliminated by the `highlight_line_offset_index` chunk).

**Strategy:**

1. **Replace String allocation with u32 capture index** — The current `collect_captures_in_range()` calls `(*name).to_string()` for every tree-sitter capture, creating hundreds of heap allocations per viewport. Instead, store the `capture.index` directly as a `u32`, and resolve the capture name lazily at theme lookup time via `self.query.capture_names().get(idx)`.

2. **Binary search for per-line capture filtering** — The current `build_line_from_captures()` linearly scans ALL viewport captures for each line. Since captures are sorted by start byte, use `partition_point()` to binary-search to the first capture that might overlap each line.

3. **Reuse captures Vec across frames** — Add a `RefCell<Vec<CaptureEntry>>` field to `SyntaxHighlighter` and clear+reuse it instead of allocating a fresh `Vec` on every `highlight_viewport()` call.

These are incremental, non-breaking changes to existing methods. All existing tests should continue to pass, and rendered output should be identical.

**Testing approach (per TESTING_PHILOSOPHY.md):**
- Write a test first that asserts the optimized capture type (`u32` index) is used
- Existing tests validate visual parity (no functional change)
- The profiling test in the investigation can be re-run to measure improvement

## Sequence

### Step 1: Define a CaptureEntry type alias

Introduce a type alias or struct to represent a capture entry:
```rust
/// A capture entry: (start_byte, end_byte, capture_index).
/// The capture_index is used to look up the capture name from Query::capture_names().
type CaptureEntry = (usize, usize, u32);
```

This clarifies the tuple semantics and makes the change from `String` to `u32` explicit.

Location: `crates/syntax/src/highlighter.rs` (near the top, after imports)

### Step 2: Add reusable captures buffer field to SyntaxHighlighter

Add a `RefCell<Vec<CaptureEntry>>` field to the struct for reusing the captures vector:

```rust
pub struct SyntaxHighlighter {
    // ... existing fields ...
    /// Reusable buffer for captures to avoid per-frame allocation.
    captures_buffer: RefCell<Vec<CaptureEntry>>,
}
```

Initialize it in `new()` with `RefCell::new(Vec::new())`.

Location: `crates/syntax/src/highlighter.rs#SyntaxHighlighter`

### Step 3: Modify collect_captures_in_range to use u32 index and reuse buffer

Change the signature and implementation:
- Instead of returning `Vec<(usize, usize, String)>`, return nothing (or `&[CaptureEntry]`)
- Clear and populate `self.captures_buffer` in-place
- Store `capture.index` (u32) instead of calling `(*name).to_string()`
- Sort the buffer by start byte (as before)

The function becomes:
```rust
fn collect_captures_in_range(&self, start_byte: usize, end_byte: usize) {
    let mut buffer = self.captures_buffer.borrow_mut();
    buffer.clear();

    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(start_byte..end_byte);
    // ... iterate captures, push (start, end, capture.index as u32) ...

    buffer.sort_by_key(|(start, _, _)| *start);
}
```

Note: Since `captures_buffer` is a `RefCell`, callers will need to borrow it after calling this method. This changes the internal API but not the public API.

Location: `crates/syntax/src/highlighter.rs#SyntaxHighlighter::collect_captures_in_range`

### Step 4: Update highlight_viewport to use the reusable buffer

Modify `highlight_viewport()` to:
1. Call `self.collect_captures_in_range(viewport_start, viewport_end)`
2. Borrow `self.captures_buffer` for reading
3. Pass the borrowed slice to `build_line_from_captures()`

```rust
self.collect_captures_in_range(viewport_start, viewport_end);
let captures = self.captures_buffer.borrow();

for line_idx in start_line..end_line {
    let styled = self.build_line_from_captures(line_idx, &captures);
    lines.push(styled);
}
```

Location: `crates/syntax/src/highlighter.rs#SyntaxHighlighter::highlight_viewport`

### Step 5: Update build_line_from_captures for u32 capture index

Change the signature to accept `&[(usize, usize, u32)]` and update the theme lookup:

```rust
fn build_line_from_captures(&self, line_idx: usize, captures: &[CaptureEntry]) -> StyledLine {
    // ... same logic, but for theme lookup:
    let capture_name = self.query.capture_names().get(*cap_idx as usize);
    if let Some(name) = capture_name {
        if let Some(style) = self.theme.style_for_capture(name) {
            spans.push(Span::new(capture_text, *style));
        } else {
            spans.push(Span::plain(capture_text));
        }
    } else {
        spans.push(Span::plain(capture_text));
    }
}
```

Location: `crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures`

### Step 6: Add binary search optimization to build_line_from_captures

Use `partition_point()` to find the first capture that could overlap the current line, instead of scanning from index 0:

```rust
let (line_start, line_end) = self.line_byte_range(line_idx)?;

// Binary search to find first capture that could overlap this line.
// A capture at (cap_start, cap_end) overlaps if cap_end > line_start.
let first_relevant = captures.partition_point(|(_, cap_end, _)| *cap_end <= line_start);

for (cap_start, cap_end, cap_idx) in &captures[first_relevant..] {
    // Early exit: captures are sorted by start, so once start >= line_end, no more overlap
    if *cap_start >= line_end {
        break;
    }
    // ... process capture ...
}
```

This reduces per-line iteration from O(total_captures) to O(overlapping_captures + log(total_captures)).

Location: `crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_line_from_captures`

### Step 7: Update build_styled_line_from_query for consistency

Apply the same changes to `build_styled_line_from_query()` (the single-line fallback path):
- Use `u32` capture index
- Use lazy name lookup via `self.query.capture_names()`
- No binary search needed here since it's a single-line range

This method calls `collect_captures_in_range()` for a single line, so it will automatically benefit from the buffer reuse. However, we need to update its local iteration to handle the new tuple type.

Location: `crates/syntax/src/highlighter.rs#SyntaxHighlighter::build_styled_line_from_query`

### Step 8: Add backreference comment

Add a chunk backreference at the top of the modified section:
```rust
// Chunk: docs/chunks/highlight_capture_alloc - Reduce per-frame allocations in hot path
```

Location: Near the `collect_captures_in_range` function definition

### Step 9: Run existing tests and verify parity

Run all existing highlighter tests to ensure visual parity:
```bash
cargo test -p lite-edit-syntax
```

Key tests to verify:
- `test_no_duplicate_text_from_overlapping_captures`
- `test_viewport_highlight_populates_cache`
- `test_highlight_line_returns_styled_line`
- `test_keyword_has_style`, `test_string_has_style`, `test_comment_has_style`

### Step 10: Run profiling test to measure improvement

Run the investigation's profiling test to measure the impact:
```bash
cargo test -p lite-edit-syntax --release --test profile_scroll -- --nocapture
```

Expected: ~100-300µs reduction in the base highlight cost at line 0 (from ~490µs to ~200-400µs), with most savings from eliminated String allocations.

## Dependencies

- **highlight_line_offset_index** (ACTIVE): This chunk must be complete first. It provides the O(1) line byte range lookups that this chunk's binary search optimization depends on. ✓ Already complete.

## Risks and Open Questions

1. **RefCell borrow conflicts** — The `captures_buffer` is borrowed mutably in `collect_captures_in_range()` and immutably in `build_line_from_captures()`. Since these are called sequentially (never overlapping), there's no runtime borrow conflict. However, if future refactoring changes the call pattern, this could panic. Mitigation: The sequential call pattern is clear in `highlight_viewport()`.

2. **Capture index validity** — We assume `capture.index` is a valid index into `Query::capture_names()`. This is guaranteed by tree-sitter's API, but if the assumption is violated, we gracefully fall back to plain styling (via the `else` branch).

3. **Performance measurement variance** — Microbenchmark results can vary significantly between runs. The expected 100-300µs savings may be hard to measure reliably. Mitigation: Run the profiling test multiple times and look for consistent improvement.

4. **Binary search edge case** — The `partition_point()` predicate must correctly handle captures that span multiple lines. The current predicate `*cap_end <= line_start` correctly identifies captures that end before the current line starts. Captures starting exactly at `line_end` won't overlap (since `line_end` is exclusive of the newline).

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