<!--
This document captures HOW you'll achieve the chunk's GOAL.
It should be specific enough that each step is a reasonable unit of work
to hand to an agent.
-->

# Implementation Plan

## Approach

The goal is to separate invalidation into three distinct kinds (Content, Layout, Overlay) so the renderer can skip work that hasn't changed. Currently all invalidation flows through `DirtyRegion`, and the renderer recomputes pane layout rects on every frame regardless of whether layout actually changed.

**Strategy:**

1. **Replace `DirtyRegion` with `InvalidationKind`**: Create a new enum that distinguishes Content, Layout, and Overlay invalidation kinds. The existing `DirtyRegion` (None, Lines, FullViewport) maps to the content-specific screen region tracking.

2. **Add cached pane rects**: Store the computed pane rects in the renderer and only recompute them when Layout invalidation is signaled. The current `calculate_pane_rects()` call happens unconditionally in `render_with_editor()` — we'll gate it behind a layout-dirty flag.

3. **Signal invalidation kind from event handlers**: Update `drain_loop.rs` event handlers and `EditorState` methods to signal the appropriate invalidation kind:
   - Content: typing, cursor movement, cursor blink, PTY output
   - Layout: resize, split/unsplit, tab bar changes, workspace switch
   - Overlay: find bar toggle, selector toggle, confirm dialog

4. **Conditional layout recalculation in renderer**: Modify `render_with_editor()` and `render_with_confirm_dialog()` to only call `calculate_pane_rects()` when Layout invalidation is present. For Content-only frames, reuse the cached rects.

**Testing approach:** Per TESTING_PHILOSOPHY.md, the invalidation logic is pure state manipulation that can be tested without a GPU. We'll write TDD-style tests for:
- `InvalidationKind` merge semantics (Content + Layout = Layout, etc.)
- Cached pane rect invalidation on Layout signal
- Event handler → invalidation kind mapping

The renderer's conditional behavior is "humble view" code and will be verified by manual QA (visual correctness) plus a measurability test: assert >90% of frames during typing skip pane rect recalculation.

## Subsystem Considerations

- **docs/subsystems/renderer** (DOCUMENTED): This chunk USES the renderer subsystem's `Renderer` struct and modifies its rendering orchestration to conditionally skip pane rect calculation. The change aligns with the subsystem's **Layering Contract** (overlays render on top of editor content) and doesn't violate any hard invariants.

- **docs/subsystems/viewport_scroll** (referenced in `dirty_region.rs` backreference): The existing `DirtyRegion` belongs to this subsystem's scope. We'll preserve backward compatibility by embedding `DirtyRegion` within the new `InvalidationKind::Content` variant rather than replacing it entirely.

## Sequence

### Step 1: Define InvalidationKind enum

Create a new `InvalidationKind` enum in `crates/editor/src/dirty_region.rs` (alongside the existing `DirtyRegion`):

```rust
/// Invalidation category for rendering optimization.
///
/// Different invalidation kinds allow the renderer to skip work:
/// - Content-only frames skip pane rect recalculation
/// - Layout frames trigger full pane rect recomputation
/// - Overlay frames render overlay layer without re-rendering content (future optimization)
// Chunk: docs/chunks/invalidation_separation - Separate invalidation kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvalidationKind {
    /// No invalidation
    #[default]
    None,
    /// Content changed within existing layout (typing, cursor blink, PTY output)
    /// Contains the screen-space dirty region for partial redraw optimization
    Content(DirtyRegion),
    /// Layout changed (pane resize, split/unsplit, tab bar change, workspace switch)
    /// Implies full content re-render after layout recalculation
    Layout,
    /// Overlay changed (find bar, selector, dialog appeared/changed)
    /// Currently treated as Layout for simplicity; future optimization could
    /// render overlay layer only
    Overlay,
}
```

Implement merge semantics:
- `None` is identity
- `Layout` absorbs everything (highest priority)
- `Overlay` absorbs `Content` but yields to `Layout`
- `Content` merges underlying `DirtyRegion` values

Add helper methods: `is_none()`, `is_layout()`, `requires_layout_recalc()`, `content_region()`.

**Tests** (TDD — write before implementation):
- `merge_none_with_content()`: None + Content → Content
- `merge_content_with_layout()`: Content + Layout → Layout
- `merge_layout_with_overlay()`: Layout + Overlay → Layout
- `merge_content_regions()`: Content(Lines{0,5}) + Content(Lines{3,8}) → Content(Lines{0,8})
- `requires_layout_recalc_content()`: Content.requires_layout_recalc() == false
- `requires_layout_recalc_layout()`: Layout.requires_layout_recalc() == true

Location: `crates/editor/src/dirty_region.rs`

---

### Step 2: Add cached pane rects to Renderer

Add a cached pane rects field to the `Renderer` struct:

