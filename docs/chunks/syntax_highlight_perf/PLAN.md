<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The root cause is that `SyntaxHighlighter::highlight_line()` creates a new `Highlighter` and calls `highlight()` on the **full source** for every single line request. The `Highlighter::highlight()` API does its own internal parse, completely bypassing the cached `self.tree` that's properly maintained by the incremental `edit()` path. The renderer calls `highlight_line()` once per visible line (~60 calls per frame), so we get 60 full-file parses per render frame.

**The fix has two parts:**

1. **Use `QueryCursor` directly against the cached tree**: Instead of calling `Highlighter::highlight()` (which reparses), use tree-sitter's lower-level `QueryCursor` API to run highlight queries directly against `self.tree`. The `QueryCursor` supports `set_byte_range()` for viewport-scoping.

2. **Batch highlighting for the viewport**: Instead of highlighting line-by-line, add a method to highlight a range of lines in a single pass. The renderer (or `HighlightedBufferView`) can call this once per frame with the visible line range, then cache results per-line.

**Alternative considered**: Caching per-line highlight results with invalidation on edits. This adds complexity around cache invalidation (which lines changed after an edit?) and memory pressure. The viewport-batch approach is simpler: compute once per frame, discard on next frame.

**Performance target from investigation (H2 benchmark)**:
- Incremental parse: ~120µs per single-char edit
- Viewport highlight (60 lines): ~170µs
- Combined: ~290µs (3.6% of 8ms budget)

Currently the implementation spends 14.5ms+ per frame due to 60× full-file reparses.

## Subsystem Considerations

No subsystems are directly relevant to this performance fix. The `viewport_scroll` subsystem is not affected since this is purely internal to the syntax highlighting module.

## Sequence

### Step 1: Add viewport-batch highlight method using QueryCursor

Add a new method `highlight_viewport(&self, start_line: usize, end_line: usize) -> Vec<StyledLine>` that:

1. Calculates the byte range for `start_line..end_line`
2. Creates a `QueryCursor` and calls `set_byte_range()` to limit queries to the viewport
3. Loads the highlight query and runs `cursor.captures()` against `self.tree`
4. Builds `StyledLine` objects from the captures, grouping by line
5. Returns the styled lines for the viewport

This method uses the already-parsed `self.tree` rather than the `Highlighter::highlight()` API which reparses.

**Key tree-sitter APIs**:
- `QueryCursor::new()` - create cursor
- `cursor.set_byte_range(start..end)` - limit to viewport bytes
- `cursor.captures(&query, tree.root_node(), source.as_bytes())` - iterate captures
- Each capture has `.node.start_byte()`, `.node.end_byte()`, and `.index` (capture index)

**Location**: `crates/syntax/src/highlighter.rs`

### Step 2: Add highlight Query to SyntaxHighlighter

The current implementation uses `HighlightConfiguration` for the `Highlighter::highlight()` API. For direct `QueryCursor` usage, we need to store a compiled `Query` object.

Add a field `query: Query` to `SyntaxHighlighter`. In `new()`, compile the highlight query:
```rust
let query = Query::new(&config.language, config.highlights_query)?;
```

Update `LanguageConfig::highlight_config()` to also return the highlights query string so it can be compiled separately for the `QueryCursor` path.

**Location**: `crates/syntax/src/highlighter.rs`, `crates/syntax/src/registry.rs`

### Step 3: Map captures to styles in viewport highlight

In `highlight_viewport()`, iterate captures and map them to `Style` using the existing `SyntaxTheme`:

1. For each capture, look up `capture.index` in the query's `capture_names()`
2. Use `theme.style_for_capture(capture_name)` to get the style
3. Track current position, build spans with styles, handle overlapping captures via a style stack

The logic is similar to `build_styled_line()` but operates on captures from `QueryCursor` rather than `HighlightEvent`s.

**Location**: `crates/syntax/src/highlighter.rs`

### Step 4: Add highlight cache to SyntaxHighlighter

