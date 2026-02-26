---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
  - crates/editor/src/renderer/panes.rs
  - crates/editor/src/viewport.rs
  - crates/editor/src/row_scroller.rs
  - crates/editor/src/drain_loop.rs
code_references:
  - ref: crates/editor/src/renderer/panes.rs#Renderer::configure_viewport_for_pane
    implements: "Per-pane viewport configuration - copies scroll state from tab and updates dimensions"
  - ref: crates/editor/src/viewport.rs#Viewport::set_visible_lines
    implements: "Direct visible line count setter without re-clamping scroll offset"
  - ref: crates/editor/src/row_scroller.rs#RowScroller::set_visible_rows
    implements: "Direct visible row count setter without re-clamping scroll offset"
  - ref: crates/editor/src/drain_loop.rs#DrainLoop::render_if_dirty
    implements: "Removed stale viewport sync - panes now configure viewport at render time"
narrative: null
investigation: null
subsystems:
- subsystem_id: viewport_scroll
  relationship: implements
friction_entries: []
bug_type: semantic
depends_on: []
created_after:
- clipboard_cut
- event_channel_waker
---

# Chunk Goal

## Minor Goal

Give each pane fully independent scroll state so that scrolling, typing, or
resizing in one pane never affects the visible content or cursor position of
another pane.

Today the `Renderer` owns a single `Viewport` instance that is reused when
drawing every pane. Because the renderer's viewport is only synchronized with
the *focused* tab's viewport, all non-focused panes render from the wrong
scroll offset. This produces three observable symptoms:

1. **Coupled scrolling** — scrolling in one pane scrolls the other pane too
   (both vertical and horizontal splits).
2. **Viewport height clamping** — the scroll range appears locked to the
   height of the smallest pane, because the renderer's `visible_lines` is
   computed once for the whole window rather than per-pane.
3. **Line-position jitter** — typing new lines in the focused pane causes
   unfocused panes to jump, because the shared scroll offset changes on
   every keystroke that triggers `ensure_visible`.
4. **Stale soft-wrap geometry** — once a second pane has existed, the
   renderer's viewport retains the wrong width for wrap calculations.
   Soft-wrap line breaks appear at the wrong column and the max scroll
   position is incorrect. This persists even after collapsing back to a
   single pane, because the renderer's viewport dimensions were never
   re-synchronized to the actual pane width.

## Success Criteria

- Open two panes (vertical or horizontal split) with different files. Scrolling
  in one pane does not move the content of the other pane.
- Each pane scrolls through its full buffer length regardless of the other
  pane's height.
- Typing in the focused pane (including adding/removing lines) does not change
  the first visible line in any unfocused pane.
- Soft-wrap line breaks respect each pane's actual width, not the window width
  or a stale width from a previous layout. This holds after splitting and after
  collapsing back to a single pane.
- The max scroll position accounts for wrapped lines using the correct pane
  width, both during and after splits.
- The renderer's per-pane draw path uses the active tab's `Viewport` (scroll
  offset, visible-lines count, and wrap width) rather than a single shared
  `Viewport`.
- All existing tests pass.