```rust
// In crates/editor/src/renderer/mod.rs
pub struct Renderer {
    // ... existing fields ...

    // Chunk: docs/chunks/invalidation_separation - Cached pane layout
    /// Cached pane rectangles from the last layout calculation.
    /// Only recomputed when Layout invalidation is signaled.
    cached_pane_rects: Vec<PaneRect>,
    /// Focused pane ID from the last layout calculation.
    cached_focused_pane_id: PaneId,
    /// Whether the cached pane rects are valid (false until first layout)
    pane_rects_valid: bool,
}
```

Initialize `pane_rects_valid = false` in `Renderer::new()`.

Add a method to invalidate the cache:

```rust
/// Marks the cached pane rects as invalid, forcing recalculation on next render.
// Chunk: docs/chunks/invalidation_separation - Layout cache invalidation
pub fn invalidate_pane_layout(&mut self) {
    self.pane_rects_valid = false;
}
```

Location: `crates/editor/src/renderer/mod.rs`

---

### Step 3: Replace dirty_region with invalidation in EditorState

Update `EditorState` to track `InvalidationKind` instead of raw `DirtyRegion`:

```rust
// In crates/editor/src/editor_state.rs
pub struct EditorState {
    // Change from:
    // pub dirty_region: DirtyRegion,
    // To:
    /// Accumulated invalidation for the current event batch
    // Chunk: docs/chunks/invalidation_separation - Invalidation kind tracking
    pub invalidation: InvalidationKind,
    // ... rest unchanged ...
}
```

Update all existing `.dirty_region.merge(DirtyRegion::...)` call sites to use the new invalidation system:

- **Content invalidation** (most common): `self.invalidation.merge(InvalidationKind::Content(DirtyRegion::...))`
- **Layout invalidation**: `self.invalidation.merge(InvalidationKind::Layout)`
- **Overlay invalidation**: `self.invalidation.merge(InvalidationKind::Overlay)`

**Classification of existing call sites:**

| Current call | New invalidation kind | Rationale |
|--------------|----------------------|-----------|
| Resize (`handle_resize`) | Layout | Pane rects change |
| Tab switch | Layout | Tab bar content changes |
| Workspace switch | Layout | Entire layout changes |
| Split/unsplit (`move_active_tab`) | Layout | Pane structure changes |
| Focus switch (`switch_focus`) | Layout | Focus border changes (could be Content if we track focus border separately) |
| Selector open/close | Overlay | Overlay layer changes |
| Find bar open/close | Overlay | Overlay layer changes |
| Confirm dialog | Overlay | Overlay layer changes |
| Cursor blink | Content | Glyph changes only |
| PTY wakeup | Content | Terminal content changes |
| Key input (typing) | Content | Buffer content changes |

Update helper methods:
- `is_dirty()` → checks `!self.invalidation.is_none()`
- `take_dirty_region()` → becomes `take_invalidation()` returning `InvalidationKind`
- `mark_full_dirty()` → `self.invalidation = InvalidationKind::Layout`

Location: `crates/editor/src/editor_state.rs`

---

### Step 4: Update drain_loop to pass InvalidationKind to renderer

Modify `render_if_dirty()` in `drain_loop.rs`:

```rust
fn render_if_dirty(&mut self) {
    // Update window title if needed
    self.update_window_title_if_needed();

    if self.state.is_dirty() {
        // Take the invalidation kind
        let invalidation = self.state.take_invalidation();

        // Chunk: docs/chunks/invalidation_separation - Conditional layout invalidation
        // Tell the renderer whether it needs to recalculate pane layout
        if invalidation.requires_layout_recalc() {
            self.renderer.invalidate_pane_layout();
        }

        // ... rest of existing render logic ...
        // The renderer will check its pane_rects_valid flag internally
    }
}
```

Location: `crates/editor/src/drain_loop.rs`

---

### Step 5: Conditional pane rect calculation in renderer

Modify `render_with_editor()` and `render_with_confirm_dialog()` to use cached pane rects:

```rust
// In render_with_editor(), replace:
//   pane_rects = calculate_pane_rects(bounds, &ws.pane_root);
//   focused_pane_id = ws.active_pane_id;
// With:

// Chunk: docs/chunks/invalidation_separation - Conditional pane rect calculation
if !self.pane_rects_valid {
    // Layout invalidation or first render: recompute pane rects
    let bounds = (
        RAIL_WIDTH,
        0.0,
        view_width - RAIL_WIDTH,
        view_height,
    );
    self.cached_pane_rects = calculate_pane_rects(bounds, &ws.pane_root);
    self.cached_focused_pane_id = ws.active_pane_id;
    self.pane_rects_valid = true;
}
let pane_rects = &self.cached_pane_rects;
let focused_pane_id = self.cached_focused_pane_id;
```

**Important**: The cache must also be invalidated when:
- Viewport size changes (`update_viewport_size()`) — already captured by Layout invalidation from resize
- Active pane focus changes — need to track `active_pane_id` changes

Add a check for focus changes in the render path:

```rust
// Focus border needs redraw if active pane changed
if ws.active_pane_id != self.cached_focused_pane_id {
    self.cached_focused_pane_id = ws.active_pane_id;
    // Focus change requires redrawing pane frames but not recalculating rects
    // This is handled by the rendering loop already
}
```

