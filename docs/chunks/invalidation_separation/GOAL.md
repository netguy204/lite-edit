---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/dirty_region.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/drain_loop.rs
- crates/editor/src/renderer/mod.rs
- crates/editor/src/pane_layout.rs
code_references:
  - ref: crates/editor/src/dirty_region.rs#InvalidationKind
    implements: "Core invalidation enum with Content, Layout, and Overlay variants"
  - ref: crates/editor/src/dirty_region.rs#InvalidationKind::merge
    implements: "Merge semantics where Layout absorbs all, Overlay absorbs Content"
  - ref: crates/editor/src/dirty_region.rs#InvalidationKind::requires_layout_recalc
    implements: "Determines if pane rect recalculation is needed (only Layout)"
  - ref: crates/editor/src/editor_state.rs#EditorState::invalidation
    implements: "Invalidation tracking field replacing dirty_region"
  - ref: crates/editor/src/editor_state.rs#EditorState::take_invalidation
    implements: "Takes accumulated invalidation for rendering"
  - ref: crates/editor/src/editor_state.rs#EditorState::mark_full_dirty
    implements: "Layout invalidation helper for full rerender"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::cached_pane_rects
    implements: "Cached pane rectangles from last layout calculation"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::invalidate_pane_layout
    implements: "Method to invalidate cached pane rects on Layout signal"
  - ref: crates/editor/src/drain_loop.rs#EventDrainLoop::render_if_dirty
    implements: "Conditional layout invalidation based on InvalidationKind"
narrative: null
investigation: null
subsystems:
- subsystem_id: renderer
  relationship: implements
friction_entries: []
bug_type: null
depends_on: []
created_after:
- typescript_highlight_layering
---

# Chunk Goal

## Minor Goal

All invalidation currently flows through a single `DirtyRegion` enum: content changes (typing), structural changes (pane resize, split), and overlay changes (find bar toggle) all go through the same path. The renderer recomputes pane layout rects on every frame regardless of whether layout actually changed. In Cocoa, `setNeedsDisplay:` vs `setNeedsLayout:` are separate because layout is more expensive than content redraw and they trigger at different frequencies.

Separate invalidation into distinct kinds so the renderer can skip work that hasn't changed:
- **Content**: Glyph changes within existing layout (typing, cursor blink) — skip layout recalculation
- **Layout**: Pane resize, split/unsplit, tab bar change — recompute pane rects then render content
- **Overlay**: Find bar, selector, dialog appeared/changed — render overlay layer only

During typical editing, 95%+ of frames are content-only. Skipping layout recalculation for these frames reduces per-frame overhead.

**Key files**: `crates/editor/src/dirty_region.rs` (DirtyRegion → InvalidationKind), `crates/editor/src/renderer.rs` (conditional layout recalculation), `crates/editor/src/drain_loop.rs` (event handlers signal invalidation kind), `crates/editor/src/pane_layout.rs` (layout computation that can be cached)

**Origin**: Architecture review recommendation #6 (P1 — Architecture/Performance). See `ARCHITECTURE_REVIEW.md`.

## Success Criteria

- `InvalidationKind` replaces or extends `DirtyRegion` with `Content`, `Layout`, and `Overlay` variants
- Content-only invalidation (typing, cursor movement) skips pane rect recomputation
- Layout invalidation (resize, split) triggers full pane rect recalculation + content re-render
- Overlay invalidation renders overlay layer without re-rendering underlying content (where possible)
- Pane rects cached between frames, only recomputed on Layout invalidation
- No visual artifacts from stale layout (layout changes always trigger Layout invalidation)
- Measurable: pane rect computation skipped on >90% of frames during normal editing

