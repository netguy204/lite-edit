---
status: ACTIVE
ticket: null
parent_chunk: content_tab_bar
code_paths:
  - crates/editor/src/renderer.rs
code_references:
  - ref: crates/editor/src/renderer.rs#buffer_content_scissor_rect
    implements: "Scissor rect helper to clip buffer content below tab bar"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on: []
created_after:
- scroll_max_last_line
---

# Chunk Goal

## Minor Goal

Buffer text content bleeds upward over the tab bar strip. When the buffer is
scrolled near the top, lines of source text render at the same y-coordinates as
the tab bar, making tab labels illegible.

The root cause is that the buffer glyph rendering pass lacks a Metal scissor
rect bounding it below the tab bar. Without clipping, the GPU renders glyphs
at any y-coordinate the vertex buffer instructs — including rows that fall
inside the tab bar's vertical extent.

The fix is to bracket the buffer text draw call in `renderer.rs` (and any
cursor or gutter draw calls that share its pass) with a scissor rect whose top
edge is set to `tab_bar_height`. The scissor is reset to the full viewport
after the buffer pass completes. No changes are needed to `TabBarGlyphBuffer`,
the buffer model, or scroll geometry.

## Success Criteria

- Buffer text, cursor, and gutter pixels never appear above `tab_bar_height`,
  regardless of scroll position.
- Tab bar labels and close buttons remain fully legible at all times.
- The tab bar rendering pass itself is unaffected — its scissor (or lack
  thereof) does not change.
- No regression in buffer rendering at normal scroll positions.

## Relationship to Parent

`content_tab_bar` introduced the tab bar strip and its `TabBarGlyphBuffer`.
That chunk correctly renders the tab bar but did not add a scissor rect to the
buffer content pass to protect the tab bar region. This chunk adds the missing
clipping guard as a follow-up implementation fix.

## Rejected Ideas

### Adjust the buffer's vertex y-offset to start below the tab bar

The vertex generation could be shifted so no glyphs are emitted above
`tab_bar_height`. Rejected because scissoring is the correct GPU-side
mechanism for this boundary; adjusting vertices would duplicate layout
concerns already handled by viewport geometry and would not protect against
future rendering passes that bypass the offset.