Location: `crates/editor/src/renderer/mod.rs`

---

### Step 6: Update tests and add new tests

**Update existing tests:**
- Any test that references `dirty_region` needs to be updated to use `invalidation`
- Tests in `dirty_region.rs` remain unchanged (they test `DirtyRegion` directly)

**Add new tests for InvalidationKind** (in `crates/editor/src/dirty_region.rs`):

```rust
#[cfg(test)]
mod invalidation_tests {
    use super::*;

    #[test]
    fn merge_none_with_content() {
        let mut inv = InvalidationKind::None;
        inv.merge(InvalidationKind::Content(DirtyRegion::single_line(5)));
        assert!(matches!(inv, InvalidationKind::Content(_)));
    }

    #[test]
    fn merge_content_with_layout() {
        let mut inv = InvalidationKind::Content(DirtyRegion::single_line(5));
        inv.merge(InvalidationKind::Layout);
        assert_eq!(inv, InvalidationKind::Layout);
    }

    #[test]
    fn merge_layout_absorbs_all() {
        let mut inv = InvalidationKind::Layout;
        inv.merge(InvalidationKind::Content(DirtyRegion::FullViewport));
        inv.merge(InvalidationKind::Overlay);
        assert_eq!(inv, InvalidationKind::Layout);
    }

    #[test]
    fn requires_layout_recalc() {
        assert!(!InvalidationKind::None.requires_layout_recalc());
        assert!(!InvalidationKind::Content(DirtyRegion::FullViewport).requires_layout_recalc());
        assert!(InvalidationKind::Layout.requires_layout_recalc());
        assert!(!InvalidationKind::Overlay.requires_layout_recalc()); // Overlay doesn't require layout recalc
    }

    #[test]
    fn content_region_extraction() {
        let inv = InvalidationKind::Content(DirtyRegion::Lines { from: 3, to: 7 });
        assert_eq!(inv.content_region(), Some(DirtyRegion::Lines { from: 3, to: 7 }));

        let layout = InvalidationKind::Layout;
        assert_eq!(layout.content_region(), None);
    }
}
```

Location: `crates/editor/src/dirty_region.rs`

---

### Step 7: Measurability verification

Add a debug/instrumentation counter to verify that pane rect calculation is being skipped:

```rust
// In Renderer (only with perf-instrumentation feature)
#[cfg(feature = "perf-instrumentation")]
layout_recalc_skipped: usize,
#[cfg(feature = "perf-instrumentation")]
layout_recalc_performed: usize,
```

Increment these counters in `render_with_editor()`:

```rust
#[cfg(feature = "perf-instrumentation")]
if self.pane_rects_valid {
    self.layout_recalc_skipped += 1;
} else {
    self.layout_recalc_performed += 1;
}
```

Add a method to report the skip rate:

```rust
#[cfg(feature = "perf-instrumentation")]
pub fn layout_skip_rate(&self) -> f64 {
    let total = self.layout_recalc_skipped + self.layout_recalc_performed;
    if total == 0 { 0.0 } else { self.layout_recalc_skipped as f64 / total as f64 }
}
```

Include this in the perf stats report. Success criteria: >90% skip rate during normal editing.

Location: `crates/editor/src/renderer/mod.rs`

## Dependencies

None. This chunk builds on the existing `DirtyRegion` infrastructure and the recently decomposed `renderer/` module structure from the `renderer_decomposition` chunk.

## Risks and Open Questions

1. **Stale pane rects after focus change**: If the user switches pane focus (Ctrl+W arrow) without triggering Layout invalidation, the `cached_focused_pane_id` will be stale. Mitigated by: detecting focus ID mismatch in render and updating the focus border rendering without full pane rect recalculation.

2. **Viewport size change detection**: The viewport size change from resize events already triggers Layout invalidation via `handle_resize()`. However, if view dimensions change through another path (e.g., display scale change), the cache could become stale. Mitigated by: `update_viewport_size()` should also set `pane_rects_valid = false`.

3. **Multi-pane tab bar changes**: Opening/closing tabs changes the tab bar appearance. Currently this triggers `FullViewport` dirty. The tab bar is rendered per-pane in multi-pane mode, but the pane rects themselves don't change. This is a Content invalidation, not Layout. We may need to distinguish "tab bar content changed" from "pane structure changed."

4. **Overlay invalidation optimization not implemented**: The goal mentions "Overlay invalidation renders overlay layer without re-rendering underlying content (where possible)". This PLAN treats Overlay as equivalent to Content for now (it doesn't require layout recalc). The full optimization of skipping underlying content rendering is deferred — it requires tracking whether content has changed independently of overlay state, which adds complexity.

5. **Breaking change to EditorState API**: Changing `dirty_region: DirtyRegion` to `invalidation: InvalidationKind` is a breaking change for any code that directly accesses `state.dirty_region`. A search shows this field is accessed in `drain_loop.rs` and internally in `EditorState`. The change is contained to the `editor` crate.

## Deviations

(To be populated during implementation)