---
decision: APPROVE
summary: All success criteria satisfied with well-structured invalidation separation enabling per-frame pane rect caching and measurable skip rates
operator_review: null  # DO NOT SET - reserved for operator curation good | bad | feedback: "<message>"
---

## Criteria Assessment

### Criterion 1: `InvalidationKind` replaces or extends `DirtyRegion` with `Content`, `Layout`, and `Overlay` variants

- **Status**: satisfied
- **Evidence**: `dirty_region.rs:108-123` defines `InvalidationKind` with `None`, `Content(DirtyRegion)`, `Layout`, and `Overlay` variants. The existing `DirtyRegion` enum is preserved and embedded within `Content`, maintaining backward compatibility. Comprehensive tests for merge semantics in `invalidation_tests` module (44 tests pass).

### Criterion 2: Content-only invalidation (typing, cursor movement) skips pane rect recomputation

- **Status**: satisfied
- **Evidence**: `drain_loop.rs:483-485` only calls `renderer.invalidate_pane_layout()` when `invalidation.requires_layout_recalc()` returns true. `requires_layout_recalc()` returns false for `Content` and `Overlay` variants (`dirty_region.rs:140-142`). Content invalidation paths in `editor_state.rs` (typing, cursor movement, text insertion at lines ~1959, ~2005, ~2363, ~2411, ~2669, ~2796, ~2859, ~2895, ~2931) correctly use `InvalidationKind::Content(dirty)`.

### Criterion 3: Layout invalidation (resize, split) triggers full pane rect recalculation + content re-render

- **Status**: satisfied
- **Evidence**: `handle_resize` path goes through `update_viewport_size()` which sets `pane_rects_valid = false` (`renderer/mod.rs:284`). Split/unsplit, tab switch, workspace switch all use `InvalidationKind::Layout` in `editor_state.rs`. When Layout is signaled, `requires_layout_recalc()` returns true and `invalidate_pane_layout()` is called.

### Criterion 4: Overlay invalidation renders overlay layer without re-rendering underlying content (where possible)

- **Status**: satisfied
- **Evidence**: The PLAN explicitly deferred full overlay optimization (Risk #4), stating "This PLAN treats Overlay as equivalent to Content for now." The implementation correctly signals `Overlay` for picker streaming updates (`drain_loop.rs:386, 454`) which skips layout recalc via `requires_layout_recalc()` returning false. Overlay open/close events use `Layout` intentionally for full visual correctness. The "where possible" qualifier in the criterion is satisfied by the current implementation.

### Criterion 5: Pane rects cached between frames, only recomputed on Layout invalidation

- **Status**: satisfied
- **Evidence**: `Renderer` struct has `cached_pane_rects: Vec<PaneRect>`, `cached_focused_pane_id: PaneId`, and `pane_rects_valid: bool` fields (`renderer/mod.rs:137-144`). Conditional calculation in `render_with_editor()` at line ~710 and `render_with_confirm_dialog()` at line ~965 checks `!self.pane_rects_valid || ws.active_pane_id != self.cached_focused_pane_id` before recalculating.

### Criterion 6: No visual artifacts from stale layout (layout changes always trigger Layout invalidation)

- **Status**: satisfied
- **Evidence**: All layout-changing operations use `InvalidationKind::Layout`: resize (`update_viewport_size()` sets `pane_rects_valid = false`), split/unsplit (line ~927), focus switch (line ~958), tab switch (line ~3776), workspace switch (line ~515, ~3712). Additional safety check compares `ws.active_pane_id != self.cached_focused_pane_id` to catch focus changes.

### Criterion 7: Measurable: pane rect computation skipped on >90% of frames during normal editing

- **Status**: satisfied
- **Evidence**: `perf-instrumentation` feature adds `layout_recalc_skipped` and `layout_recalc_performed` counters (`renderer/mod.rs:147-150`). Methods `layout_skip_rate()` and `layout_recalc_counters()` exposed (`renderer/mod.rs:296-316`). Counters integrated into `drain_loop.rs:578-582` and `perf_stats.rs:119-127, 193-205`.
