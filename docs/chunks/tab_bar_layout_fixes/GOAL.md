---
status: HISTORICAL
ticket: null
parent_chunk: content_tab_bar
code_paths:
- crates/editor/src/renderer.rs
- crates/editor/src/editor_state.rs
- crates/editor/src/glyph_buffer.rs
code_references:
  - ref: crates/editor/src/glyph_buffer.rs#GlyphBuffer::update_from_buffer_with_cursor
    implements: "Fixed glyph positioning to use quad_vertices_with_xy_offset for proper tab bar/rail offset"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: implementation
depends_on:
- content_tab_bar
created_after:
- agent_lifecycle
- content_tab_bar
- terminal_input_encoding
- find_in_file
- cursor_wrap_scroll_alignment
- row_scroller_extract
---

# Chunk Goal

## Minor Goal

Fix two layout bugs introduced by the `content_tab_bar` chunk:

1. **Buffer text renders over tab bar text.** The tab bar and the buffer content share the same coordinate space without proper layering or clipping. Buffer glyphs are drawn at Y positions that overlap the tab bar strip, causing content to bleed into and obscure the tab labels.

2. **Click targeting is off by ~one line height.** Mouse clicks in the buffer area hit approximately one line height above the intended position. The `handle_mouse()` coordinate transformation subtracts `TAB_BAR_HEIGHT` from the raw Y coordinate when converting to buffer-local coordinates, but the subtraction is either applied incorrectly, applied twice, or the constant used doesn't match the actual rendered height of the tab bar. The result is that the cursor lands one line above where the user clicked.

These are purely corrective fixes to the layout/coordinate accounting introduced in `content_tab_bar`. No new features are added.

## Success Criteria

- Tab bar labels are fully visible and not obscured by buffer text at any scroll position
- Buffer text does not render within the tab bar strip (Y < TAB_BAR_HEIGHT from top of content area)
- Clicking on a line in the buffer moves the cursor to that line, not the line above it
- The click offset error is consistent at zero — not just approximately zero — across the full range of Y positions in the buffer
- Existing tab bar click-to-switch behavior (clicking a tab switches to it) continues to work correctly
- No regression in left rail click handling

## Relationship to Parent

The `content_tab_bar` chunk (parent) introduced the tab bar and shifted the content area down by `TAB_BAR_HEIGHT`. Two coordinate accounting errors crept in during that integration:

- The renderer draws buffer glyphs without enforcing a Y floor at the tab bar boundary, allowing overlap
- The mouse Y coordinate transformation in `EditorState::handle_mouse()` doesn't correctly subtract the tab bar height before converting to buffer-local coordinates (or the constant is mismatched with the actual rendered height)

Everything else from `content_tab_bar` remains valid — tab rendering, click-to-switch, keyboard shortcuts, unread badges.