Add a simple cache to avoid re-highlighting the same viewport every frame when nothing changed:

```rust
struct HighlightCache {
    start_line: usize,
    end_line: usize,
    lines: Vec<StyledLine>,
    generation: u64, // incremented on each edit()
}
```

In `highlight_viewport()`:
1. Check if cache matches the requested range and current generation
2. If hit, return cached lines
3. If miss, compute highlights and cache them

In `edit()` and `update_source()`:
1. Increment generation counter (invalidates cache)

**Location**: `crates/syntax/src/highlighter.rs`

### Step 5: Update highlight_line to use cached viewport results

Modify `highlight_line()` to:
1. Check if the requested line is in the cached viewport
2. If yes, return the cached `StyledLine` directly
3. If no, fall through to single-line computation (for edge cases)

This is a transitional step. Ideally the renderer would call `highlight_viewport()` directly, but maintaining `highlight_line()` API compatibility reduces change scope.

**Location**: `crates/syntax/src/highlighter.rs`

### Step 6: Update HighlightedBufferView to batch-highlight

Modify `HighlightedBufferView::styled_line()` to leverage the viewport cache efficiently:

Since `styled_line()` is called in order by the renderer (line 0, 1, 2...), detect when we're starting a new viewport and trigger `highlight_viewport()` for the expected range.

Alternative approach: The renderer could be modified to call a viewport-batch method directly. But this requires changing the `BufferView` trait or the render loop. For minimal change, keep using `highlight_line()` and rely on the cache.

**Location**: `crates/editor/src/highlighted_buffer.rs`

### Step 7: Add benchmark test for viewport highlighting

Add a test that verifies viewport highlighting completes within the performance budget:

```rust
#[test]
fn test_viewport_highlight_performance() {
    // Load a large Rust file (~5K lines)
    // Highlight 60-line viewport
    // Assert total time < 1ms (leaving headroom for the 8ms budget)
}
```

Use a representative file from the codebase (e.g., `editor_state.rs` as in the investigation).

**Location**: `crates/syntax/src/highlighter.rs` (test module)

### Step 8: Verify existing tests pass and add regression tests

1. Run existing `syntax` crate tests to ensure behavior is preserved
2. Add test: editing a file updates highlighting correctly (cache invalidation)
3. Add test: `highlight_line()` for lines outside viewport still works
4. Add test: empty file, single-line file edge cases

**Location**: `crates/syntax/src/highlighter.rs` (test module)

### Step 9: Remove dead code path

After the new implementation is working, the old per-line `Highlighter::highlight()` path in `highlight_line()` becomes dead code for normal operation. Either:
- Remove it entirely if no longer needed
- Keep it as a fallback for edge cases (files without trees, error recovery)

Document the decision in code comments.

**Location**: `crates/syntax/src/highlighter.rs`

## Dependencies

- **syntax_highlighting chunk** (ACTIVE): This chunk fixes a bug in the implementation from that chunk. The ACTIVE status means we're building on shipped work.

No new external dependencies needed. We use tree-sitter's `Query` and `QueryCursor` APIs which are already available via the `tree-sitter` crate.

## Risks and Open Questions

1. **QueryCursor capture ordering**: Captures may not be returned in byte-order. Need to verify the iteration order and sort if necessary for proper span construction.

2. **Injection languages**: The current `Highlighter::highlight()` API handles injection languages (e.g., JS inside HTML) via the `injection_callback`. The `QueryCursor` approach may need additional handling for injections. Initial implementation can skip injection support and document as a known limitation for HTML/Markdown files.

3. **Cache memory**: Caching 60 `StyledLine` objects per viewport is minimal (~KB), but verify memory impact for files with very long lines.

4. **Thread safety**: `SyntaxHighlighter` is used from a single thread (the render thread). The cache doesn't need synchronization, but document this assumption.

5. **Query compilation cost**: Compiling the `Query` once in `new()` should be fine (one-time cost on file open). Verify this doesn't regress file open latency significantly.

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