---
decision: FEEDBACK
summary: "Cache data structure and unit tests are solid, but invalidation is never called—cache will serve stale data after edits."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `StyledLineCache` caches computed `StyledLine` per buffer line index

- **Status**: satisfied
- **Evidence**: `crates/editor/src/styled_line_cache.rs` implements `StyledLineCache` with a `Vec<Option<StyledLine>>` keyed by line index. `GlyphBuffer` owns a `styled_line_cache: StyledLineCache` field (line 294) and uses `self.styled_line_cache.get(line)` in the render path (lines 639, 648, 1308, 1317).

### Criterion 2: Cache invalidated correctly by `DirtyLines::Single`, `Range`, `FromLineToEnd`

- **Status**: gap
- **Evidence**: The `invalidate()` method correctly handles all variants (lines 87-111 of styled_line_cache.rs). Unit tests verify this behavior (17 passing tests). However, **the invalidation is never actually invoked**: `renderer.invalidate_styled_lines()` and `glyph_buffer.invalidate_styled_lines()` are defined but never called anywhere in the codebase. `EditorState.take_dirty_lines()` exists but is never called. The signal flow from buffer mutations to cache invalidation is broken.

### Criterion 3: Unchanged lines serve from cache with zero allocation

- **Status**: gap
- **Evidence**: The cache-first pattern exists in `update_glyphs_wrapped()` (lines 638-649) and `update_glyphs()` (lines 1306-1318). However, because invalidation is never called, ALL lines will eventually be cached and served from cache—including lines that HAVE changed. This produces wrong output, not zero-allocation correctness.

### Criterion 4: Line insertion/deletion correctly shifts cache entries (or invalidates from point of change)

- **Status**: gap
- **Evidence**: `FromLineToEnd` handling truncates the cache (line 102-108), which is the correct conservative approach per the PLAN. However, `FromLineToEnd` is never passed to `invalidate()` because the entire invalidation path is disconnected.

### Criterion 5: Buffer switch / tab change clears or replaces cache

- **Status**: gap
- **Evidence**: `clear_styled_line_cache()` exists on both `GlyphBuffer` (line 370) and `Renderer` (line 318 of mod.rs). Neither method is ever called. Buffer/tab switch will serve stale cache entries from the previous buffer.

### Criterion 6: Measurable: during steady-state typing, heap allocations per frame reduced by ~90% for styled_line path

- **Status**: unclear
- **Evidence**: `CacheStats` is implemented for `perf-instrumentation` feature (lines 144-185). However, without proper invalidation, the cache will show 100% hit rate (incorrectly—it's serving stale data). The allocation reduction cannot be properly measured until the implementation is correct.

### Criterion 7: No visual artifacts — cache coherence verified by existing rendering tests

- **Status**: gap
- **Evidence**: Unit tests for cache logic pass, but the integration is broken. Since invalidation never happens, edited text will NOT be re-rendered. This WILL cause visual artifacts—typed characters won't appear until the cache is somehow cleared (e.g., by scroll repositioning that accesses new lines).

### Criterion 8: Terminal buffers (which change many lines per PTY read) also benefit when only partial grid updates occur

- **Status**: gap
- **Evidence**: The cache is shared between text buffers and terminal buffers (single `GlyphBuffer` per pane). Terminal output changes many lines per PTY read, but no invalidation call means terminal content will also be stale after the first render.

## Feedback Items

### Issue 1: Cache invalidation path not connected

- **Location**: `crates/editor/src/drain_loop.rs:432` (and surrounding render loop)
- **Concern**: The render loop calls `self.state.take_dirty_region()` but does NOT call `self.state.take_dirty_lines()` to pass to `self.renderer.invalidate_styled_lines()`. The cache is never told which lines changed.
- **Suggestion**: Add after line 432 in `render_if_dirty()`:
  ```rust
  let dirty_lines = self.state.take_dirty_lines();
  self.renderer.invalidate_styled_lines(&dirty_lines);
  ```
- **Severity**: functional
- **Confidence**: high

### Issue 2: Buffer/tab switch doesn't clear cache

- **Location**: Tab switch handling (various locations in editor_state.rs)
- **Concern**: When switching tabs, `clear_styled_line_cache()` is never called. Stale content from previous buffer will render.
- **Suggestion**: Either call `renderer.clear_styled_line_cache()` on tab switch, OR (better) track buffer identity and clear when it changes. The PLAN (Step 5) suggests tracking buffer identity in `Renderer::render_pane()`.
- **Severity**: functional
- **Confidence**: high

### Issue 3: EditorContext doesn't accumulate dirty_lines for mutations

- **Location**: `crates/editor/src/context.rs` and `buffer_target.rs`
- **Concern**: `EditorContext.mark_dirty()` receives `DirtyLines` but only converts to `DirtyRegion`, discarding the `DirtyLines`. The `dirty_lines` field exists on `EditorState` but is only populated in the drag-drop path (`editor_state.rs:2678`), not the main editing path.
- **Suggestion**: Either pass `&mut DirtyLines` to `EditorContext` and accumulate there, OR ensure all buffer mutations in `editor_state.rs` call `self.dirty_lines.merge()` (similar to line 2678).
- **Severity**: functional
- **Confidence**: high
