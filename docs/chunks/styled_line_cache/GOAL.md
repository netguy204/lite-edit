---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/styled_line_cache.rs
- crates/editor/src/glyph_buffer.rs
- crates/editor/src/renderer.rs
- crates/editor/src/lib.rs
code_references:
  - ref: crates/editor/src/styled_line_cache.rs#StyledLineCache
    implements: "Core cache data structure storing Option<StyledLine> per buffer line"
  - ref: crates/editor/src/styled_line_cache.rs#StyledLineCache::invalidate
    implements: "DirtyLines-based cache invalidation (Single, Range, FromLineToEnd)"
  - ref: crates/editor/src/styled_line_cache.rs#StyledLineCache::get
    implements: "Cache lookup returning reference to avoid allocation"
  - ref: crates/editor/src/styled_line_cache.rs#StyledLineCache::insert
    implements: "Cache population with auto-resize"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::invalidate_styled_lines
    implements: "Integration point for dirty line notification"
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::clear_styled_line_cache
    implements: "Cache clearing on buffer/tab switch"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::invalidate_styled_lines
    implements: "Renderer API forwarding dirty lines to GlyphBuffer cache"
  - ref: crates/editor/src/renderer/mod.rs#Renderer::clear_styled_line_cache
    implements: "Renderer API for full cache clear on tab switch"
  - ref: crates/editor/src/drain_loop.rs
    implements: "Integration site calling invalidate/clear on each render pass"
  - ref: crates/editor/src/context.rs#EditorContext
    implements: "DirtyLines accumulation during edit operations"
  - ref: crates/editor/src/editor_state.rs#EditorState::take_dirty_lines
    implements: "Dirty line state management for render pass"
  - ref: crates/editor/src/editor_state.rs#EditorState::take_clear_styled_line_cache
    implements: "Clear cache flag management for tab switch detection"
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

Every frame, the renderer calls `styled_line(line_idx)` for every visible line (~40 lines), each allocating a new `StyledLine` containing a `Vec<StyledSpan>`. During typical editing, only 1 line changes per keystroke — yet all 40 are recomputed and reallocated. This wastes ~4µs average and up to 40µs P99 under heap fragmentation.

Introduce a `StyledLineCache` that stores computed `StyledLine` results per buffer line, invalidated by the existing `DirtyLines` tracking. On a typical keystroke, only 1 line is recomputed; the other 39 are served from cache. On scroll, the cache still has lines from the previous viewport that overlap.

**Key files**: New cache struct (likely in `crates/editor/src/` near viewport or glyph_buffer), `crates/editor/src/renderer.rs` (use cache instead of direct styled_line calls), `crates/buffer/src/buffer_view.rs` (DirtyLines already provides invalidation signal)

**Origin**: Architecture review recommendation #3 (P1 — Performance). See `ARCHITECTURE_REVIEW.md`.

## Success Criteria

- `StyledLineCache` caches computed `StyledLine` per buffer line index
- Cache invalidated correctly by `DirtyLines::Single`, `Range`, `FromLineToEnd`
- Unchanged lines serve from cache with zero allocation
- Line insertion/deletion correctly shifts cache entries (or invalidates from point of change)
- Buffer switch / tab change clears or replaces cache
- Measurable: during steady-state typing, heap allocations per frame reduced by ~90% for styled_line path
- No visual artifacts — cache coherence verified by existing rendering tests
- Terminal buffers (which change many lines per PTY read) also benefit when only partial grid updates occur

