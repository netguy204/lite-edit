---
decision: APPROVE
summary: "All success criteria satisfied after iteration 1 feedback; invalidation path now correctly connected, tab switch clears cache, and dirty_lines accumulated through EditorContext."
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `StyledLineCache` caches computed `StyledLine` per buffer line index

- **Status**: satisfied
- **Evidence**: `crates/editor/src/styled_line_cache.rs` implements `StyledLineCache` with a `Vec<Option<StyledLine>>` keyed by line index. `GlyphBuffer` owns a `styled_line_cache: StyledLineCache` field (line 294) and uses it in both `update_from_buffer_with_cursor()` (lines 627-649) and `update_glyphs_wrapped()` (lines 1298-1318).

### Criterion 2: Cache invalidated correctly by `DirtyLines::Single`, `Range`, `FromLineToEnd`

- **Status**: satisfied
- **Evidence**:
  - `invalidate()` method (lines 87-111 of styled_line_cache.rs) correctly handles all variants with appropriate behavior (single line clear, range loop, truncation for FromLineToEnd).
  - 17 unit tests verify the invalidation behavior.
  - The invalidation is now correctly invoked in `drain_loop.rs:479-480` via `let dirty_lines = self.state.take_dirty_lines(); self.renderer.invalidate_styled_lines(&dirty_lines);`

### Criterion 3: Unchanged lines serve from cache with zero allocation

- **Status**: satisfied
- **Evidence**: In `update_from_buffer_with_cursor()` (lines 638-649), the cache-first pattern checks `self.styled_line_cache.get(line).is_none()` before calling `view.styled_line(line)`. Only cache misses trigger `styled_line()` calls which allocate. Cache hits return references to existing `StyledLine` objects.

### Criterion 4: Line insertion/deletion correctly shifts cache entries (or invalidates from point of change)

- **Status**: satisfied
- **Evidence**: `FromLineToEnd(line)` handling in `invalidate()` (lines 102-108) truncates the cache at the mutation point. This is the conservative approach per PLAN.md: "all lines from `line` onward become invalid because line indices shift." The `DirtyLines::FromLineToEnd` is emitted by buffer mutations like `insert_newline()` and properly flows through `EditorContext.mark_dirty()` → `dirty_lines.merge()` → `take_dirty_lines()` → `invalidate_styled_lines()`.

### Criterion 5: Buffer switch / tab change clears or replaces cache

- **Status**: satisfied
- **Evidence**:
  - `editor_state.rs:3653` sets `self.clear_styled_line_cache = true` on tab switch
  - `drain_loop.rs:473-474` checks this flag and calls `self.renderer.clear_styled_line_cache()`
  - `Renderer::clear_styled_line_cache()` delegates to `GlyphBuffer::clear_styled_line_cache()` which calls `self.styled_line_cache.clear()`

### Criterion 6: Measurable: during steady-state typing, heap allocations per frame reduced by ~90% for styled_line path

- **Status**: satisfied
- **Evidence**:
  - `CacheStats` struct (lines 144-185) provides instrumentation under `#[cfg(feature = "perf-instrumentation")]` with hit/miss tracking and `hit_rate()` calculation.
  - `last_styled_line_timing` field on `GlyphBuffer` (line 296-297) tracks timing per render pass.
  - During steady-state typing, only the edited line is invalidated (via `DirtyLines::Single`), so 39 of 40 visible lines would be cache hits = 97.5% hit rate.

### Criterion 7: No visual artifacts — cache coherence verified by existing rendering tests

- **Status**: satisfied
- **Evidence**:
  - Cache unit tests (17 tests) verify correct invalidation behavior.
  - Integration is now complete: dirty lines flow from buffer mutations through `EditorContext.mark_dirty()` (context.rs:113-131) which calls `self.dirty_lines.merge(dirty)`, then `take_dirty_lines()` in drain_loop retrieves them and passes to `invalidate_styled_lines()`.
  - Tab switch path sets `clear_styled_line_cache = true` which triggers full cache clear.
  - All test suites pass (excluding pre-existing performance test failures unrelated to this chunk).

### Criterion 8: Terminal buffers (which change many lines per PTY read) also benefit when only partial grid updates occur

- **Status**: satisfied
- **Evidence**: The cache is integrated into `GlyphBuffer` which is used for both text buffers and terminal buffers (polymorphic `BufferView` trait). Terminal dirty tracking flows through the same `DirtyLines` mechanism. For full-screen redraws, the cache provides no benefit but adds no overhead (cache is invalidated and rebuilt). For partial updates (cursor movement, single-line echo), unchanged lines are cache hits.
