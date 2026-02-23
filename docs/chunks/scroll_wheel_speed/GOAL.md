---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/metal_view.rs
code_references:
  - ref: crates/editor/src/metal_view.rs#MetalView::DEFAULT_LINE_HEIGHT_PX
    implements: "Line height constant for mouse wheel to pixel delta conversion"
  - ref: crates/editor/src/metal_view.rs#MetalView::convert_scroll_event
    implements: "Device-specific scroll delta handling via hasPreciseScrollingDeltas()"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- unsaved_tab_tint
- cursor_blink_pane_focus
- pane_hover_scroll
---

# Chunk Goal

## Minor Goal

Mouse scroll wheel events produce painfully slow scrolling compared to trackpad scrolling, which feels correct. The root cause is in `convert_scroll_event()` in `metal_view.rs` — it reads `scrollingDeltaX()`/`scrollingDeltaY()` and uses the raw values identically for both input devices. Trackpad events produce precise pixel-level deltas (large values), while discrete mouse wheel events produce line-level deltas (small values like 1.0–3.0). Without distinguishing between the two, mouse wheel scrolling moves only a few pixels per tick.

The fix is to check `NSEvent::hasPreciseScrollingDeltas()` — when `false` (mouse wheel), multiply the delta by the line height to convert line-based deltas into pixel-based deltas that match the scroll system's expectations. When `true` (trackpad), use the delta as-is (current behavior).

This applies to all scroll consumers: buffer views, terminal tabs, selector/picker, and any future scroll targets.

## Success Criteria

- Mouse wheel scrolling moves approximately one line of text per tick (matching typical editor behavior)
- Trackpad scrolling behavior is unchanged from current behavior
- The distinction is made via `hasPreciseScrollingDeltas()` in `convert_scroll_event()`, keeping all downstream scroll handlers device-agnostic
- Scrolling feels responsive with both input devices across buffer tabs, terminal tabs, and the selector/file picker